use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use nxdesk_core::{
    AuthRequest, AuthResponse, InputEvent, ListDirectoryResponse, ScreenInfoDto, SessionRole,
    SignalingMessage,
};
use nxdesk_file_transfer::list_directory_async;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

pub async fn start_web_server(
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let initial_port = {
        let cfg = state.config.read().await;
        cfg.web_port
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/health", get(health_handler))
        .route("/api/status", get(get_status))
        .route("/api/peers", get(get_peers))
        .route("/api/files", get(get_files))
        .route("/api/auth", post(post_auth))
        .route("/api/screens", get(get_screens))
        .route("/api/screens/select", post(post_select_screen))
        .route("/api/connect", post(post_connect))
        .route("/api/disconnect", post(post_disconnect))
        .route("/api/terminal/auth", post(post_terminal_auth))
        .route("/api/host/credentials", get(get_host_credentials))
        .route("/api/input", post(post_input))
        .route("/ws/stream", get(ws_handler))
        .with_state(state.clone());

    // Detección y notificación explícita de colisión de puertos
    let mut bound_port = initial_port;
    let mut listener_opt = None;

    for p in initial_port..initial_port + 50 {
        let addr = SocketAddr::from(([0, 0, 0, 0], p));
        match TcpListener::bind(addr).await {
            Ok(l) => {
                bound_port = p;
                listener_opt = Some(l);
                break;
            }
            Err(_) => continue,
        }
    }

    let listener = listener_opt.ok_or_else(|| {
        format!(
            "No se pudo enlazar ningún puerto web entre {} y {}",
            initial_port,
            initial_port + 50
        )
    })?;

    if bound_port != initial_port {
        tracing::warn!(
            "⚠️  [COLISIÓN DE PUERTO] El puerto original {} estaba ocupado por otro proceso.",
            initial_port
        );
        tracing::warn!(
            "⚠️  NxDesk Dashboard se reubicó automáticamente en: http://localhost:{}",
            bound_port
        );
    }

    // Actualizar el puerto asignado en la configuración
    {
        let mut cfg = state.config.write().await;
        cfg.web_port = bound_port;
    }

    info!("==================================================");
    info!("   ✨  NxDesk Web Dashboard (Pure Rust UI)        ");
    info!("   Acceso Local: http://localhost:{}", bound_port);
    info!("   Acceso Red:   http://0.0.0.0:{}", bound_port);
    if bound_port != initial_port {
        info!(
            "   (Puerto original {} ocupado por otro proceso)",
            initial_port
        );
    }
    info!("==================================================");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            error!("Error en servidor web NxDesk: {}", e);
        }
    });

    Ok(())
}

#[derive(Serialize)]
struct StatusResponse {
    device_id: String,
    device_alias: String,
    web_port: u16,
    signaling_connected: bool,
    discovered_peers_count: usize,
    connected_viewers_count: usize,
    active_room_users: usize,
    active_remote_peer: Option<String>,
    allow_remote_terminal: bool,
    max_fps: u32,
    quality: u32,
}

async fn health_handler() -> &'static str {
    "{\"status\":\"healthy\",\"service\":\"nxdesk-client\"}"
}

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let cfg = state.config.read().await;
    let peers_count = state.discovered_peers.read().await.len();
    let viewers = state.get_connected_viewers();
    let room_users = state.get_room_occupancy();
    let remote_peer = state.active_remote_peer.read().await.clone();

    Json(StatusResponse {
        device_id: cfg.device_id.clone(),
        device_alias: cfg.device_alias.clone(),
        web_port: cfg.web_port,
        signaling_connected: state.is_signaling_connected(),
        discovered_peers_count: peers_count,
        connected_viewers_count: viewers,
        active_room_users: room_users,
        active_remote_peer: remote_peer,
        allow_remote_terminal: cfg.allow_remote_terminal,
        max_fps: cfg.max_fps,
        quality: cfg.quality,
    })
}

async fn get_peers(State(state): State<AppState>) -> Json<Vec<nxdesk_core::PeerIdentity>> {
    let peers = state.get_discovered_peers().await;
    Json(peers)
}

#[derive(Deserialize)]
struct FilesQuery {
    path: Option<String>,
    token: Option<String>,
}

async fn get_files(
    State(state): State<AppState>,
    Query(query): Query<FilesQuery>,
) -> Json<ListDirectoryResponse> {
    let cfg = state.config.read().await;
    if cfg.access_password.is_some() {
        let token = query.token.as_deref().unwrap_or_default();
        if !state.auth.has_role(token, SessionRole::FullControl).await
            && !state.auth.has_role(token, SessionRole::Administrator).await
        {
            return Json(ListDirectoryResponse {
                current_path: String::new(),
                items: Vec::new(),
                error: Some("Acceso denegado: se requiere autorización de anfitrión o control total para explorar archivos.".to_string()),
            });
        }
    }
    drop(cfg);

    let path = query.path.unwrap_or_default();
    if path.contains("..") {
        return Json(ListDirectoryResponse {
            current_path: path,
            items: Vec::new(),
            error: Some(
                "Ataque de Path Traversal bloqueado: secuencias '..' prohibidas.".to_string(),
            ),
        });
    }

    let resp = list_directory_async(&path).await;
    Json(resp)
}

async fn post_auth(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<AuthRequest>,
) -> Json<AuthResponse> {
    // Rate limit: máximo 5 intentos por cada 10 segundos por dirección IP
    if !state
        .rate_limiter
        .check(addr.ip(), 5, std::time::Duration::from_secs(10))
        .await
    {
        return Json(AuthResponse {
            success: false,
            error_message: Some(
                "Demasiados intentos de autenticación. Bloqueado temporalmente por seguridad."
                    .to_string(),
            ),
            session_token: None,
            granted_role: SessionRole::ViewOnly,
        });
    }

    let resp = state.auth.authenticate(&payload).await;
    Json(resp)
}

/// Obtiene la lista de monitores detectados (Pantalla 1, Pantalla 2, Doble Pantalla)
async fn get_screens(State(state): State<AppState>) -> Json<Vec<ScreenInfoDto>> {
    let screens = state.get_screens().await;
    Json(screens)
}

#[derive(Deserialize)]
struct SelectScreenPayload {
    index: i32,
}

/// Cambia el monitor activo que se está capturando y retransmite
async fn post_select_screen(
    State(state): State<AppState>,
    Json(payload): Json<SelectScreenPayload>,
) -> Json<Result<ScreenInfoDto, String>> {
    let res = state.select_screen(payload.index).await;
    Json(res)
}

#[derive(Deserialize)]
struct InputPayload {
    event: InputEvent,
}

async fn post_input(
    State(state): State<AppState>,
    Json(payload): Json<InputPayload>,
) -> &'static str {
    let injector = state.input_injector.lock().await;
    injector.inject(&payload.event);
    "OK"
}

#[derive(Deserialize)]
struct ConnectPayload {
    target: String,
}

#[derive(Serialize)]
struct ConnectResponse {
    success: bool,
    message: String,
    target: String,
}

/// Conecta el cliente a una sesión remota (ID NxDesk o IP directa)
async fn post_connect(
    State(state): State<AppState>,
    Json(payload): Json<ConnectPayload>,
) -> Json<ConnectResponse> {
    let target = payload.target.trim().to_string();
    if target.is_empty() {
        return Json(ConnectResponse {
            success: false,
            message: "El ID o IP objetivo no puede estar vacío.".to_string(),
            target,
        });
    }

    if let Some(client) = state.signaling_client.read().await.clone() {
        match client.join_room(&target).await {
            Ok(_) => {
                state.set_active_remote_peer(Some(target.clone())).await;
                info!(
                    "🚀 [NxDesk Connect] Conectado exitosamente a la sala del equipo remoto '{}'",
                    target
                );
                Json(ConnectResponse {
                    success: true,
                    message: format!("Conectado exitosamente a la sala remota '{}'", target),
                    target,
                })
            }
            Err(e) => {
                // Fallback automático para conexión directa LAN si el objetivo es IP o no hay sala central
                state.set_active_remote_peer(Some(target.clone())).await;
                info!("🌐 [NxDesk Connect] Conexión configurada en modo directo LAN a '{}' (Aviso de sala: {})", target, e);
                Json(ConnectResponse {
                    success: true,
                    message: format!(
                        "Objetivo configurado para conexión directa LAN al nodo '{}'",
                        target
                    ),
                    target,
                })
            }
        }
    } else {
        state.set_active_remote_peer(Some(target.clone())).await;
        Json(ConnectResponse {
            success: true,
            message: format!("Objetivo configurado para conexión directa LAN: {}", target),
            target,
        })
    }
}

