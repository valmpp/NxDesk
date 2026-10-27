use futures_util::{SinkExt, StreamExt};
use nxdesk_core::SignalingMessage;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

type Tx = mpsc::UnboundedSender<Message>;

/// Rastreador de salas y presencia de equipos (Equivalente a PeerTracker + SignalingHub en C#)
#[derive(Default)]
struct HubState {
    // roomId -> Set de IDs de conexión
    rooms: HashMap<String, HashSet<String>>,
    // connectionId -> Tx channel
    clients: HashMap<String, Tx>,
    // peerId -> connectionId
    peer_tracker: HashMap<String, String>,
    // connectionId -> roomId
    client_room: HashMap<String, String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let bind_addr_str = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".to_string());
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

async fn handle_connection(mut stream: TcpStream, peer_addr: SocketAddr, state: Arc<RwLock<HubState>>) {
    // Sonda de salud (Health Check /health) para balanceadores y monitoreo SRE
    let mut peek_buf = [0u8; 16];
    if let Ok(n) = stream.peek(&mut peek_buf).await {
        if n >= 4 && &peek_buf[0..4] == b"GET " {
            let peek_str = String::from_utf8_lossy(&peek_buf[0..n]);
            if peek_str.starts_with("GET /health") {
                use tokio::io::AsyncWriteExt;
                let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"healthy\",\"service\":\"nxdesk-server\"}";
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
                return;
            }
        }
    }

    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            warn!("Error durante el apretón de manos WebSocket desde {}: {}", peer_addr, e);
            return;
        }
    };

    let conn_id = format!("{}_{}", peer_addr, uuid_v4_simple());
    info!("🔗 Cliente conectado: {} ({})", conn_id, peer_addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Registrar cliente en el estado global
    {
        let mut s = state.write().await;
        s.clients.insert(conn_id.clone(), tx);
    }

    // Tarea para enviar mensajes al WebSocket desde el canal de salida
    let conn_id_tx = conn_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                warn!("[{}] Error enviando mensaje a socket: {}", conn_id_tx, e);
                break;
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
    // 1. Mensaje de unión de sala
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(action) = val.get("Type").and_then(|v| v.as_str()) {
            match action {
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

async fn join_room(conn_id: &str, room_id: &str, state: &Arc<RwLock<HubState>>) {
    let mut s = state.write().await;
    s.rooms.entry(room_id.to_string()).or_default().insert(conn_id.to_string());
    s.client_room.insert(conn_id.to_string(), room_id.to_string());
    s.peer_tracker.insert(room_id.to_string(), conn_id.to_string());

    let count = s.rooms.get(room_id).map(|set| set.len()).unwrap_or(0);
    info!("🚪 Cliente {} se unió a la sala '{}' (Total: {})", conn_id, room_id, count);

    // Notificar a todos en la sala el cambio de ocupación
    let notify_msg = format!("{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}", count);
    if let Some(members) = s.rooms.get(room_id) {
        for member_id in members {
            if let Some(tx) = s.clients.get(member_id) {
                let _ = tx.send(Message::Text(notify_msg.clone()));
            }
        }
    }
}

async fn leave_room(conn_id: &str, room_id: &str, state: &Arc<RwLock<HubState>>) {
    let mut s = state.write().await;
    let count = if let Some(members) = s.rooms.get_mut(room_id) {
        members.remove(conn_id);
        members.len()
    } else {
        0
    };
    s.client_room.remove(conn_id);

    info!("🚪 Cliente {} salió de la sala '{}' (Total: {})", conn_id, room_id, count);

    if count > 0 {
        let notify_msg = format!("{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}", count);
        if let Some(members) = s.rooms.get(room_id) {
            for member_id in members {
                if let Some(tx) = s.clients.get(member_id) {
                    let _ = tx.send(Message::Text(notify_msg.clone()));
                }
            }
        }
    } else {
        s.rooms.remove(room_id);
    }
}

async fn relay_message_to_room(sender_id: &str, msg: &SignalingMessage, state: &Arc<RwLock<HubState>>) {
    let s = state.read().await;
    if let Some(room_id) = s.client_room.get(sender_id) {
        if let Some(members) = s.rooms.get(room_id) {
            let json_str = format!("{}\u{1e}", serde_json::to_string(msg).unwrap_or_default());
            for member_id in members {
                if member_id != sender_id {
                    if let Some(tx) = s.clients.get(member_id) {
                        let _ = tx.send(Message::Text(json_str.clone()));
                    }
                }
            }
        }
    }
}

async fn relay_binary_to_room(sender_id: &str, data: Vec<u8>, state: &Arc<RwLock<HubState>>) {
    let s = state.read().await;
    if let Some(room_id) = s.client_room.get(sender_id) {
        if let Some(members) = s.rooms.get(room_id) {
            for member_id in members {
                if member_id != sender_id {
                    if let Some(tx) = s.clients.get(member_id) {
                        let _ = tx.send(Message::Binary(data.clone()));
                    }
                }
            }
        }
    }
}

async fn cleanup_connection(conn_id: &str, state: &Arc<RwLock<HubState>>) {
    let mut s = state.write().await;
    s.clients.remove(conn_id);
    if let Some(room_id) = s.client_room.remove(conn_id) {
        let count = if let Some(members) = s.rooms.get_mut(&room_id) {
            members.remove(conn_id);
            members.len()
        } else {
            0
        };

        if count > 0 {
            let notify_msg = format!("{{\"Type\":\"RoomOccupancyChanged\",\"Payload\":\"{}\"}}\u{1e}", count);
            if let Some(members) = s.rooms.get(&room_id) {
                for member_id in members {
                    if let Some(tx) = s.clients.get(member_id) {
                        let _ = tx.send(Message::Text(notify_msg.clone()));
                    }
                }
            }
        } else {
            s.rooms.remove(&room_id);
        }
    }
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", nanos)
}
