use futures_util::{SinkExt, StreamExt};
use nxdesk_core::SignalingMessage;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
struct ClientHandle {
    control_tx: mpsc::UnboundedSender<Message>,
    video_tx: watch::Sender<Option<Vec<u8>>>,
}

/// Rastreador de salas y presencia de equipos (Equivalente a PeerTracker + SignalingHub en C#)
#[derive(Default)]
struct HubState {
    // roomId -> Set de IDs de conexión
    rooms: HashMap<String, HashSet<String>>,
    // connectionId -> ClientHandle (canal de control + canal de video con zero bufferbloat)
    clients: HashMap<String, ClientHandle>,
    // peerId -> connectionId
    peer_tracker: HashMap<String, String>,
    // connectionId -> Set de roomIds a los que pertenece
    client_rooms: HashMap<String, HashSet<String>>,
    // connectionId -> PeerIdentity (presencia de pares en línea)
    online_peers: HashMap<String, nxdesk_core::PeerIdentity>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args: Vec<String> = std::env::args().collect();
    let bind_addr_str = if let Some(pos) = args.iter().position(|a| a == "--bind") {
        args.get(pos + 1).cloned().unwrap_or_else(|| "0.0.0.0:5000".to_string())
    } else {
        std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".to_string())
    };
    let addr: SocketAddr = bind_addr_str.parse()?;
    let listener = TcpListener::bind(&addr).await?;

    info!("==================================================");
    info!("   🌐  NxDesk Signaling Server (Rust Native)       ");
    info!("   Escuchando en: http://{}/signalinghub", addr);
    info!("==================================================");

    let state = Arc::new(RwLock::new(HubState::default()));

