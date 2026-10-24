use futures_util::{SinkExt, StreamExt};
use nxdesk_core::{InputEvent, SignalingMessage};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, WebSocketStream};

/// Cliente de señalización basado en WebSockets para NxDesk
pub struct SignalingClient {
    write_half: Arc<Mutex<futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    room_id: Arc<Mutex<Option<String>>>,
}

pub enum SignalingEvent {
    Message(SignalingMessage),
    Input(InputEvent),
    RoomOccupancy(usize),
    Disconnected,
}

impl SignalingClient {
    /// Se conecta a la URL del servidor de señalización (ej. "ws://localhost:5000/signalinghub")
    pub async fn connect(
        url: &str,
    ) -> Result<(Self, mpsc::Receiver<SignalingEvent>), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(url).await?;
        let (write_half, mut read_half) = ws_stream.split();
        let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(100);

        let write_mutex = Arc::new(Mutex::new(write_half));
        let room_id = Arc::new(Mutex::new(None));

        // Tarea en segundo plano para escuchar mensajes del servidor
        let tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = read_half.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        // El protocolo SignalR estándar termina sus mensajes con el caracter 0x1E (Record Separator)
                        let clean_text = text.trim_end_matches('\u{1e}');
                        
                        // Intentar parsear como SignalingMessage
                        if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(clean_text) {
                            let _ = tx_clone.send(SignalingEvent::Message(sig_msg)).await;
                        } else if let Ok(input_event) = serde_json::from_str::<InputEvent>(clean_text) {
                            let _ = tx_clone.send(SignalingEvent::Input(input_event)).await;
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        let sig_msg = SignalingMessage::new_frame(bin);
                        let _ = tx_clone.send(SignalingEvent::Message(sig_msg)).await;
                    }
                    Ok(Message::Close(_)) => {
                        let _ = tx_clone.send(SignalingEvent::Disconnected).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        let client = Self {
            write_half: write_mutex,
            room_id,
        };

        Ok((client, event_rx))
    }

    /// Se une a una sala específica de conexión remota (room_id o ID de equipo)
    pub async fn join_room(&self, room_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut room_lock = self.room_id.lock().await;
        *room_lock = Some(room_id.to_string());

        let join_msg = SignalingMessage::new("join", room_id);
        self.send_message(&join_msg).await
    }

    /// Envía un mensaje de señalización (ofertas SDP, respuestas, etc.)
    pub async fn send_message(&self, message: &SignalingMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_string(message)?;
        // Formato SignalR: json + delimitador 0x1E
        let formatted = format!("{}\u{1e}", json);

        let mut writer = self.write_half.lock().await;
        writer.send(Message::Text(formatted.into())).await?;
        Ok(())
    }

    /// Envía un evento de ratón o teclado directamente al host remoto
    pub async fn send_input(&self, event: &InputEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sig_msg = SignalingMessage::new_input(event)?;
        self.send_message(&sig_msg).await
    }

    /// Envía un fotograma de video en binario puro
    pub async fn send_video_frame(&self, frame_bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = self.write_half.lock().await;
        writer.send(Message::Binary(frame_bytes.into())).await?;
        Ok(())
    }
}