/// Finaliza la sesión remota activa y vuelve al modo anfitrión local
async fn post_disconnect(State(state): State<AppState>) -> Json<ConnectResponse> {
    let prev = state.get_active_remote_peer().await;
    state.set_active_remote_peer(None).await;
    info!("🔌 [Web Disconnect] Desconectado de sesión remota.");
    Json(ConnectResponse {
        success: true,
        message: "Sesión remota finalizada.".to_string(),
        target: prev.unwrap_or_default(),
    })
}

#[derive(Deserialize)]
struct TerminalAuthPayload {
    password: Option<String>,
    enable: Option<bool>,
}

#[derive(Serialize)]
struct TerminalAuthResponse {
    success: bool,
    allowed: bool,
    message: String,
}

#[derive(Serialize)]
struct HostCredentialsResponse {
    is_localhost: bool,
    host_password: Option<String>,
}

/// Endpoint seguro: solo entrega la clave al anfitrión local validado por socket loopback
async fn get_host_credentials(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Json<HostCredentialsResponse> {
    let is_localhost = addr.ip().is_loopback();
    let cfg = state.config.read().await;
    Json(HostCredentialsResponse {
        is_localhost,
        host_password: if is_localhost {
            cfg.access_password.clone()
        } else {
            None
        },
    })
}

/// Autoriza o revoca el uso de la terminal remota en caliente (VUL-01)
async fn post_terminal_auth(
    State(state): State<AppState>,
    Json(payload): Json<TerminalAuthPayload>,
) -> Json<TerminalAuthResponse> {
    let enable = payload.enable.unwrap_or(true);

    if enable {
        let cfg = state.config.read().await;
        let expected_pw = match cfg.access_password.as_deref() {
            Some(pw) => pw,
            None => {
                return Json(TerminalAuthResponse {
                    success: false,
                    allowed: false,
                    message: "Desbloqueo denegado: El anfitrión no ha configurado una contraseña administrativa (--password=<clave>). Por seguridad, la terminal permanece bloqueada.".to_string(),
                });
            }
        };
        let provided = payload.password.as_deref().unwrap_or_default();
        if provided != expected_pw {
            return Json(TerminalAuthResponse {
                success: false,
                allowed: false,
                message: "Contraseña maestra de terminal incorrecta. Solo el anfitrión autorizado puede habilitar la terminal.".to_string(),
            });
        }
    }

    state.set_remote_terminal_allowed(enable).await;
    let msg = if enable {
        "🔓 Terminal Remota desbloqueada y autorizada para ejecución (VUL-01 verificada)."
    } else {
        "🔒 Terminal Remota bloqueada (VUL-01 activa)."
    };
    info!("🛡️ [Terminal Auth] {}", msg);
    let _ = state
        .terminal_broadcast_tx
        .send(format!("\r\n📢 [NxDesk Seguridad] {}\r\n\r\n", msg));

    Json(TerminalAuthResponse {
        success: true,
        allowed: enable,
        message: msg.to_string(),
    })
}

#[derive(Deserialize, Default)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Rate limit: máximo 20 conexiones WebSocket cada 10 segundos por dirección IP
    if !state
        .rate_limiter
        .check(addr.ip(), 20, std::time::Duration::from_secs(10))
        .await
    {
        return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    let initial_token = query.token;
    ws.on_upgrade(move |socket| handle_web_socket(socket, state, addr, initial_token))
        .into_response()
}

async fn handle_web_socket(
    socket: WebSocket,
    state: AppState,
    client_addr: SocketAddr,
    initial_token: Option<String>,
) {
    state.increment_connected_viewers();
    let v_count = state.get_connected_viewers();
    info!(
        "👥 [Web] Nuevo visor web conectado desde {}. Espectadores activos: {}",
        client_addr, v_count
    );

    let is_loopback = client_addr.ip().is_loopback();
    let access_password_required = {
        let cfg = state.config.read().await;
        cfg.access_password.is_some()
    };

    let initial_auth = if !access_password_required || is_loopback {
        true
    } else if let Some(ref tok) = initial_token {
        state.auth.has_role(tok, SessionRole::FullControl).await
            || state.auth.has_role(tok, SessionRole::Administrator).await
    } else {
        false
    };

    let is_authorized = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(initial_auth));
    let is_authorized_clone = is_authorized.clone();

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut frame_rx = state.frame_broadcast_tx.subscribe();
    let mut term_rx = state.terminal_broadcast_tx.subscribe();

    // Enviar último fotograma si cuenta con autorización
    if initial_auth {
        if let Some(initial_frame) = state.latest_jpeg_frame.read().await.clone() {
            let _ = ws_sender.send(Message::Binary(initial_frame.into())).await;
        }
    } else {
        let auth_req_msg = serde_json::json!({
            "type": "auth_required",
            "message": "Autenticación requerida para visualizar y controlar la pantalla remota."
        });
        let _ = ws_sender
            .send(Message::Text(auth_req_msg.to_string().into()))
            .await;
    }

    // Tarea para enviar fotogramas y mensajes al cliente web
    let is_auth_send = is_authorized.clone();
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                frame_res = frame_rx.recv() => {
                    match frame_res {
                        Ok(frame_bytes) => {
                            if is_auth_send.load(std::sync::atomic::Ordering::Relaxed)
                                && ws_sender.send(Message::Binary(frame_bytes.into())).await.is_err()
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                term_res = term_rx.recv() => {
                    match term_res {
                        Ok(term_msg) => {
                            if is_auth_send.load(std::sync::atomic::Ordering::Relaxed) {
                                let json = format!("{{\"type\":\"terminal\",\"data\":{}}}", serde_json::to_string(&term_msg).unwrap_or_default());
                                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    // Tarea para recibir comandos del navegador web
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Validar si es mensaje de autenticación dinámica en caliente
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if val.get("type").and_then(|v| v.as_str()) == Some("auth") {
                            if let Some(token) = val.get("token").and_then(|v| v.as_str()) {
                                if state_clone
                                    .auth
                                    .has_role(token, SessionRole::FullControl)
                                    .await
                                    || state_clone
                                        .auth
                                        .has_role(token, SessionRole::Administrator)
                                        .await
                                {
                                    is_authorized_clone
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                    info!("🔐 [Web Auth] Cliente {} autenticado exitosamente por WebSocket.", client_addr);
                                    if let Some(initial_frame) =
                                        state_clone.latest_jpeg_frame.read().await.clone()
                                    {
                                        let _ = state_clone.frame_broadcast_tx.send(initial_frame);
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    if let Ok(evt) = serde_json::from_str::<InputEvent>(&text) {
                        // Verificación estricta de autorización para inyección de periféricos (SEC-02)
                        if is_authorized_clone.load(std::sync::atomic::Ordering::Relaxed) {
                            let injector = state_clone.input_injector.lock().await;
                            injector.inject(&evt);
                        } else {
                            warn!("🛡️ [Seguridad] Entrada de ratón/teclado denegada desde {}: cliente sin rol de Control Remoto autenticado.", client_addr);
                        }
                    } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(cmd) = val.get("terminal_cmd").and_then(|v| v.as_str()) {
                            if !is_authorized_clone.load(std::sync::atomic::Ordering::Relaxed) {
                                let warn_msg = "\r\n⚠️ [Seguridad] Comando de terminal rechazado: cliente no autenticado.\r\n";
                                let _ =
                                    state_clone.terminal_broadcast_tx.send(warn_msg.to_string());
                                continue;
                            }

                            let remote_peer = state_clone.get_active_remote_peer().await;
                            let sig_client = state_clone.get_signaling_client().await;

                            if let (Some(_peer), Some(signaling)) = (remote_peer, sig_client) {
                                info!("📡 [Web Terminal] Reenviando comando '{}' a equipo remoto vía Señalización.", cmd);
                                let sig_msg = SignalingMessage::new("terminal_cmd", cmd);
                                if let Err(e) = signaling.send_message(&sig_msg).await {
                                    let err_msg = format!(
                                        "\r\n❌ Error al enviar comando al host remoto: {}\r\n",
                                        e
                                    );
                                    let _ = state_clone.terminal_broadcast_tx.send(err_msg);
                                }
                            } else {
                                let allow = state_clone.is_remote_terminal_allowed().await;
                                if allow {
                                    if !state_clone.terminal_session.is_running() {
                                        if let Ok(mut rx) =
                                            state_clone.terminal_session.start().await
                                        {
                                            let term_tx = state_clone.terminal_broadcast_tx.clone();
                                            tokio::spawn(async move {
                                                while let Some(chunk) = rx.recv().await {
                                                    let _ = term_tx.send(chunk);
                                                }
                                            });
                                        }
                                    }
                                    if let Err(e) =
                                        state_clone.terminal_session.write_input(cmd).await
                                    {
                                        let err_msg = format!(
                                            "\r\n❌ Error al escribir en terminal local: {}\r\n",
                                            e
                                        );
                                        let _ = state_clone.terminal_broadcast_tx.send(err_msg);
                                    }
                                } else {
                                    let warn_msg = "\r\n⚠️ [VUL-01: Terminal Bloqueada] La ejecución de comandos remotos requiere autorización.\r\nPara autorizar la terminal, presione el botón '🔓 Desbloquear' e ingrese la clave maestra del anfitrión.\r\n\r\n";
                                    let _ = state_clone
                                        .terminal_broadcast_tx
                                        .send(warn_msg.to_string());
                                }
                            }
                        } else if let Some(idx) = val.get("select_screen").and_then(|v| v.as_i64())
                        {
                            if is_authorized_clone.load(std::sync::atomic::Ordering::Relaxed) {
                                info!("🖥️ [Web] Cambiando a monitor index: {}", idx);
                                let _ = state_clone.select_screen(idx as i32).await;
                            }
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.decrement_connected_viewers();
    let v_rem = state.get_connected_viewers();
    info!(
        "👥 [Web] Visor web desconectado. Espectadores restantes: {}",
        v_rem
    );
}

async fn serve_index(
    State(state): State<AppState>,
) -> ([(axum::http::HeaderName, &'static str); 2], Html<String>) {
    let cfg = state.config.read().await;
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="es">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>NxDesk — Escritorio Remoto Seguro (Pure Rust)</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
    <style>
        :root {{
            --bg-base: #0a0e17;
            --bg-surface: #111827;
            --bg-card: #1f2937;
            --border: #374151;
            --primary: #3b82f6;
            --primary-glow: rgba(59, 130, 246, 0.35);
            --accent: #10b981;
            --danger: #ef4444;
            --warning: #f59e0b;
            --text-main: #f9fafb;
            --text-muted: #9ca3af;
        }}
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            background: var(--bg-base);
            color: var(--text-main);
            font-family: 'Outfit', sans-serif;
            display: flex;
            flex-direction: column;
            height: 100vh;
            overflow: hidden;
        }}
        header {{
            background: rgba(17, 24, 39, 0.85);
            backdrop-filter: blur(12px);
            border-bottom: 1px solid var(--border);
            padding: 12px 24px;
            display: flex;
            align-items: center;
            justify-content: space-between;
            z-index: 50;
        }}
        .brand {{
            display: flex;
            align-items: center;
            gap: 12px;
            font-size: 1.25rem;
            font-weight: 700;
            background: linear-gradient(135deg, #60a5fa, #34d399);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }}
        .brand span.badge {{
            font-size: 0.7rem;
            font-weight: 600;
            background: rgba(59, 130, 246, 0.2);
            color: #60a5fa;
            border: 1px solid rgba(59, 130, 246, 0.4);
            padding: 2px 8px;
            border-radius: 9999px;
            -webkit-text-fill-color: #60a5fa;
        }}
        .header-meta {{
            display: flex;
            align-items: center;
            gap: 20px;
        }}
        .status-pill {{
            display: flex;
            align-items: center;
            gap: 8px;
            background: rgba(16, 185, 129, 0.1);
            color: var(--accent);
            border: 1px solid rgba(16, 185, 129, 0.3);
            padding: 4px 12px;
            border-radius: 9999px;
            font-size: 0.85rem;
            font-weight: 500;
        }}
        .status-dot {{
            width: 8px;
            height: 8px;
            background: var(--accent);
            border-radius: 50%;
            box-shadow: 0 0 8px var(--accent);
            animation: pulse 2s infinite;
        }}
        @keyframes pulse {{
            0%, 100% {{ opacity: 1; transform: scale(1); }}
            50% {{ opacity: 0.4; transform: scale(0.85); }}
        }}
        nav.tabs {{
            display: flex;
            gap: 8px;
            background: var(--bg-surface);
            padding: 8px 24px;
            border-bottom: 1px solid var(--border);
        }}
        .tab-btn {{
            background: transparent;
            border: none;
            color: var(--text-muted);
            padding: 8px 16px;
            font-size: 0.9rem;
            font-weight: 500;
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.2s;
            display: flex;
            align-items: center;
            gap: 8px;
        }}
        .tab-btn:hover {{
            background: rgba(255, 255, 255, 0.05);
            color: var(--text-main);
        }}
        .tab-btn.active {{
            background: var(--primary);
            color: #ffffff;
            box-shadow: 0 0 16px var(--primary-glow);
        }}
        main {{
            flex: 1;
            position: relative;
            overflow: hidden;
            display: flex;
            min-height: 0;
            height: calc(100vh - 65px);
        }}
        .view-panel {{
            display: none;
            width: 100%;
            height: 100%;
            flex-direction: column;
            min-height: 0;
            overflow: hidden;
        }}
        .view-panel.active {{
            display: flex;
        }}
        /* Visor de Pantalla */
        #viewport-container {{
            flex: 1;
            position: relative;
            background: #000;
            display: flex;
            align-items: center;
            justify-content: center;
            overflow: auto;
        }}
        #viewport {{
            max-width: 100%;
            max-height: 100%;
            object-fit: contain;
            cursor: default;
            box-shadow: 0 0 32px rgba(0, 0, 0, 0.8);
            transition: cursor 0.2s;
        }}
        #viewport.control-active {{
            cursor: crosshair;
        }}
        .viewport-toolbar {{
            position: absolute;
            top: 16px;
            right: 16px;
            display: flex;
            align-items: center;
            gap: 10px;
            background: rgba(17, 24, 39, 0.9);
            backdrop-filter: blur(12px);
            padding: 8px 16px;
            border-radius: 10px;
            border: 1px solid var(--border);
            z-index: 10;
            box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
        }}
        .btn-exit-fullscreen-float {{
            display: none;
            position: fixed;
            top: 20px;
            left: 50%;
            transform: translateX(-50%);
            z-index: 999999;
            background: rgba(220, 38, 38, 0.95);
            color: #ffffff;
            font-weight: 700;
            padding: 10px 24px;
            border-radius: 9999px;
            box-shadow: 0 8px 30px rgba(0, 0, 0, 0.8), 0 0 15px rgba(239, 68, 68, 0.6);
            cursor: pointer;
            border: 2px solid rgba(255, 255, 255, 0.8);
            font-family: inherit;
            font-size: 0.95rem;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            backdrop-filter: blur(8px);
        }}
        .btn-exit-fullscreen-float:hover {{
            background: #b91c1c;
            transform: translateX(-50%) scale(1.05);
            box-shadow: 0 10px 35px rgba(0, 0, 0, 0.9), 0 0 25px rgba(239, 68, 68, 0.9);
        }}
        .btn {{
            background: var(--bg-card);
            color: var(--text-main);
            border: 1px solid var(--border);
            padding: 6px 12px;
            border-radius: 6px;
            font-size: 0.85rem;
            cursor: pointer;
            transition: all 0.2s;
            display: inline-flex;
            align-items: center;
            gap: 6px;
            font-family: inherit;
        }}
        .btn:hover {{ background: #2d3748; border-color: #4b5563; }}
        .btn-primary {{
            background: var(--primary);
            border-color: var(--primary);
        }}
        .btn-primary:hover {{ background: #2563eb; }}
        .btn-danger {{
            background: #dc2626;
            color: #ffffff;
            border-color: #ef4444;
        }}
        .btn-danger:hover {{ background: #b91c1c; }}
        .btn-mode-view {{
            background: rgba(255, 255, 255, 0.05);
            color: var(--text-muted);
            border-color: var(--border);
        }}
        .btn-mode-control {{
            background: #059669;
            color: #ffffff;
            border-color: #10b981;
            box-shadow: 0 0 12px rgba(16, 185, 129, 0.4);
            font-weight: 600;
        }}
        .btn-mode-control:hover {{
            background: #047857;
        }}
        .search-container {{
            display: flex;
            align-items: center;
            gap: 8px;
            background: rgba(31, 41, 55, 0.8);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 4px 8px;
        }}
        .search-container input {{
            background: transparent;
            border: none;
            color: #f9fafb;
            font-family: inherit;
            font-size: 0.9rem;
            outline: none;
            width: 320px;
        }}
        .screen-select-dropdown {{
            background: #1f2937;
            color: #f9fafb;
            border: 1px solid #4b5563;
            border-radius: 6px;
            padding: 6px 10px;
            font-size: 0.85rem;
            font-family: inherit;
            outline: none;
            cursor: pointer;
        }}
        .screen-select-dropdown:hover {{
            border-color: #60a5fa;
        }}
        .notice-banner {{
            display: none;
            position: absolute;
            top: 70px;
            left: 50%;
            transform: translateX(-50%);
            background: rgba(245, 158, 11, 0.95);
            color: #111827;
            padding: 8px 18px;
            border-radius: 8px;
            font-size: 0.85rem;
            font-weight: 600;
            z-index: 15;
            box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
            text-align: center;
        }}
        /* Panel Terminal */
        .terminal-toolbar {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            flex-wrap: wrap;
            gap: 10px;
            background: #111827;
            padding: 8px 14px;
            border-bottom: 1px solid #1f2937;
        }}
        .terminal-status-group {{
            display: flex;
            align-items: center;
            gap: 8px;
        }}
        .term-badge {{
            font-size: 0.75rem;
            padding: 3px 8px;
            border-radius: 4px;
            font-weight: 600;
        }}
        .term-badge-locked {{
            background: rgba(245, 158, 11, 0.2);
            color: #fbbf24;
            border: 1px solid rgba(245, 158, 11, 0.4);
        }}
        .term-badge-unlocked {{
            background: rgba(16, 185, 129, 0.2);
            color: #34d399;
            border: 1px solid rgba(16, 185, 129, 0.4);
        }}
        .term-target-badge {{
            font-size: 0.75rem;
            padding: 3px 8px;
            border-radius: 4px;
            background: #1e293b;
            color: #94a3b8;
            border: 1px solid #334155;
        }}
        .terminal-actions {{
            display: flex;
            align-items: center;
            gap: 6px;
            flex-wrap: wrap;
        }}
        .chip {{
            background: #1f2937;
            color: #93c5fd;
            border: 1px solid #374151;
            padding: 3px 8px;
            border-radius: 4px;
            font-size: 0.75rem;
            cursor: pointer;
            transition: all 0.2s;
            font-family: 'JetBrains Mono', monospace;
        }}
        .chip:hover {{
            background: #374151;
            color: #ffffff;
            border-color: #60a5fa;
        }}
        .terminal-container {{
            flex: 1;
            background: #030712;
            padding: 16px;
            font-family: 'JetBrains Mono', monospace;
            font-size: 0.9rem;
            display: flex;
            flex-direction: column;
            min-height: 0;
            height: 100%;
            overflow: hidden;
        }}
        .terminal-output {{
            flex: 1;
            overflow-y: scroll;
            overflow-x: hidden;
            white-space: pre-wrap;
            word-break: break-all;
            color: #10b981;
            line-height: 1.5;
            padding-bottom: 8px;
            min-height: 0;
            scroll-behavior: smooth;
        }}
        .terminal-output::-webkit-scrollbar {{
            width: 10px;
        }}
        .terminal-output::-webkit-scrollbar-track {{
            background: #0b0f19;
            border-radius: 4px;
        }}
        .terminal-output::-webkit-scrollbar-thumb {{
            background: #374151;
            border-radius: 4px;
            border: 2px solid #0b0f19;
        }}
        .terminal-output::-webkit-scrollbar-thumb:hover {{
            background: #60a5fa;
        }}
        .terminal-input-bar {{
            display: flex;
            gap: 8px;
            border-top: 1px solid #1f2937;
            padding-top: 12px;
        }}
        .terminal-input-bar span {{ color: #60a5fa; }}
        .terminal-input-bar input {{
            flex: 1;
            background: transparent;
            border: none;
            color: #f9fafb;
            font-family: inherit;
            outline: none;
            font-size: 0.9rem;
        }}
        /* Panel Radar */
        .grid-cards {{
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
            gap: 16px;
            padding: 24px;
            overflow-y: auto;
        }}
        .card {{
            background: var(--bg-surface);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 16px;
            display: flex;
            flex-direction: column;
            gap: 12px;
            transition: transform 0.2s, border-color 0.2s;
        }}
        .card:hover {{
            transform: translateY(-2px);
            border-color: var(--primary);
        }}
        .card-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
        }}
        .card-title {{ font-size: 1.05rem; font-weight: 600; color: #fff; }}
        .card-meta {{ font-size: 0.85rem; color: var(--text-muted); }}

        /* Explorador de Archivos Interactivo */
        .file-row {{
            padding: 9px 12px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
            display: flex;
            justify-content: space-between;
            align-items: center;
            border-radius: 6px;
            transition: background 0.15s ease;
        }}
        .file-row:hover {{
            background: rgba(255, 255, 255, 0.05);
        }}
        .file-row-dir {{
            cursor: pointer;
        }}
        .file-row-dir:hover {{
            background: rgba(99, 102, 241, 0.15);
        }}
        .file-row-dir .file-name {{
            color: #60a5fa;
            font-weight: 500;
        }}
        .file-meta {{
            color: var(--text-muted);
            font-size: 0.82rem;
            font-family: monospace;
        }}
    </style>
</head>
<body>
    <header>
        <div class="brand">
            ⚡ NxDesk <span>v1.0.0</span> <span class="badge">PURE RUST</span>
        </div>

        <!-- Buscador NxDesk: Permite ingresar ID o IP remota para conectarse -->
        <div class="search-container">
            <input type="text" id="target-search-input" placeholder="🔍 Conectar a ID NxDesk o IP remota (ej. 111-222-333 o 192.168.1.50:8080)...">
            <button class="btn btn-primary" onclick="connectTargetInput()">⚡ Conectar</button>
        </div>

        <div class="header-meta">
            <div>
                <span style="color: var(--text-muted); font-size: 0.85rem;">Mi ID:</span>
                <strong style="color: #60a5fa; font-family: monospace; font-size: 1.05rem; margin-left: 4px;">{device_id}</strong>
            </div>

            <!-- Clave Maestra de Seguridad (Solo visible para el Anfitrión en Localhost) -->
            <div id="host-secret-pill" class="status-pill" style="display: none; background: rgba(99, 102, 241, 0.15); border-color: rgba(99, 102, 241, 0.4); color: #a5b4fc;" title="Clave maestra para autorizar terminal remota. Solo visible en la máquina anfitriona.">
                <span>🔐 Clave Anfitrión:</span>
                <strong id="host-secret-display" style="font-family: monospace; letter-spacing: 2px;">••••••••</strong>
                <button id="btn-toggle-secret" onclick="toggleHostSecret()" class="btn" style="padding: 2px 6px; font-size: 0.75rem; background: transparent; border: 1px solid rgba(255,255,255,0.2);" title="Mostrar / Ocultar">👁️</button>
                <button onclick="copyHostSecret()" class="btn" style="padding: 2px 6px; font-size: 0.75rem; background: transparent; border: 1px solid rgba(255,255,255,0.2);" title="Copiar clave">📋</button>
            </div>

            <!-- Contadores de Usuarios de Sesión NxDesk -->
            <div class="status-pill" title="Visores web conectados viendo esta pantalla">
                <span id="active-viewers-count">0</span> 👁️ Espectadores
            </div>
            <div class="status-pill" title="Participantes concurrentes en la sala de señalización">
                <span id="room-users-count">1</span> 👥 En Sala
            </div>
            <div class="status-pill">
                <div class="status-dot"></div>
                <span id="conn-status">En línea</span>
            </div>
        </div>
    </header>

    <nav class="tabs">
        <button class="tab-btn active" onclick="switchTab('screen')">🖥️ Pantalla Remota</button>
        <button class="tab-btn" onclick="switchTab('radar')">📡 Radar LAN (<span id="peer-count">0</span>)</button>
        <button class="tab-btn" onclick="switchTab('terminal')">💻 Terminal Remota</button>
        <button class="tab-btn" onclick="switchTab('files')">📁 Archivos</button>
    </nav>

    <main>
        <!-- Tab 1: Pantalla Remota -->
        <div id="panel-screen" class="view-panel active">
            <div id="viewport-container">
                <!-- Botón flotante siempre visible en pantalla completa para salir inmediatamente con clic o ESC -->
                <button id="btn-exit-fullscreen-float" class="btn-exit-fullscreen-float" onclick="toggleFullscreen()">
                    🗗 Salir de Pantalla Completa (ESC)
                </button>

                <!-- Banner de Sesión Remota Activa -->
                <div id="active-session-banner" class="notice-banner" style="background: rgba(16, 185, 129, 0.95); color: #042f2e; top: 16px;">
                    <span>🔴 Sesión activa con nodo NxDesk: <strong id="remote-peer-label">-</strong></span>
                    <button class="btn btn-danger" style="margin-left: 12px; padding: 4px 10px; font-size: 0.8rem;" onclick="disconnectRemoteSession()">🔌 Desconectar</button>
                </div>

                <div id="localhost-notice" class="notice-banner">
                    🛡️ Modo Vista Activo en Localhost: Control remoto deshabilitado en esta máquina para prevenir bucles de cursor.
                </div>

                <!-- Modal de Desbloqueo de Sesión para Clientes Externos -->
                <div id="auth-overlay" style="display: none; position: absolute; inset: 0; background: rgba(10, 14, 23, 0.94); backdrop-filter: blur(8px); z-index: 90; flex-direction: column; align-items: center; justify-content: center; padding: 20px;">
                    <div style="background: #1e293b; border: 1px solid #334155; border-radius: 14px; padding: 2.2rem; max-width: 440px; width: 100%; text-align: center; box-shadow: 0 20px 40px rgba(0,0,0,0.6);">
                        <div style="font-size: 2.4rem; margin-bottom: 0.5rem;">🔐</div>
                        <h3 style="color: #f8fafc; font-size: 1.25rem; font-weight: 600; margin-bottom: 0.5rem;">Transmisión Protegida</h3>
                        <p style="color: #94a3b8; font-size: 0.9rem; line-height: 1.5; margin-bottom: 1.5rem;">Introduce la clave del anfitrión para desbloquear la transmisión en vivo de este equipo:</p>
                        <form onsubmit="submitAuthForm(event)" style="display: flex; flex-direction: column; gap: 12px;">
                            <input type="password" id="auth-password-input" placeholder="Contraseña de anfitrión..." required style="background: #0f172a; border: 1px solid #475569; border-radius: 8px; padding: 12px 14px; color: #f8fafc; font-size: 1rem; outline: none; text-align: center;">
                            <button type="submit" id="btn-auth-submit" class="btn btn-primary" style="padding: 12px; font-weight: 600; font-size: 0.95rem;">🔓 Desbloquear y Ver Pantalla</button>
                        </form>
                    </div>
                </div>

                <canvas id="viewport"></canvas>
                <div class="viewport-toolbar">
                    <!-- Selector de Doble Pantalla / Monitores -->
                    <div style="display: flex; align-items: center; gap: 6px;">
                        <label for="screen-select" style="font-size: 0.8rem; color: #9ca3af; font-weight: 500;">🖥️ Monitor:</label>
                        <select id="screen-select" class="screen-select-dropdown" onchange="changeScreen(this.value)">
                            <option value="0">Detectando pantallas...</option>
                        </select>
                    </div>

                    <!-- Conmutador de Control Remoto Seguro (Previene mouse loco) -->
                    <button id="btn-control-toggle" class="btn btn-mode-view" onclick="toggleControlMode()">
                        👁️ Modo: Solo Vista
                    </button>

                    <button id="btn-fullscreen" class="btn" onclick="toggleFullscreen()">⛶ Pantalla Completa</button>
                </div>
            </div>
        </div>

        <!-- Tab 2: Radar LAN -->
        <div id="panel-radar" class="view-panel">
            <div class="grid-cards" id="peers-grid">
                <p style="color: var(--text-muted); padding: 24px;">Buscando equipos en la red local (UDP 5001)...</p>
            </div>
        </div>

        <!-- Tab 3: Terminal Remota -->
        <div id="panel-terminal" class="view-panel">
            <div class="terminal-toolbar">
                <div class="terminal-status-group">
                    <span id="term-status-badge" class="term-badge term-badge-locked">🛡️ VUL-01: Verificando...</span>
                    <span id="term-target-badge" class="term-target-badge">Nodo: Local</span>
                </div>
                <div class="terminal-actions">
                    <button class="btn btn-sm btn-outline" id="btn-term-unlock" onclick="toggleTerminalAuth()">🔓 Desbloquear</button>
                    <button class="btn btn-sm btn-outline" onclick="clearTerminal()">🧹 Limpiar</button>
                    <span style="color: #6b7280; font-size: 0.75rem; margin: 0 4px;">|</span>
                    <button class="chip" onclick="sendQuickCmd('ipconfig')">ipconfig</button>
                    <button class="chip" onclick="sendQuickCmd('whoami')">whoami</button>
                    <button class="chip" onclick="sendQuickCmd('dir')">dir</button>
                    <button class="chip" onclick="sendQuickCmd('netstat -an')">netstat</button>
                    <button class="chip" onclick="sendQuickCmd('systeminfo')">systeminfo</button>
                </div>
            </div>
            <!-- Barra de Privacidad y Estado: Anfitrión Local vs Cliente Remoto -->
            <div id="term-security-notice" style="background: #111827; border-bottom: 1px solid #1f2937; padding: 6px 14px; font-size: 0.8rem; color: #94a3b8; display: flex; align-items: center; justify-content: space-between;">
                <span id="term-security-msg">🛡️ Terminal protegida con contraseña maestra de anfitrión.</span>
                <span id="term-client-mode-tag" style="font-size: 0.75rem; background: #1e293b; padding: 2px 8px; border-radius: 4px; color: #38bdf8; border: 1px solid #334155;">Verificando nodo...</span>
            </div>
            <div class="terminal-container">
                <div class="terminal-output" id="term-out">NxDesk Native Rust Terminal Session [VUL-01 Secured]&#10;Presione 'Desbloquear' o ejecute comandos con validación de seguridad activa.&#10;</div>
                <div class="terminal-input-bar">
                    <span>nxdesk&gt;</span>
                    <input type="text" id="term-in" placeholder="dir, ipconfig, whoami... (o 'cls' para limpiar)" autofocus>
                </div>
            </div>
        </div>

        <!-- Tab 4: Explorador de Archivos Interactivo -->
        <div id="panel-files" class="view-panel">
            <div style="padding: 24px; overflow-y: auto;">
                <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px;">
                    <h3 style="margin: 0; font-size: 1.15rem; color: #f8fafc;">📂 Explorador de Archivos del Sistema</h3>
                    <div style="display: flex; gap: 8px;">
                        <button class="btn btn-sm btn-outline" onclick="goParentDirectory()">⬆️ Subir Nivel</button>
                        <button class="btn btn-sm btn-outline" onclick="loadExplorerPath(currentExplorerPath)">🔄 Actualizar</button>
                    </div>
                </div>
                <div style="display: flex; gap: 8px; margin-bottom: 14px;">
                    <input type="text" id="current-path-input" style="flex: 1; background: var(--bg-surface); border: 1px solid var(--border); border-radius: 6px; padding: 8px 12px; color: var(--text-main); font-family: monospace; font-size: 0.85rem;" placeholder="Ruta del sistema...">
                    <button class="btn btn-sm btn-primary" onclick="loadExplorerPath(document.getElementById('current-path-input').value)">Ir</button>
                </div>
                <div id="files-list" style="background: var(--bg-surface); border-radius: 8px; border: 1px solid var(--border); padding: 8px; max-height: 520px; overflow-y: auto;">
                    Cargando archivos...
                </div>
            </div>
        </div>
    </main>

    <!-- Contenedor Toast para Notificaciones Seguras en Contextos HTTP y HTTPS -->
    <div id="toast-container" style="position: fixed; bottom: 24px; right: 24px; z-index: 99999; display: flex; flex-direction: column; gap: 8px; pointer-events: none;"></div>

    <script>
        const canvas = document.getElementById('viewport');
        const ctx = canvas.getContext('2d', {{ alpha: false, desynchronized: true }});
        let ws;
        let isControlEnabled = false; // MODO SOLO LECTURA POR DEFECTO (Previene bucles de ratón)
        let sessionToken = null;
        const isLocalhost = (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1');

        function showToast(msg, isWarn) {{
            const container = document.getElementById('toast-container');
            if (!container) return;
            const toast = document.createElement('div');
            toast.style.background = isWarn ? '#78350f' : '#1e293b';
            toast.style.border = isWarn ? '1px solid #d97706' : '1px solid #3b82f6';
            toast.style.color = '#f8fafc';
            toast.style.padding = '10px 16px';
            toast.style.borderRadius = '8px';
            toast.style.fontSize = '0.85rem';
            toast.style.boxShadow = '0 10px 25px rgba(0,0,0,0.5)';
            toast.style.transition = 'opacity 0.3s ease, transform 0.3s ease';
            toast.style.pointerEvents = 'auto';
            toast.textContent = msg;
            container.appendChild(toast);
            setTimeout(() => {{
                toast.style.opacity = '0';
                toast.style.transform = 'translateY(10px)';
                setTimeout(() => toast.remove(), 300);
            }}, 3500);
        }}

        async function submitAuthForm(e) {{
            if (e && e.preventDefault) e.preventDefault();
            const pwInput = document.getElementById('auth-password-input');
            const pw = pwInput.value.trim();
            if (!pw) return;
            const btn = document.getElementById('btn-auth-submit');
            btn.textContent = 'Verificando...';
            btn.disabled = true;

            try {{
                const res = await fetch('/api/auth', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ password: pw, requested_role: 'FullControl' }})
                }});
                const data = await res.json();
                if (!data.success) {{
                    showToast("❌ " + (data.error_message || "Contraseña incorrecta"), true);
                    btn.textContent = '🔓 Desbloquear y Ver Pantalla';
                    btn.disabled = false;
                    return;
                }}
                sessionToken = data.session_token;
                document.getElementById('auth-overlay').style.display = 'none';
                if (ws && ws.readyState === WebSocket.OPEN) {{
                    ws.send(JSON.stringify({{ type: 'auth', token: sessionToken }}));
                }}
                showToast("✅ Transmisión desbloqueada exitosamente.");
            }} catch (err) {{
                showToast("❌ Error al autenticar: " + err.message, true);
                btn.textContent = '🔓 Desbloquear y Ver Pantalla';
                btn.disabled = false;
            }}
        }}

        async function toggleControlMode() {{
            if (isLocalhost) {{
                showToast("🛡️ Control Remoto deshabilitado en Localhost para prevenir bucles de cursor. Conéctate desde otra PC o móvil para controlar este equipo.", true);
                return;
            }}
            if (!isControlEnabled && !sessionToken) {{
                document.getElementById('auth-overlay').style.display = 'flex';
                setTimeout(() => {{
                    const inp = document.getElementById('auth-password-input');
                    if (inp) inp.focus();
                }}, 100);
                return;
            }}
            isControlEnabled = !isControlEnabled;
            const btn = document.getElementById('btn-control-toggle');
            if (isControlEnabled) {{
                btn.className = 'btn btn-mode-control';
                btn.textContent = '🕹️ Control Remoto Activo';
                canvas.classList.add('control-active');
            }} else {{
                btn.className = 'btn btn-mode-view';
                btn.textContent = '👁️ Modo: Solo Vista';
                canvas.classList.remove('control-active');
            }}
        }}

        let currentActiveTab = 'screen';
        let latestFrameData = null;
        let isRenderLoopActive = false;

        function processRenderQueue() {{
            if (!latestFrameData) {{
                isRenderLoopActive = false;
                return;
            }}

            const frameBytes = latestFrameData;
            latestFrameData = null; // Consume el fotograma más fresco, descartando cola acumulada (0ms delay)
            const blob = new Blob([frameBytes], {{ type: 'image/jpeg' }});

            if (window.createImageBitmap) {{
                createImageBitmap(blob).then(bitmap => {{
                    if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {{
                        canvas.width = bitmap.width;
                        canvas.height = bitmap.height;
                    }}
                    ctx.drawImage(bitmap, 0, 0);
                    bitmap.close(); // Libera la memoria GPU y VRAM inmediatamente
                    requestAnimationFrame(processRenderQueue);
                }}).catch(() => {{
                    requestAnimationFrame(processRenderQueue);
                }});
            }} else {{
                const url = URL.createObjectURL(blob);
                const img = new Image();
                img.onload = function() {{
                    if (canvas.width !== img.width || canvas.height !== img.height) {{
                        canvas.width = img.width;
                        canvas.height = img.height;
                    }}
                    ctx.drawImage(img, 0, 0);
                    URL.revokeObjectURL(url);
                    requestAnimationFrame(processRenderQueue);
                }};
                img.onerror = function() {{
                    URL.revokeObjectURL(url);
                    requestAnimationFrame(processRenderQueue);
                }};
                img.src = url;
            }}
        }}

        function initWebSocket() {{
            const loc = window.location;
            let wsUri = (loc.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + loc.host + '/ws/stream';
            if (sessionToken) {{
                wsUri += '?token=' + encodeURIComponent(sessionToken);
            }}
            ws = new WebSocket(wsUri);
            ws.binaryType = 'arraybuffer';

            ws.onmessage = function(event) {{
                if (typeof event.data === 'string') {{
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'auth_required') {{
                        document.getElementById('auth-overlay').style.display = 'flex';
                        const urlParams = new URLSearchParams(window.location.search);
                        const urlPwd = urlParams.get('pwd') || urlParams.get('password');
                        if (urlPwd) {{
                            document.getElementById('auth-password-input').value = urlPwd;
                            setTimeout(() => submitAuthForm(new Event('submit')), 100);
                        }} else {{
                            setTimeout(() => {{
                                const inp = document.getElementById('auth-password-input');
                                if (inp) inp.focus();
                            }}, 100);
                        }}
                    }} else if (msg.type === 'terminal') {{
                        const out = document.getElementById('term-out');
                        const isAtBottom = (out.scrollHeight - out.scrollTop <= out.clientHeight + 80);
                        out.textContent += msg.data;
                        if (isAtBottom) {{
                            out.scrollTop = out.scrollHeight;
                        }}
                    }}
                }} else {{
                    if (currentActiveTab !== 'screen' || document.hidden) {{
                        return;
                    }}
                    // Cola de retardo cero: Almacena siempre el fotograma más reciente y procesa con VSync
                    latestFrameData = event.data;
                    if (!isRenderLoopActive) {{
                        isRenderLoopActive = true;
                        requestAnimationFrame(processRenderQueue);
                    }}
                }}
            }};

            ws.onclose = function() {{
                document.getElementById('conn-status').textContent = 'Reconectando...';
                setTimeout(initWebSocket, 2000);
            }};
        }}
        initWebSocket();

        // Conectar usando el Buscador NxDesk (por ID o IP)
        async function connectTargetInput() {{
            const input = document.getElementById('target-search-input');
            const target = input.value.trim();
            if (!target) {{
                alert("Por favor ingrese un ID de equipo (ej. 111-222-333) o una dirección IP.");
                return;
            }}
            await connectToPeer(target, target);
        }}

        // Conectar a un par (por ID o IP)
        async function connectToPeer(id, ip) {{
            try {{
                const res = await fetch('/api/connect', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ target: id || ip }})
                }});
                const data = await res.json();
                if (data.success) {{
                    document.getElementById('remote-peer-label').textContent = id || ip;
                    document.getElementById('active-session-banner').style.display = 'block';
                    switchTab('screen');
                }} else {{
                    alert("No se pudo conectar: " + data.message);
                }}
            }} catch (e) {{
                console.error("Error al conectar:", e);
                alert("Error de conexión: " + e.message);
            }}
        }}

        // Desconectar sesión remota
        async function disconnectRemoteSession() {{
            try {{
                await fetch('/api/disconnect', {{ method: 'POST' }});
                document.getElementById('active-session-banner').style.display = 'none';
                document.getElementById('target-search-input').value = '';
            }} catch (e) {{
                console.error("Error al desconectar:", e);
            }}
        }}

        // 1. Cargar lista de pantallas (Doble Pantalla / Monitores)
        async function loadScreens() {{
            try {{
                const res = await fetch('/api/screens');
                const screens = await res.json();
                const sel = document.getElementById('screen-select');
                if (screens && screens.length > 0) {{
                    sel.innerHTML = screens.map(s => {{
                        const idx = (s.Index !== undefined) ? s.Index : s.index;
                        const name = s.Name || s.name;
                        const width = s.Width || s.width;
                        const height = s.Height || s.height;
                        const isPrimary = (s.IsPrimary !== undefined) ? s.IsPrimary : s.is_primary;
                        return `<option value="${{idx}}" ${{isPrimary ? 'selected' : ''}}>${{name}} (${{width}}x${{height}})</option>`;
                    }}).join('');
                }}
            }} catch (e) {{
                console.error("Error al cargar lista de pantallas:", e);
            }}
        }}
        loadScreens();

        // 2. Conmutar pantalla en caliente
        async function changeScreen(index) {{
            const idx = parseInt(index, 10);
            try {{
                const res = await fetch('/api/screens/select', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ index: idx }})
                }});
                const data = await res.json();
                if (data.Ok) {{
                    console.log("Pantalla cambiada con éxito:", data.Ok.Name);
                }}
            }} catch (e) {{
                console.error("Error al cambiar pantalla:", e);
            }}
        }}

        // 3. Inyección Segura de Eventos de Ratón (Throttled & Control-Gated & Anti-Bucle Localhost)
        let lastMoveTime = 0;
        canvas.addEventListener('mousemove', (e) => {{
            if (isLocalhost || !isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            const now = performance.now();
            if (now - lastMoveTime < 33) return; // Límite a ~30 eventos por segundo (anti-rebote)
            lastMoveTime = now;

            const rect = canvas.getBoundingClientRect();
            const x = (e.clientX - rect.left) / rect.width;
            const y = (e.clientY - rect.top) / rect.height;

            if (x >= 0.0 && x <= 1.0 && y >= 0.0 && y <= 1.0) {{
                ws.send(JSON.stringify({{ Type: 'MouseMove', X: x, Y: y }}));
            }}
        }});

        let lastMouseDownTime = 0;
        canvas.addEventListener('mousedown', (e) => {{
            if (isLocalhost || !isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            const now = performance.now();
            if (now - lastMouseDownTime < 100) return; // Debounce 100ms para prevenir clics duplicados
            lastMouseDownTime = now;

            const btn = e.button === 0 ? 'Left' : (e.button === 2 ? 'Right' : 'Middle');
            const rect = canvas.getBoundingClientRect();
            const x = (e.clientX - rect.left) / rect.width;
            const y = (e.clientY - rect.top) / rect.height;
            ws.send(JSON.stringify({{ Type: 'MouseDown', MouseButton: btn, X: x, Y: y }}));
        }});

        canvas.addEventListener('mouseup', (e) => {{
            if (isLocalhost || !isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            const btn = e.button === 0 ? 'Left' : (e.button === 2 ? 'Right' : 'Middle');
            const rect = canvas.getBoundingClientRect();
            const x = (e.clientX - rect.left) / rect.width;
            const y = (e.clientY - rect.top) / rect.height;
            ws.send(JSON.stringify({{ Type: 'MouseUp', MouseButton: btn, X: x, Y: y }}));
        }});

        canvas.addEventListener('contextmenu', (e) => {{
            if (isControlEnabled) e.preventDefault();
        }});

        canvas.addEventListener('wheel', (e) => {{
            if (isLocalhost || !isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            e.preventDefault();
            const deltaY = e.deltaY < 0 ? 1.0 : -1.0;
            ws.send(JSON.stringify({{ Type: 'MouseWheel', DeltaY: deltaY }}));
        }});

        // 4. Inyección Segura de Teclado
        window.addEventListener('keydown', (e) => {{
            if (e.key === 'Escape' || e.key === 'Esc') {{
                if (document.fullscreenElement) {{
                    if (document.exitFullscreen) {{
                        document.exitFullscreen().catch(err => console.error(err));
                    }}
                    return;
                }}
            }}
            if (isLocalhost || !isControlEnabled) return;
            if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'SELECT') return;
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            ws.send(JSON.stringify({{ Type: 'KeyDown', Key: e.keyCode, KeyCode: e.code }}));
        }});

        window.addEventListener('keyup', (e) => {{
            if (isLocalhost || !isControlEnabled) return;
            if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'SELECT') return;
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            ws.send(JSON.stringify({{ Type: 'KeyUp', Key: e.keyCode, KeyCode: e.code }}));
        }});

        // 5. Gestión Segura de Credenciales de Anfitrión y Terminal Remota
        let isTerminalAllowed = false;
        let hostPasswordSecret = null;
        let isSecretVisible = false;
        const termInput = document.getElementById('term-in');

        async function checkHostCredentials() {{
            try {{
                const res = await fetch('/api/host/credentials');
                const data = await res.json();
                const pill = document.getElementById('host-secret-pill');
                const secMsg = document.getElementById('term-security-msg');
                const modeTag = document.getElementById('term-client-mode-tag');

                if (data.is_localhost && data.host_password) {{
                    hostPasswordSecret = data.host_password;
                    // Auto-autorizar terminal localmente para el anfitrión (dueño del equipo)
                    isTerminalAllowed = true;
                    updateTerminalUI(true);
                    fetch('/api/terminal/auth', {{
                        method: 'POST',
                        headers: {{ 'Content-Type': 'application/json' }},
                        body: JSON.stringify({{ enable: true, password: hostPasswordSecret }})
                    }}).catch(() => {{}});

                    fetch('/api/auth', {{
                        method: 'POST',
                        headers: {{ 'Content-Type': 'application/json' }},
                        body: JSON.stringify({{ password: hostPasswordSecret, requested_role: 'Administrator' }})
                    }}).then(r => r.json()).then(d => {{
                        if (d.success) {{
                            sessionToken = d.session_token;
                            if (ws && ws.readyState === WebSocket.OPEN) {{
                                ws.send(JSON.stringify({{ type: 'auth', token: sessionToken }}));
                            }}
                        }}
                    }}).catch(() => {{}});

                    if (pill) pill.style.display = 'inline-flex';
                    if (secMsg) secMsg.innerHTML = '👑 <strong>Modo Anfitrión Local:</strong> Terminal autorizada automáticamente con clave maestra.';
                    if (modeTag) {{
                        modeTag.textContent = '👑 Anfitrión Autorizado';
                        modeTag.style.color = '#34d399';
                        modeTag.style.borderColor = '#10b981';
                    }}
                }} else {{
                    hostPasswordSecret = null;
                    if (pill) pill.style.display = 'none';
                    if (secMsg) secMsg.innerHTML = '🛡️ <strong>Modo Cliente Remoto:</strong> Para ejecutar comandos debes solicitar la Clave Maestra al anfitrión de este equipo.';
                    if (modeTag) {{
                        modeTag.textContent = '🔒 Cliente Remoto';
                        modeTag.style.color = '#f59e0b';
                        modeTag.style.borderColor = '#d97706';
                    }}
                }}
            }} catch (e) {{
                console.error("Error al consultar credenciales del anfitrión:", e);
            }}
        }}
        checkHostCredentials();

        function toggleHostSecret() {{
            if (!hostPasswordSecret) return;
            isSecretVisible = !isSecretVisible;
            const display = document.getElementById('host-secret-display');
            const inlineDisplay = document.getElementById('term-host-pw-inline');
            const btn = document.getElementById('btn-toggle-secret');

            if (isSecretVisible) {{
                if (display) display.textContent = hostPasswordSecret;
                if (inlineDisplay) inlineDisplay.textContent = hostPasswordSecret;
                if (btn) btn.textContent = '🙈';
            }} else {{
                if (display) display.textContent = '••••••••';
                if (inlineDisplay) inlineDisplay.textContent = '••••••••';
                if (btn) btn.textContent = '👁️';
            }}
        }}

        function copyHostSecret() {{
            if (!hostPasswordSecret) return;
            copyToClipboard(hostPasswordSecret, "📋 ¡Clave maestra del anfitrión copiada al portapapeles!");
        }}

        function copyToClipboard(text, successMsg) {{
            if (navigator.clipboard && window.isSecureContext) {{
                navigator.clipboard.writeText(text).then(() => {{
                    showToast(successMsg || "📋 ¡Copiado al portapapeles!");
                }}).catch(() => {{
                    fallbackCopyText(text, successMsg);
                }});
            }} else {{
                fallbackCopyText(text, successMsg);
            }}
        }}

        function fallbackCopyText(text, successMsg) {{
            try {{
                const ta = document.createElement('textarea');
                ta.value = text;
                ta.style.position = 'fixed';
                ta.style.left = '-9999px';
                ta.style.top = '-9999px';
                document.body.appendChild(ta);
                ta.focus();
                ta.select();
                const ok = document.execCommand('copy');
                document.body.removeChild(ta);
                if (ok) {{
                    showToast(successMsg || "📋 ¡Copiado al portapapeles!");
                }} else {{
                    prompt("Copia manualmente con Ctrl+C:", text);
                }}
            }} catch (e) {{
                prompt("Copia manualmente con Ctrl+C:", text);
            }}
        }}

        function clearTerminal() {{
            document.getElementById('term-out').textContent = '';
        }}

        function sendTerminalCommand(cmd) {{
            const trimmed = cmd.trim();
            if (!trimmed) return;

            if (trimmed.toLowerCase() === 'cls' || trimmed.toLowerCase() === 'clear') {{
                clearTerminal();
                return;
            }}

            if (!isTerminalAllowed) {{
                const out = document.getElementById('term-out');
                out.textContent += `\n> ${{trimmed}}\n⛔ [ACCESO DENEGADO]: La terminal remota está bloqueada.\nHaga clic en '🔓 Desbloquear' e ingrese la clave maestra del anfitrión para ejecutar comandos.\n`;
                out.scrollTop = out.scrollHeight;
                return;
            }}

            if (ws && ws.readyState === WebSocket.OPEN) {{
                ws.send(JSON.stringify({{ terminal_cmd: trimmed }}));
                const out = document.getElementById('term-out');
                out.textContent += `\n> ${{trimmed}}\n`;
                out.scrollTop = out.scrollHeight;
            }} else {{
                showToast("⚠️ Conexión WebSocket no activa.", true);
            }}
        }}

        function sendQuickCmd(cmd) {{
            sendTerminalCommand(cmd);
        }}

        async function toggleTerminalAuth() {{
            let password = null;
            if (!isTerminalAllowed) {{
                const promptMsg = hostPasswordSecret 
                    ? `🔐 Ingrese la clave maestra del anfitrión (Detectada localmente: ${{hostPasswordSecret}}):`
                    : "🔐 Ingrese la clave maestra proporcionada por el anfitrión para desbloquear la terminal:";
                password = prompt(promptMsg, hostPasswordSecret || "");
                if (password === null) return;
            }}

            try {{
                const res = await fetch('/api/terminal/auth', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ enable: !isTerminalAllowed, password: password }})
                }});
                const data = await res.json();
                if (data.success) {{
                    isTerminalAllowed = data.allowed;
                    updateTerminalUI(data.allowed);
                    const out = document.getElementById('term-out');
                    if (data.allowed) {{
                        out.textContent += "\n🔓 [AUTORIZADO]: Terminal remota desbloqueada con éxito.\n";
                    }} else {{
                        out.textContent += "\n🔒 [BLOQUEADA]: Terminal remota bloqueada.\n";
                    }}
                    out.scrollTop = out.scrollHeight;
                }} else {{
                    alert("⛔ Error de autorización: " + data.message);
                    const out = document.getElementById('term-out');
                    out.textContent += `\n⛔ [DENEGADO]: ${{data.message}}\n`;
                    out.scrollTop = out.scrollHeight;
                }}
            }} catch (e) {{
                console.error("Error al autenticar terminal:", e);
                alert("Error de red al autorizar terminal.");
            }}
        }}

        function updateTerminalUI(allowed) {{
            isTerminalAllowed = allowed;
            const badge = document.getElementById('term-status-badge');
            const btn = document.getElementById('btn-term-unlock');
            if (badge && btn) {{
                if (allowed) {{
                    badge.className = 'term-badge term-badge-unlocked';
                    badge.textContent = '🔓 VUL-01: Terminal Habilitada';
                    btn.textContent = '🔒 Bloquear';
                }} else {{
                    badge.className = 'term-badge term-badge-locked';
                    badge.textContent = '🛡️ VUL-01: Terminal Bloqueada';
                    btn.textContent = '🔓 Desbloquear';
                }}
            }}
        }}

        termInput.addEventListener('keydown', (e) => {{
            if (e.key === 'Enter') {{
                const cmd = termInput.value;
                termInput.value = '';
                sendTerminalCommand(cmd);
            }}
        }});

        function switchTab(name) {{
            currentActiveTab = name;
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.view-panel').forEach(p => p.classList.remove('active'));
            const tabBtn = Array.from(document.querySelectorAll('.tab-btn')).find(b => b.getAttribute('onclick') && b.getAttribute('onclick').includes(name));
            if (tabBtn) tabBtn.classList.add('active');
            const panel = document.getElementById('panel-' + name);
            if (panel) panel.classList.add('active');
            if (name === 'radar') refreshPeers();
            if (name === 'files') loadExplorerPath(currentExplorerPath || '');
        }}

        function toggleFullscreen() {{
            const container = document.getElementById('viewport-container');
            if (!document.fullscreenElement) {{
                if (container.requestFullscreen) {{
                    container.requestFullscreen().catch(err => console.error(err));
                }}
            }} else {{
                if (document.exitFullscreen) {{
                    document.exitFullscreen().catch(err => console.error(err));
                }}
            }}
        }}

        document.addEventListener('fullscreenchange', () => {{
            const isFull = !!document.fullscreenElement;
            const btn = document.getElementById('btn-fullscreen');
            const exitFloat = document.getElementById('btn-exit-fullscreen-float');
            if (isFull) {{
                if (btn) btn.textContent = '🗗 Salir Pantalla Completa (ESC)';
                if (exitFloat) exitFloat.style.display = 'block';
            }} else {{
                if (btn) btn.textContent = '⛶ Pantalla Completa';
                if (exitFloat) exitFloat.style.display = 'none';
            }}
        }});

        async function refreshPeers() {{
            const res = await fetch('/api/peers');
            const peers = await res.json();
            document.getElementById('peer-count').textContent = peers.length;
            const grid = document.getElementById('peers-grid');
            if (peers.length === 0) {{
                grid.innerHTML = '<p style="color: var(--text-muted); padding: 24px;">No hay pares descubiertos en la red local en este instante.</p>';
                return;
            }}
            grid.innerHTML = peers.map(p => `
                <div class="card">
                    <div class="card-header">
                        <span class="card-title">${{p.Alias || 'Equipo'}}</span>
                        <span class="status-pill">${{p.Status}}</span>
                    </div>
                    <div class="card-meta">
                        <div>ID: <strong>${{p.Id}}</strong></div>
                        <div>IP: ${{p.IpAddress}}:${{p.Port}}</div>
                        <div>SO: ${{p.Os || 'Desconocido'}}</div>
                    </div>
                    <button class="btn btn-primary" onclick="connectToPeer('${{p.Id}}', '${{p.IpAddress}}:${{p.Port}}')">⚡ Conectar a este Equipo</button>
                </div>
            `).join('');
        }}

        let currentExplorerPath = '';

        async function loadExplorerPath(targetPath) {{
            const list = document.getElementById('files-list');
            list.innerHTML = '<div style="color: var(--text-muted); padding: 12px;">⏳ Explorando directorio...</div>';
            try {{
                let url = targetPath ? ('/api/files?path=' + encodeURIComponent(targetPath)) : '/api/files';
                if (sessionToken) {{
                    url += (url.includes('?') ? '&' : '?') + 'token=' + encodeURIComponent(sessionToken);
                }}
                const res = await fetch(url);
                const data = await res.json();
                currentExplorerPath = data.CurrentPath || '';

                if (data.Error) {{
                    list.innerHTML = `<div style="color: #ef4444; padding: 12px;">❌ Error: ${{data.Error}}</div>`;
                    return;
                }}

                const inputPath = document.getElementById('current-path-input');
                if (inputPath) inputPath.value = currentExplorerPath;

                if (!data.Items || data.Items.length === 0) {{
                    list.innerHTML = '<div style="color: var(--text-muted); padding: 12px;">📁 Esta carpeta está vacía.</div>';
                    return;
                }}

                // Ordenar: primero carpetas, luego archivos
                const sorted = [...data.Items].sort((a, b) => {{
                    if (a.IsDirectory === b.IsDirectory) return a.Name.localeCompare(b.Name);
                    return a.IsDirectory ? -1 : 1;
                }});

                list.innerHTML = sorted.map(item => {{
                    const icon = item.IsDirectory ? '📁' : '📄';
                    const sizeStr = item.IsDirectory ? '&lt;CARPETA&gt;' : formatBytes(item.Size);
                    const itemPath = (currentExplorerPath.endsWith('/') || currentExplorerPath.endsWith('\\'))
                        ? (currentExplorerPath + item.Name)
                        : (currentExplorerPath + (currentExplorerPath.includes('/') ? '/' : '\\') + item.Name);
                    const escapedPath = itemPath.replace(/\\/g, '\\\\').replace(/'/g, "\\'");

                    if (item.IsDirectory) {{
                        return `
                            <div class="file-row file-row-dir" onclick="loadExplorerPath('${{escapedPath}}')">
                                <span class="file-name">📁 <strong>${{item.Name}}</strong></span>
                                <span class="file-meta">${{sizeStr}}</span>
                            </div>
                        `;
                    }} else {{
                        return `
                            <div class="file-row">
                                <span class="file-name">📄 ${{item.Name}}</span>
                                <span class="file-meta">${{sizeStr}}</span>
                            </div>
                        `;
                    }}
                }}).join('');
            }} catch (e) {{
                list.innerHTML = `<div style="color: #ef4444; padding: 12px;">❌ Error de conexión al cargar archivos: ${{e.message}}</div>`;
            }}
        }}

        function goParentDirectory() {{
            if (!currentExplorerPath) return;
            const sep = currentExplorerPath.includes('/') ? '/' : '\\';
            const parts = currentExplorerPath.split(sep).filter(p => p.length > 0);
            if (parts.length <= 1) return;
            parts.pop();
            const parent = parts.join(sep) + (currentExplorerPath.startsWith(sep) ? sep : '');
            loadExplorerPath(parent);
        }}

        function formatBytes(bytes) {{
            if (!bytes || bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
        }}

        // Sondeo periódico del estado del nodo y contadores de usuarios
        setInterval(async () => {{
            try {{
                const res = await fetch('/api/status');
                const data = await res.json();
                document.getElementById('peer-count').textContent = data.discovered_peers_count;
                document.getElementById('active-viewers-count').textContent = data.connected_viewers_count;
                document.getElementById('room-users-count').textContent = data.active_room_users;
                
                const banner = document.getElementById('active-session-banner');
                const targetBadge = document.getElementById('term-target-badge');
                if (data.active_remote_peer) {{
                    document.getElementById('remote-peer-label').textContent = data.active_remote_peer;
                    banner.style.display = 'block';
                    if (targetBadge) {{
                        targetBadge.textContent = 'Nodo Remoto: ' + data.active_remote_peer;
                        targetBadge.style.color = '#38bdf8';
                    }}
                }} else {{
                    if (targetBadge) {{
                        targetBadge.textContent = 'Nodo: Local';
                        targetBadge.style.color = '#94a3b8';
                    }}
                }}

                if (data.allow_remote_terminal !== undefined && isTerminalAllowed !== data.allow_remote_terminal) {{
                    updateTerminalUI(data.allow_remote_terminal);
                }}
            }} catch (e) {{}}
        }}, 2500);
    </script>
</body>
</html>"#,
        device_id = cfg.device_id
    );

    (
        [
            (
                axum::http::header::CACHE_CONTROL,
                "no-cache, no-store, must-revalidate",
            ),
            (axum::http::header::PRAGMA, "no-cache"),
        ],
        Html(html),
    )
}