    while let Ok((stream, peer_addr)) = listener.accept().await {
        let state_clone = Arc::clone(&state);
        tokio::spawn(handle_connection(stream, peer_addr, state_clone));
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    state: Arc<RwLock<HubState>>,
) {
    // Desactivar Nagle y acotar los buffers del kernel (192 KiB/dirección): el servidor no debe
    // acumular MBs de frames viejos por conexión cuando el túnel del visor es lento. El
    // backpressure real hace que cada watch de sala descarte en el origen.
    // (tokio no expone el tope de buffers: se ajusta vía socket2 sobre el socket nativo.)
    let std_stream = match stream.into_std() {
        Ok(s) => s,
        Err(e) => {
            warn!("No se pudo obtener el socket nativo: {}", e);
            return;
        }
    };
    {
        let sock = socket2::SockRef::from(&std_stream);
        let _ = sock.set_nodelay(true);
        // Búfer optimizado (128 KiB send / 256 KiB recv): relay rápido sin fragmentación para fotogramas Q>=80
        let _ = sock.set_send_buffer_size(128 * 1024);
        let _ = sock.set_recv_buffer_size(256 * 1024);
        let _ = std_stream.set_nonblocking(true);
    }
    let mut stream: TcpStream = match TcpStream::from_std(std_stream) {
        Ok(s) => s,
        Err(e) => {
            warn!("No se pudo reenvolver el socket ajustado: {}", e);
            return;
        }
    };

    // Inspección de encabezados HTTP: /health, descarga de ZIP o WebSocket
    let mut peek_buf = [0u8; 1024];
    let n = match stream.peek(&mut peek_buf).await {
        Ok(n) if n >= 4 => n,
        _ => return,
    };

    let peek_str = String::from_utf8_lossy(&peek_buf[..n]);
    let is_websocket = peek_str.to_lowercase().contains("upgrade: websocket");

    if !is_websocket {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Consumir la petición HTTP del socket para vaciar el receive buffer y evitar TCP RST
        let mut req_bytes = Vec::new();
        let mut temp = [0u8; 512];
        while let Ok(n_read) = stream.read(&mut temp).await {
            if n_read == 0 {
                break;
            }
            req_bytes.extend_from_slice(&temp[..n_read]);
            if req_bytes.windows(4).any(|w| w == b"\r\n\r\n") || req_bytes.len() >= 4096 {
                break;
            }
        }

        let req_str = String::from_utf8_lossy(&req_bytes);
        let first_line = req_str.lines().next().unwrap_or_default();
        let is_head = first_line.starts_with("HEAD");
        let path = first_line.split_whitespace().nth(1).unwrap_or("/");

        if path == "/health" || path.starts_with("/health") {
            let body = "{\"status\":\"healthy\",\"service\":\"nxdesk-server\"}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
            return;
        } else if path == "/download"
            || path.starts_with("/download")
            || path.starts_with("/NxDesk-Portable")
            || path.starts_with("/dist/")
        {
            let candidate_paths = [
                std::path::PathBuf::from("/home/nxdesk/dist/NxDesk-Portable-v1.0-Windows-x64.zip"),
                std::path::PathBuf::from("dist/NxDesk-Portable-v1.0-Windows-x64.zip"),
                std::path::PathBuf::from("../../dist/NxDesk-Portable-v1.0-Windows-x64.zip"),
            ];

            let mut zip_data = None;
            for p in &candidate_paths {
                if let Ok(bytes) = std::fs::read(p) {
                    info!("📦 [HTTP Download] Sirviendo archivo portable ({} bytes) desde {:?}", bytes.len(), p);
                    zip_data = Some(bytes);
                    break;
                }
            }

            if let Some(bytes) = zip_data {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"NxDesk-Portable-v1.0-Windows-x64.zip\"\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;

                if !is_head {
                    for chunk in bytes.chunks(65536) {
                        if let Err(e) = stream.write_all(chunk).await {
                            warn!("Error enviando bloque de descarga: {}", e);
                            break;
                        }
                    }
                }
                let _ = stream.flush().await;
                let _ = stream.shutdown().await;
            } else {
                warn!("⚠️ [HTTP Download] Archivo portable no encontrado en las rutas candidatas");
                let err_body = "Archivo portable no encontrado en el servidor. Por favor ejecute la empaquetación.";
                let header = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_body.len(),
                    err_body
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.flush().await;
                let _ = stream.shutdown().await;
            }
            return;
        } else {
            // Respuesta amigable para navegadores web que acceden al servidor de señalización
            let html = r#"<!DOCTYPE html>
<html lang="es">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>NxDesk — Servidor de Señalización</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f172a; color: #f8fafc; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; padding: 1.5rem; box-sizing: border-box; }
        .card { background: #1e293b; border: 1px solid #334155; border-radius: 14px; padding: 2.2rem; max-width: 600px; width: 100%; box-shadow: 0 15px 35px rgba(0,0,0,0.5); text-align: center; }
        .badge { display: inline-flex; align-items: center; gap: 6px; padding: 0.35rem 0.85rem; border-radius: 9999px; font-size: 0.875rem; font-weight: 600; background: rgba(16, 185, 129, 0.15); color: #34d399; border: 1px solid rgba(52, 211, 153, 0.3); margin-bottom: 1.25rem; }
        .dot { width: 8px; height: 8px; background: #10b981; border-radius: 50%; box-shadow: 0 0 8px #10b981; }
        h1 { margin: 0 0 0.75rem 0; font-size: 1.6rem; color: #ffffff; }
        p { color: #94a3b8; font-size: 0.95rem; line-height: 1.6; margin-bottom: 1.25rem; }
        .btn-download { display: block; background: #10b981; color: #ffffff; padding: 1rem 1.75rem; border-radius: 10px; text-decoration: none; font-weight: 700; font-size: 1.05rem; box-shadow: 0 4px 16px rgba(16,185,129,0.35); transition: all 0.2s; margin-bottom: 1.25rem; }
        .btn-download:hover { background: #059669; transform: translateY(-1px); }
        .notice-box { background: rgba(234, 179, 8, 0.1); border: 1px solid rgba(234, 179, 8, 0.3); border-radius: 8px; padding: 1rem; font-size: 0.85rem; text-align: left; color: #fef08a; margin-bottom: 1.25rem; line-height: 1.6; }
        .notice-box strong { color: #facc15; }
        .info-box { background: #0f172a; border-radius: 8px; padding: 1.2rem; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.85rem; text-align: left; color: #38bdf8; margin-bottom: 1rem; border: 1px solid #334155; line-height: 1.8; }
        .cmd-box { background: #020617; border: 1px solid #1e293b; border-radius: 6px; padding: 0.6rem 0.8rem; font-family: monospace; color: #a5f3fc; font-size: 0.8rem; user-select: all; word-break: break-all; margin-top: 0.5rem; }
    </style>
</head>
<body>
    <div class="card">
        <span class="badge"><span class="dot"></span> Servidor Activo y Operacional</span>
        <h1>🌐 NxDesk Signaling Server</h1>
        <p>Descarga la aplicación cliente de escritorio portátil para conectarte y controlar computadoras de forma remota:</p>
        
        <a href="/download" class="btn-download">📥 Descargar NxDesk Portable para Windows (.ZIP — 4.18 MB)</a>

        <div class="notice-box">
            <strong>⚠️ Nota para Google Chrome / Microsoft Edge:</strong><br>
            Al provenir de un túnel Cloudflare sin certificado comercial EV, el navegador puede mostrar <em>"El archivo se bloqueó"</em>. Pulsa <code>Ctrl + J</code> en el navegador y haz clic en <strong>"Conservar archivo" → "Mantener de todos modos"</strong>.
        </div>

        <div class="info-box">
            📡 Protocolo: WebSocket (RFC 6455)<br>
            🔗 Endpoint WAN: /signalinghub<br>
            🩺 Sonda SRE: /health (HTTP 200 OK)<br>
            💻 Descarga alternativa por PowerShell:<br>
            <div class="cmd-box">iwr -Uri &quot;https://antenna-joined-watches-tattoo.trycloudflare.com/download&quot; -OutFile NxDesk.zip; Expand-Archive NxDesk.zip -Force</div>
        </div>
    </div>
</body>
</html>"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
            return;
        }
    }

    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            warn!(
                "Error durante el apretón de manos WebSocket desde {}: {}",
                peer_addr, e
            );
            return;
        }
    };

    let conn_id = format!("{}_{}", peer_addr, uuid_v4_simple());
    info!("🔗 Cliente conectado: {} ({})", conn_id, peer_addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<Message>();
    let (video_tx, mut video_rx) = watch::channel::<Option<Vec<u8>>>(None);
    let tx_pong = control_tx.clone();

    let client_handle = ClientHandle {
        control_tx,
        video_tx,
    };

    // Registrar cliente en el estado global
    {
        let mut s = state.write().await;
        s.clients.insert(conn_id.clone(), client_handle);
    }

    // Tarea para enviar mensajes al WebSocket desde el canal de salida con equidad justa
    // Control / Ping / Entrada y Video se intercalan sin inanición (Zero Starvation, Zero Bufferbloat).
    // Descarte anti-cola: si una escritura de video tarda >120 ms, los frames frescos se descartan
    // en el watch (en vez de encolarse en TCP) mientras la ventana de drenado aguanta; el RTT de
    // control del par no se arrastra por detrás de frames obsoletos en sesiones largas.
    let conn_id_tx = conn_id.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = control_rx.recv() => {
                    if let Err(e) = ws_sender.send(msg).await {
                        warn!("[{}] Error enviando control a socket: {}", conn_id_tx, e);
                        break;
                    }
                }
                Ok(()) = video_rx.changed() => {
                    let maybe_frame = video_rx.borrow_and_update().clone();
                    if let Some(frame_bytes) = maybe_frame {
                        if let Err(e) = ws_sender.send(Message::Binary(frame_bytes)).await {
                            warn!("[{}] Error enviando frame a socket: {}", conn_id_tx, e);
                            break;
                        }
                    }
                }
                else => break,
            }
        }
    });

    // Bucle para recibir mensajes del cliente
    while let Some(msg_res) = ws_receiver.next().await {
        match msg_res {
            Ok(Message::Text(raw)) => {
                let text = raw.trim_end_matches('\u{1e}');
                handle_client_text(&conn_id, text, &state).await;
            }
            Ok(Message::Binary(bin)) => {
                // Retransmitir frames binarios al resto de participantes de la sala
                relay_binary_to_room(&conn_id, bin, &state).await;
            }
            Ok(Message::Ping(data)) => {
                let _ = tx_pong.send(Message::Pong(data));
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!("[{}] Error de lectura en WebSocket: {}", conn_id, e);
                break;
            }
            _ => {}
        }
    }

    // Limpieza al desconectarse
    cleanup_connection(&conn_id, &state).await;
    info!("❌ Cliente desconectado: {}", conn_id);
}

async fn handle_client_text(conn_id: &str, text: &str, state: &Arc<RwLock<HubState>>) {
    // 1. Mensaje de unión de sala o anuncio de presencia
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(action) = val.get("Type").and_then(|v| v.as_str()) {
            match action {
                "announce" => {
                    if let Some(payload_str) = val.get("Payload").and_then(|v| v.as_str()) {
                        if let Ok(peer) = serde_json::from_str::<nxdesk_core::PeerIdentity>(payload_str) {
                            handle_peer_announce(conn_id, peer, state).await;
                        }
                    }
                    return;
                }
                "join" => {
                    if let Some(room_id) = val.get("Payload").and_then(|v| v.as_str()) {
                        join_room(conn_id, room_id, state).await;
                    }
                    return;
                }
                "leave" => {
                    if let Some(room_id) = val.get("Payload").and_then(|v| v.as_str()) {
                        leave_room(conn_id, room_id, state).await;
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    // 2. Retransmisión general de SignalingMessage
    if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(text) {
        relay_message_to_room(conn_id, &sig_msg, state).await;
    }
}

async fn handle_peer_announce(
    conn_id: &str,
    peer: nxdesk_core::PeerIdentity,
    state: &Arc<RwLock<HubState>>,
) {
    let mut s = state.write().await;
    s.online_peers.insert(conn_id.to_string(), peer.clone());
    s.peer_tracker.insert(peer.id.clone(), conn_id.to_string());
    info!(
        "📢 [Presencia WAN] Par anunciado en red: '{}' ({})",
        peer.alias, peer.id
    );

    // 1. Enviar al nuevo par la lista de todos los pares ya conectados
    let existing_peers: Vec<nxdesk_core::PeerIdentity> = s
        .online_peers
        .iter()
        .filter(|(cid, _)| *cid != conn_id)
        .map(|(_, p)| p.clone())
        .collect();

    if let Some(client) = s.clients.get(conn_id) {
        if let Ok(json_list) = serde_json::to_string(&existing_peers) {
            let msg = SignalingMessage::new("peer_list", &json_list);
            let formatted = format!("{}\u{1e}", serde_json::to_string(&msg).unwrap_or_default());
            let _ = client.control_tx.send(Message::Text(formatted));
        }
    }

    // 2. Notificar a todos los demás clientes que este par está en línea
    if let Ok(peer_json) = serde_json::to_string(&peer) {
        let msg = SignalingMessage::new("peer_online", &peer_json);
        let formatted = format!("{}\u{1e}", serde_json::to_string(&msg).unwrap_or_default());
        for (cid, client) in &s.clients {
            if cid != conn_id {
                let _ = client.control_tx.send(Message::Text(formatted.clone()));
            }
        }
    }
}

async fn join_room(conn_id: &str, room_id: &str, state: &Arc<RwLock<HubState>>) {
    // Mitigación ID 02: Validar formato alfanumérico seguro para el ID de sala
    let trimmed = room_id.trim();
    if trimmed.is_empty()
        || trimmed.len() > 64
        || !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        warn!(
            "⛔ [Seguridad] Intento de unirse con Room ID inválido: '{}' desde {}",
            room_id, conn_id
        );
        return;
    }

    let mut s = state.write().await;
    let HubState {
        ref mut rooms,
        ref clients,
        ref mut client_rooms,
        ref mut peer_tracker,
        ..
    } = *s;

    // 1. Limpieza proactiva de sockets desconectados en la sala
    if let Some(members) = rooms.get_mut(trimmed) {
        members.retain(|mid| clients.contains_key(mid));
    }

    let active_count = rooms.get(trimmed).map(|m| m.len()).unwrap_or(0);
    if active_count >= 20 {
        warn!(
            "⛔ [Seguridad] Sala '{}' alcanzó el cupo máximo de 20 conexiones activas. Rechazando a {}",
            trimmed, conn_id
        );
        return;
    }

    rooms
        .entry(trimmed.to_string())
        .or_default()
        .insert(conn_id.to_string());
    client_rooms
        .entry(conn_id.to_string())
        .or_default()
        .insert(trimmed.to_string());
    peer_tracker
        .insert(trimmed.to_string(), conn_id.to_string());

    let count = rooms.get(trimmed).map(|set| set.len()).unwrap_or(0);
    info!(
        "🚪 Cliente {} se unió a la sala '{}' (Total activos: {})",
        conn_id, trimmed, count
    );

    // Notificar a todos en la sala el cambio de ocupación
    let notify_msg = format!(
        "{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}",
        count
    );
    if let Some(members) = rooms.get(trimmed) {
        for member_id in members {
            if let Some(client) = clients.get(member_id) {
                let _ = client.control_tx.send(Message::Text(notify_msg.clone()));
            }
        }
    }
}

async fn leave_room(conn_id: &str, room_id: &str, state: &Arc<RwLock<HubState>>) {
    let mut s = state.write().await;
    let HubState {
        ref mut rooms,
        ref clients,
        ref mut client_rooms,
        ..
    } = *s;

    if let Some(r_set) = client_rooms.get_mut(conn_id) {
        r_set.remove(room_id);
    }
    let count = if let Some(members) = rooms.get_mut(room_id) {
        members.remove(conn_id);
        members.retain(|mid| clients.contains_key(mid));
        members.len()
    } else {
        0
    };

    info!(
        "🚪 Cliente {} salió de la sala '{}' (Total activos: {})",
        conn_id, room_id, count
    );

    if count > 0 {
        let notify_msg = format!(
            "{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}",
            count
        );
        if let Some(members) = rooms.get(room_id) {
            for member_id in members {
                if let Some(client) = clients.get(member_id) {
                    let _ = client.control_tx.send(Message::Text(notify_msg.clone()));
                }
            }
        }
    } else {
        rooms.remove(room_id);
    }
}

async fn relay_message_to_room(
    sender_id: &str,
    msg: &SignalingMessage,
    state: &Arc<RwLock<HubState>>,
) {
    let s = state.read().await;
    if let Some(rooms) = s.client_rooms.get(sender_id) {
        let json_str = format!("{}\u{1e}", serde_json::to_string(msg).unwrap_or_default());
        let mut recipients = HashSet::new();
        for room_id in rooms {
            if let Some(members) = s.rooms.get(room_id) {
                for member_id in members {
                    if member_id != sender_id {
                        recipients.insert(member_id.clone());
                    }
                }
            }
        }
        for member_id in recipients {
            if let Some(client) = s.clients.get(&member_id) {
                let _ = client.control_tx.send(Message::Text(json_str.clone()));
            }
        }
    }
}

async fn relay_binary_to_room(sender_id: &str, data: Vec<u8>, state: &Arc<RwLock<HubState>>) {
    let s = state.read().await;
    if let Some(rooms) = s.client_rooms.get(sender_id) {
        let mut recipients = HashSet::new();
        for room_id in rooms {
            if let Some(members) = s.rooms.get(room_id) {
                for member_id in members {
                    if member_id != sender_id {
                        recipients.insert(member_id.clone());
                    }
                }
            }
        }
        for member_id in recipients {
            if let Some(client) = s.clients.get(&member_id) {
                let _ = client.video_tx.send(Some(data.clone()));
            }
        }
    }
}

async fn cleanup_connection(conn_id: &str, state: &Arc<RwLock<HubState>>) {
    let mut s = state.write().await;
    s.clients.remove(conn_id);
    if let Some(peer) = s.online_peers.remove(conn_id) {
        s.peer_tracker.remove(&peer.id);
        info!(
            "📢 [Presencia WAN] Par desconectado: '{}' ({})",
            peer.alias, peer.id
        );
        let msg = SignalingMessage::new("peer_offline", &peer.id);
        let formatted = format!("{}\u{1e}", serde_json::to_string(&msg).unwrap_or_default());
        for client in s.clients.values() {
            let _ = client.control_tx.send(Message::Text(formatted.clone()));
        }
    }

    let HubState {
        ref mut rooms,
        ref clients,
        ref mut client_rooms,
        ..
    } = *s;

    if let Some(r_set) = client_rooms.remove(conn_id) {
        for room_id in r_set {
            let count = if let Some(members) = rooms.get_mut(&room_id) {
                members.remove(conn_id);
                members.retain(|mid| clients.contains_key(mid));
                members.len()
            } else {
                0
            };

            if count > 0 {
                let notify_msg = format!(
                    "{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}",
                    count
                );
                if let Some(members) = rooms.get(&room_id) {
                    for member_id in members {
                        if let Some(client) = clients.get(member_id) {
                            let _ = client.control_tx.send(Message::Text(notify_msg.clone()));
                        }
                    }
                }
            } else {
                rooms.remove(&room_id);
            }
        }
    }

    // Limpieza de seguridad en todo rooms para eliminar cualquier referencia residual
    for members in rooms.values_mut() {
        members.remove(conn_id);
    }
    rooms.retain(|_, members| !members.is_empty());
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}
