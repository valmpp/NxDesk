use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use nxdesk_core::{AuthRequest, AuthResponse, InputEvent, ListDirectoryResponse, ScreenInfoDto, SignalingMessage};
use nxdesk_file_transfer::list_directory_async;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info};

pub async fn start_web_server(state: AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let initial_port = {
        let cfg = state.config.read().await;
        cfg.web_port
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/status", get(get_status))
        .route("/api/peers", get(get_peers))
        .route("/api/files", get(get_files))
        .route("/api/auth", post(post_auth))
        .route("/api/screens", get(get_screens))
        .route("/api/screens/select", post(post_select_screen))
        .route("/api/connect", post(post_connect))
        .route("/api/disconnect", post(post_disconnect))
        .route("/api/terminal/auth", post(post_terminal_auth))
        .route("/api/input", post(post_input))
        .route("/ws/stream", get(ws_handler))
        .with_state(state.clone());

    // Auto-incremento de puerto si está ocupado (evita colisiones AddrInUse en pruebas locales)
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
        format!("No se pudo enlazar ningún puerto web entre {} y {}", initial_port, initial_port + 50)
    })?;

    // Actualizar el puerto asignado en la configuración
    {
        let mut cfg = state.config.write().await;
        cfg.web_port = bound_port;
    }

    info!("==================================================");
    info!("   ✨  NxDesk Web Dashboard (Pure Rust UI)        ");
    info!("   Acceso Local: http://localhost:{}", bound_port);
    info!("   Acceso Red:   http://0.0.0.0:{}", bound_port);
    info!("==================================================");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("Error en servidor web NxDesk: {}", e);
        }
    });

    Ok(())
}

#[derive(Serialize)]
struct StatusResponse {
    device_id: String,
    device_alias: String,
    signaling_connected: bool,
    discovered_peers_count: usize,
    connected_viewers_count: usize,
    active_room_users: usize,
    active_remote_peer: Option<String>,
    allow_remote_terminal: bool,
    max_fps: u32,
    quality: u32,
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
}

async fn get_files(Query(query): Query<FilesQuery>) -> Json<ListDirectoryResponse> {
    let path = query.path.unwrap_or_default();
    let resp = list_directory_async(&path).await;
    Json(resp)
}

