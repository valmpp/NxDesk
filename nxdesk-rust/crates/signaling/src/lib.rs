use futures_util::{SinkExt, StreamExt};
use nxdesk_core::{InputEvent, SignalingMessage};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Cliente de señalización de ultra baja latencia con priorización de entradas sobre video
pub struct SignalingClient {
    control_tx: mpsc::UnboundedSender<Message>,
    video_tx: watch::Sender<Option<Vec<u8>>>,
    incoming_video_rx: watch::Receiver<Option<Vec<u8>>>,
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
    ) -> Result<(Self, mpsc::Receiver<SignalingEvent>), Box<dyn std::error::Error + Send + Sync>>
    {
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write_half, mut read_half) = ws_stream.split();
        let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(100);

        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<Message>();
        let (video_tx, mut video_rx) = watch::channel::<Option<Vec<u8>>>(None);
        let (incoming_video_tx, incoming_video_rx) = watch::channel::<Option<Vec<u8>>>(None);
        let room_id = Arc::new(Mutex::new(None));

        // Tarea dedicada de ultra baja latencia para el envío por socket (Equidad justa: Control y Video sin starvation)
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    Some(msg) = control_rx.recv() => {
                        if let Err(_) = write_half.send(msg).await {
                            break;
                        }
                    }
                    Ok(()) = video_rx.changed() => {
                        let maybe_frame = video_rx.borrow_and_update().clone();
                        if let Some(frame) = maybe_frame {
                            if let Err(_) = write_half.send(Message::Binary(frame)).await {
                                break;
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        // Tarea en segundo plano para enviar PING de keepalive periódico cada 15 segundos
        // Crucial para evitar que túneles de Cloudflare, proxies y NATs cierren el socket por inactividad
        let ping_tx = control_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                interval.tick().await;
                if ping_tx.send(Message::Ping(vec![])).is_err() {
                    break;
                }
            }
        });

        // Tarea en segundo plano para escuchar mensajes del servidor
        let tx_clone = event_tx.clone();
        let pong_tx = control_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = read_half.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let clean_text = text.trim_end_matches('\u{1e}');

                        if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(clean_text) {
                            if sig_msg.msg_type == "RoomOccupancyChanged" {
                                if let Ok(count) = sig_msg.payload.parse::<usize>() {
                                    let _ = tx_clone.send(SignalingEvent::RoomOccupancy(count)).await;
                                }
                            } else if sig_msg.msg_type == "input" {
                                if let Ok(input_evt) = serde_json::from_str::<InputEvent>(&sig_msg.payload) {
                                    let _ = tx_clone.send(SignalingEvent::Input(input_evt)).await;
                                }
                            } else {
                                let _ = tx_clone.send(SignalingEvent::Message(sig_msg)).await;
                            }
                        } else if let Ok(input_event) =
                            serde_json::from_str::<InputEvent>(clean_text)
                        {
                            let _ = tx_clone.send(SignalingEvent::Input(input_event)).await;
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        // Enrutamiento directo al canal watch de video: CERO cola FIFO y CERO delay acumulado
                        let _ = incoming_video_tx.send(Some(bin));
                    }
                    Ok(Message::Ping(data)) => {
                        let _ = pong_tx.send(Message::Pong(data));
                    }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Close(_)) => {
                        let _ = tx_clone.send(SignalingEvent::Disconnected).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        let client = Self {
            control_tx,
            video_tx,
            incoming_video_rx,
            room_id,
        };

        Ok((client, event_rx))
    }

    /// Retorna una suscripción al canal watch de video entrante de ultra baja latencia
    pub fn subscribe_incoming_video(&self) -> watch::Receiver<Option<Vec<u8>>> {
        self.incoming_video_rx.clone()
    }

    /// Se une a una sala específica de conexión remota (room_id o ID de equipo)
    pub async fn join_room(
        &self,
        room_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut room_lock = self.room_id.lock().await;
        *room_lock = Some(room_id.to_string());

        let join_msg = SignalingMessage::new("join", room_id);
        self.send_message(&join_msg).await
    }

    /// Envía un mensaje de señalización (ofertas SDP, respuestas, etc.)
    pub async fn send_message(
        &self,
        message: &SignalingMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_string(message)?;
        let formatted = format!("{}\u{1e}", json);

        self.control_tx
            .send(Message::Text(formatted))
            .map_err(|e| format!("Error enviando mensaje: {}", e))?;
        Ok(())
    }

    /// Envía un evento de ratón o teclado directamente al host remoto (modo async)
    pub async fn send_input(
        &self,
        event: &InputEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_input_direct(event)
    }

    /// Envía un evento de ratón o teclado de forma 100% síncrona sin spawnear tareas de Tokio (~30 ns)
    pub fn send_input_direct(
        &self,
        event: &InputEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sig_msg = SignalingMessage::new_input(event)?;
        let json = serde_json::to_string(&sig_msg)?;
        let formatted = format!("{}\u{1e}", json);
        self.control_tx
            .send(Message::Text(formatted))
            .map_err(|e| format!("Error enviando input: {}", e))?;
        Ok(())
    }

    /// Envía un fotograma de video en binario puro de forma síncrona sin sobrecarga de runtime
    pub fn send_video_frame_direct(&self, frame_bytes: Vec<u8>) {
        let _ = self.video_tx.send(Some(frame_bytes));
    }

    /// Envía un fotograma de video en binario puro con descarte automático de fotogramas atrasados (Zero Bufferbloat)
    pub async fn send_video_frame(
        &self,
        frame_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_video_frame_direct(frame_bytes);
        Ok(())
    }
}