async fn post_auth(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Json<AuthResponse> {
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

/// Conecta el cliente a una sesión remota (ID AnyDesk o IP directa)
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
                info!("🚀 [Web Connect] Conectado exitosamente a la sala del equipo remoto '{}'", target);
                Json(ConnectResponse {
                    success: true,
                    message: format!("Conectado exitosamente a la sala remota '{}'", target),
                    target,
                })
            }
            Err(e) => {
                Json(ConnectResponse {
                    success: false,
                    message: format!("Error al unirse a la sala remota: {}", e),
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
async fn post_disconnect(
    State(state): State<AppState>,
) -> Json<ConnectResponse> {
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

/// Autoriza o revoca el uso de la terminal remota en caliente (VUL-01)
async fn post_terminal_auth(
    State(state): State<AppState>,
    Json(payload): Json<TerminalAuthPayload>,
) -> Json<TerminalAuthResponse> {
    let enable = payload.enable.unwrap_or(true);

    if enable {
        let cfg = state.config.read().await;
        if let Some(expected_pw) = &cfg.access_password {
            if !expected_pw.is_empty() {
                let provided = payload.password.unwrap_or_default();
                if provided != *expected_pw {
                    return Json(TerminalAuthResponse {
                        success: false,
                        allowed: false,
                        message: "Contraseña administrativa incorrecta.".to_string(),
                    });
                }
            }
        }
    }

    state.set_remote_terminal_allowed(enable).await;
    let msg = if enable {
        "🔓 Terminal Remota desbloqueada y autorizada para ejecución (VUL-01 verificada)."
    } else {
        "🔒 Terminal Remota bloqueada (VUL-01 activa)."
    };
    info!("🛡️ [Terminal Auth] {}", msg);
    let _ = state.terminal_broadcast_tx.send(format!("\r\n📢 [NxDesk Seguridad] {}\r\n\r\n", msg));

    Json(TerminalAuthResponse {
        success: true,
        allowed: enable,
        message: msg.to_string(),
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_web_socket(socket, state))
}

async fn handle_web_socket(socket: WebSocket, state: AppState) {
    state.increment_connected_viewers();
    let v_count = state.get_connected_viewers();
    info!("👥 [Web] Nuevo visor web conectado. Espectadores activos: {}", v_count);

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut frame_rx = state.frame_broadcast_tx.subscribe();
    let mut term_rx = state.terminal_broadcast_tx.subscribe();

    // Enviar último fotograma si existe
    if let Some(initial_frame) = state.latest_jpeg_frame.read().await.clone() {
        let _ = ws_sender.send(Message::Binary(initial_frame.into())).await;
    }

    // Tarea para enviar fotogramas y mensajes al cliente web
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                frame_res = frame_rx.recv() => {
                    match frame_res {
                        Ok(frame_bytes) => {
                            if ws_sender.send(Message::Binary(frame_bytes.into())).await.is_err() {
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
                            let json = format!("{{\"type\":\"terminal\",\"data\":{}}}", serde_json::to_string(&term_msg).unwrap_or_default());
                            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                break;
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
                    if let Ok(evt) = serde_json::from_str::<InputEvent>(&text) {
                        let injector = state_clone.input_injector.lock().await;
                        injector.inject(&evt);
                    } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(cmd) = val.get("terminal_cmd").and_then(|v| v.as_str()) {
                            let remote_peer = state_clone.get_active_remote_peer().await;
                            let sig_client = state_clone.get_signaling_client().await;

                            if let (Some(_peer), Some(signaling)) = (remote_peer, sig_client) {
                                info!("📡 [Web Terminal] Reenviando comando '{}' a equipo remoto vía Señalización.", cmd);
                                let sig_msg = SignalingMessage::new("terminal_cmd", cmd);
                                if let Err(e) = signaling.send_message(&sig_msg).await {
                                    let err_msg = format!("\r\n❌ Error al enviar comando al host remoto: {}\r\n", e);
                                    let _ = state_clone.terminal_broadcast_tx.send(err_msg);
                                }
                            } else {
                                let allow = state_clone.is_remote_terminal_allowed().await;
                                if allow {
                                    if !state_clone.terminal_session.is_running() {
                                        if let Ok(mut rx) = state_clone.terminal_session.start().await {
                                            let term_tx = state_clone.terminal_broadcast_tx.clone();
                                            tokio::spawn(async move {
                                                while let Some(chunk) = rx.recv().await {
                                                    let _ = term_tx.send(chunk);
                                                }
                                            });
                                        }
                                    }
                                    if let Err(e) = state_clone.terminal_session.write_input(cmd).await {
                                        let err_msg = format!("\r\n❌ Error al escribir en terminal local: {}\r\n", e);
                                        let _ = state_clone.terminal_broadcast_tx.send(err_msg);
                                    }
                                } else {
                                    let warn_msg = "\r\n⚠️ [VUL-01: Seguridad Activa] La terminal remota está deshabilitada en este nodo.\r\nPara autorizar la ejecución de comandos:\r\n  1. Presione el botón '🔓 Desbloquear' en la barra superior del panel,\r\n  2. O reinicie con el parámetro: --allow-remote-terminal\r\n\r\n";
                                    let _ = state_clone.terminal_broadcast_tx.send(warn_msg.to_string());
                                }
                            }
                        } else if let Some(idx) = val.get("select_screen").and_then(|v| v.as_i64()) {
                            info!("🖥️ [Web] Cambiando a monitor index: {}", idx);
                            let _ = state_clone.select_screen(idx as i32).await;
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
    info!("👥 [Web] Visor web desconectado. Espectadores restantes: {}", v_rem);
}

async fn serve_index(State(state): State<AppState>) -> Html<String> {
    let cfg = state.config.read().await;
    let html = format!(r#"<!DOCTYPE html>
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
        }}
        .view-panel {{
            display: none;
            width: 100%;
            height: 100%;
            flex-direction: column;
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
        }}
        .terminal-output {{
            flex: 1;
            overflow-y: auto;
            white-space: pre-wrap;
            color: #10b981;
            line-height: 1.5;
            padding-bottom: 8px;
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
    </style>
</head>
<body>
    <header>
        <div class="brand">
            ⚡ NxDesk <span>v0.1.0</span> <span class="badge">PURE RUST</span>
        </div>

        <!-- Buscador AnyDesk: Permite ingresar ID o IP remota para conectarse -->
        <div class="search-container">
            <input type="text" id="target-search-input" placeholder="🔍 Conectar a ID o IP remota (ej. 111-222-333 o 192.168.1.50:5050)...">
            <button class="btn btn-primary" onclick="connectTargetInput()">⚡ Conectar</button>
        </div>

        <div class="header-meta">
            <div>
                <span style="color: var(--text-muted); font-size: 0.85rem;">Mi ID:</span>
                <strong style="color: #60a5fa; font-family: monospace; font-size: 1.05rem; margin-left: 4px;">{device_id}</strong>
            </div>
            <!-- Contadores de Usuarios tipo AnyDesk -->
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
                <!-- Banner de Sesión Remota Activa (tipo AnyDesk) -->
                <div id="active-session-banner" class="notice-banner" style="background: rgba(16, 185, 129, 0.95); color: #042f2e; top: 16px;">
                    <span>🔴 Sesión activa con equipo remoto: <strong id="remote-peer-label">-</strong></span>
                    <button class="btn btn-danger" style="margin-left: 12px; padding: 4px 10px; font-size: 0.8rem;" onclick="disconnectRemoteSession()">🔌 Desconectar</button>
                </div>

                <div id="localhost-notice" class="notice-banner">
                    ⚠️ Modo Control activo en localhost: Para evitar bucles con tu cursor físico, se aplica filtro anti-rebote.
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

                    <button class="btn" onclick="toggleFullscreen()">⛶ Pantalla Completa</button>
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
            <div class="terminal-container">
                <div class="terminal-output" id="term-out">NxDesk Native Rust Terminal Session [VUL-01 Secured]&#10;Presione 'Desbloquear' o ejecute comandos con validación de seguridad activa.&#10;</div>
                <div class="terminal-input-bar">
                    <span>nxdesk&gt;</span>
                    <input type="text" id="term-in" placeholder="dir, ipconfig, whoami... (o 'cls' para limpiar)" autofocus>
                </div>
            </div>
        </div>

        <!-- Tab 4: Explorador de Archivos -->
        <div id="panel-files" class="view-panel">
            <div style="padding: 24px; overflow-y: auto;">
                <h3 style="margin-bottom: 16px;">📂 Explorador de Archivos Seguro (Sandbox: Downloads/NxDeskShared)</h3>
                <div id="files-list" style="background: var(--bg-surface); border-radius: 8px; border: 1px solid var(--border); padding: 16px;">
                    Cargando archivos...
                </div>
            </div>
        </div>
    </main>

    <script>
        const canvas = document.getElementById('viewport');
        const ctx = canvas.getContext('2d');
        let ws;
        let isControlEnabled = false; // MODO SOLO LECTURA POR DEFECTO (Previene bucles de ratón)
        const isLocalhost = (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1');

        function toggleControlMode() {{
            isControlEnabled = !isControlEnabled;
            const btn = document.getElementById('btn-control-toggle');
            const notice = document.getElementById('localhost-notice');

            if (isControlEnabled) {{
                btn.className = 'btn btn-mode-control';
                btn.textContent = '🕹️ Control Remoto Activo';
                canvas.classList.add('control-active');
                if (isLocalhost) {{
                    notice.style.display = 'block';
                    setTimeout(() => {{ notice.style.display = 'none'; }}, 6000);
                }}
            }} else {{
                btn.className = 'btn btn-mode-view';
                btn.textContent = '👁️ Modo: Solo Vista';
                canvas.classList.remove('control-active');
                notice.style.display = 'none';
            }}
        }}

        function initWebSocket() {{
            const loc = window.location;
            const wsUri = (loc.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + loc.host + '/ws/stream';
            ws = new WebSocket(wsUri);
            ws.binaryType = 'arraybuffer';

            ws.onmessage = function(event) {{
                if (typeof event.data === 'string') {{
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'terminal') {{
                        const out = document.getElementById('term-out');
                        out.textContent += msg.data;
                        out.scrollTop = out.scrollHeight;
                    }}
                }} else {{
                    const blob = new Blob([event.data], {{ type: 'image/jpeg' }});
                    const img = new Image();
                    img.onload = function() {{
                        if (canvas.width !== img.width || canvas.height !== img.height) {{
                            canvas.width = img.width;
                            canvas.height = img.height;
                        }}
                        ctx.drawImage(img, 0, 0);
                        URL.revokeObjectURL(img.src);
                    }};
                    img.src = URL.createObjectURL(blob);
                }}
            }};

            ws.onclose = function() {{
                document.getElementById('conn-status').textContent = 'Reconectando...';
                setTimeout(initWebSocket, 2000);
            }};
        }}
        initWebSocket();

        // Conectar usando el Buscador AnyDesk (por ID o IP)
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
                    sel.innerHTML = screens.map(s => `
                        <option value="${{s.Index}}">
                            ${{s.Name}} (${{s.Width}}x${{s.Height}})
                        </option>
                    `).join('');
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

        // 3. Inyección Segura de Eventos de Ratón (Throttled & Control-Gated)
        let lastMoveTime = 0;
        canvas.addEventListener('mousemove', (e) => {{
            if (!isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
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

        canvas.addEventListener('mousedown', (e) => {{
            if (!isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            const btn = e.button === 0 ? 'Left' : (e.button === 2 ? 'Right' : 'Middle');
            const rect = canvas.getBoundingClientRect();
            const x = (e.clientX - rect.left) / rect.width;
            const y = (e.clientY - rect.top) / rect.height;
            ws.send(JSON.stringify({{ Type: 'MouseDown', MouseButton: btn, X: x, Y: y }}));
        }});

        canvas.addEventListener('mouseup', (e) => {{
            if (!isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
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
            if (!isControlEnabled || !ws || ws.readyState !== WebSocket.OPEN) return;
            e.preventDefault();
            const deltaY = e.deltaY < 0 ? 1.0 : -1.0;
            ws.send(JSON.stringify({{ Type: 'MouseWheel', DeltaY: deltaY }}));
        }});

        // 4. Inyección Segura de Teclado
        window.addEventListener('keydown', (e) => {{
            if (!isControlEnabled) return;
            if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'SELECT') return;
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            ws.send(JSON.stringify({{ Type: 'KeyDown', Key: e.keyCode, KeyCode: e.code }}));
        }});

        window.addEventListener('keyup', (e) => {{
            if (!isControlEnabled) return;
            if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'SELECT') return;
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            ws.send(JSON.stringify({{ Type: 'KeyUp', Key: e.keyCode, KeyCode: e.code }}));
        }});

        // 5. Terminal Remota
        let isTerminalAllowed = false;
        const termInput = document.getElementById('term-in');

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

            if (ws && ws.readyState === WebSocket.OPEN) {{
                ws.send(JSON.stringify({{ terminal_cmd: trimmed }}));
                const out = document.getElementById('term-out');
                out.textContent += `\n> ${{trimmed}}\n`;
                out.scrollTop = out.scrollHeight;
            }} else {{
                alert("Conexión WebSocket no activa.");
            }}
        }}

        function sendQuickCmd(cmd) {{
            sendTerminalCommand(cmd);
        }}

        async function toggleTerminalAuth() {{
            let password = null;
            if (!isTerminalAllowed) {{
                password = prompt("Ingrese la clave administrativa de NxDesk (dejar en blanco si no configuró una):");
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
                }} else {{
                    alert("Error de autorización: " + data.message);
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
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.view-panel').forEach(p => p.classList.remove('active'));
            const tabBtn = Array.from(document.querySelectorAll('.tab-btn')).find(b => b.getAttribute('onclick') && b.getAttribute('onclick').includes(name));
            if (tabBtn) tabBtn.classList.add('active');
            const panel = document.getElementById('panel-' + name);
            if (panel) panel.classList.add('active');
            if (name === 'radar') refreshPeers();
            if (name === 'files') refreshFiles();
        }}

        function toggleFullscreen() {{
            if (!document.fullscreenElement) {{
                document.getElementById('viewport-container').requestFullscreen();
            }} else {{
                document.exitFullscreen();
            }}
        }}

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

        async function refreshFiles() {{
            const res = await fetch('/api/files');
            const data = await res.json();
            const list = document.getElementById('files-list');
            list.innerHTML = `<strong>Ruta: ${{data.CurrentPath}}</strong><br><br>` + 
                data.Items.map(item => `
                    <div style="padding: 6px 0; border-bottom: 1px solid #1f2937; display: flex; justify-content: space-between;">
                        <span>${{item.IsDirectory ? '📁 ' : '📄 '}}${{item.Name}}</span>
                        <span style="color: var(--text-muted); font-size: 0.85rem;">${{item.IsDirectory ? '<DIR>' : (item.Size / 1024).toFixed(1) + ' KB'}}</span>
                    </div>
                `).join('');
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

    Html(html)
}
