use futures_util::{SinkExt, StreamExt};
use nxdesk_core::{InputEvent, SignalingMessage};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::Message;

static UDP_FRAME_SEQ: AtomicU32 = AtomicU32::new(1);

/// Reensamblador de datagramas UDP para streaming de video con Zero Head-of-Line Blocking
pub struct UdpFrameAssembler {
    pub current_seq: u32,
    pub chunks_received: usize,
    pub total_chunks: usize,
    pub chunks: Vec<Option<Vec<u8>>>,
}

impl UdpFrameAssembler {
    pub fn new() -> Self {
        Self {
            current_seq: 0,
            chunks_received: 0,
            total_chunks: 0,
            chunks: Vec::new(),
        }
    }

    pub fn add_chunk(&mut self, seq: u32, chunk_idx: usize, total_chunks: usize, data: &[u8]) -> Option<Vec<u8>> {
        if total_chunks == 0 || total_chunks > 1024 || chunk_idx >= total_chunks {
            return None;
        }

        // Si llega un fotograma más nuevo, descartamos el previo incompleto (Zero-Lag / Cero bufferbloat)
        let is_newer = if self.current_seq == 0 {
            true
        } else if seq > self.current_seq {
            seq - self.current_seq < 0x8000_0000
        } else if seq < self.current_seq {
            self.current_seq - seq > 0x8000_0000
        } else {
            false
        };

        if is_newer {
            self.current_seq = seq;
            self.chunks_received = 0;
            self.total_chunks = total_chunks;
            self.chunks.clear();
            self.chunks.resize(total_chunks, None);
        } else if seq != self.current_seq {
            return None;
        }

        if self.chunks.len() == total_chunks && self.chunks[chunk_idx].is_none() {
            self.chunks[chunk_idx] = Some(data.to_vec());
            self.chunks_received += 1;

            if self.chunks_received == total_chunks {
                let total_len: usize = self.chunks.iter().filter_map(|c| c.as_ref().map(|d| d.len())).sum();
                let mut full_frame = Vec::with_capacity(total_len);
                for c in &self.chunks {
                    if let Some(ref d) = c {
                        full_frame.extend_from_slice(d);
                    }
                }
                self.chunks_received = 0;
                self.chunks.clear();
                return Some(full_frame);
            }
        }

        None
    }
}

/// Cliente de señalización de ultra baja latencia con priorización de entradas sobre video y canal P2P UDP directo
pub struct SignalingClient {
    control_tx: mpsc::UnboundedSender<Message>,
    video_tx: watch::Sender<Option<Vec<u8>>>,
    incoming_video_rx: watch::Receiver<Option<Vec<u8>>>,
    room_id: Arc<Mutex<Option<String>>>,
    pub last_socket_send_us: Arc<std::sync::atomic::AtomicU32>,
    p2p_socket: Option<Arc<tokio::net::UdpSocket>>,
    p2p_remote_addr: Arc<std::sync::RwLock<Option<std::net::SocketAddr>>>,
    p2p_public_addr: Arc<std::sync::RwLock<Option<std::net::SocketAddr>>>,
    is_p2p_active: Arc<AtomicBool>,
    local_ip: Option<std::net::IpAddr>,
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
        // Conexión TCP manual para desactivar Nagle (TCP_NODELAY): sin esto, los mensajes pequeños
        // de control (ping/pong de RTT, entrada de teclado/ratón y session_end) se acumulan detrás
        // del flujo saturado de video y el delay medido se infla a 300–400 ms aunque la red real
        // sea de pocos milisegundos.
        let uri: Uri = url.parse()?;
        let scheme = uri.scheme_str().unwrap_or("ws").to_string();
        let host = uri
            .host()
            .ok_or_else(|| format!("URL sin host: {}", url))?
            .to_string();
        let default_port = if scheme == "wss" || scheme == "https" { 443 } else { 80 };
        let port = uri.port_u16().unwrap_or(default_port);

        let tcp: tokio::net::TcpStream = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
        let local_addr = tcp.local_addr().ok();
        let local_ip = local_addr.map(|a| a.ip());
        let _ = tcp.set_nodelay(true);
        // Encolado máximo del kernel: 192 KiB por dirección. Sin esto, Windows auto-ajusta los
        // buffers hasta varios MB y un túnel lento genera colas invisibles de 300–1000 ms: el frame
        // fresco queda atrapado detrás de frames viejos. Con el tope, el backpressure llega al
        // watch-channel que descarta en el origen (Zero Bufferbloat end-to-end).
        // (tokio no expone el tope de buffers: se ajusta vía socket2 sobre el socket nativo.)
        let std_tcp = tcp.into_std()?;
        let sock = socket2::SockRef::from(&std_tcp);
        // Búfer optimizado (128 KiB send / 256 KiB recv): acomoda perfectamente un fotograma completo
        // de calidad >=80 (65-90 KiB) en un único vuelo TCP sin fragmentación ni bloqueos intermedios por ACKs,
        // al tiempo que previene el bufferbloat masivo en túneles WAN.
        let _ = sock.set_send_buffer_size(128 * 1024);
        let _ = sock.set_recv_buffer_size(256 * 1024);
        let _ = std_tcp.set_nonblocking(true);
        let tcp = tokio::net::TcpStream::from_std(std_tcp)?;

        let request = url.into_client_request()?;
        let ws_stream = if scheme == "wss" || scheme == "https" {
            let tls_connector = native_tls::TlsConnector::builder().build()?;
            let connector = tokio_tungstenite::Connector::NativeTls(tls_connector);
            let (ws, _) =
                tokio_tungstenite::client_async_tls_with_config(request, tcp, None, Some(connector))
                    .await?;
            ws
        } else {
            let plain = tokio_tungstenite::MaybeTlsStream::Plain(tcp);
            let (ws, _) = tokio_tungstenite::client_async(request, plain).await?;
            ws
        };
        let (mut write_half, mut read_half) = ws_stream.split();
        let (event_tx, event_rx) = mpsc::channel::<SignalingEvent>(100);

        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<Message>();
        let (video_tx, mut video_rx) = watch::channel::<Option<Vec<u8>>>(None);
        let (incoming_video_tx, incoming_video_rx) = watch::channel::<Option<Vec<u8>>>(None);
        let room_id = Arc::new(Mutex::new(None));
        let last_socket_send_us = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sock_metric = last_socket_send_us.clone();

        // Tarea dedicada de ultra baja latencia para el envío por socket (Equidad justa: Control y Video sin starvation).
        // Descarte anti-cola: si una escritura de video tarda más de un frame (~33 ms), la cola TCP
        // está creciendo; entonces se DESCARTAN los frames obsoletos en el origen (mientras el socket
        // drena) en lugar de encolarlos: el RTT de control y el video se auto-estabilizan en sesiones largas.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    Some(msg) = control_rx.recv() => {
                        // Prioridad estricta: ping_rtt/input/session_end se envían inmediatamente sin esperar a video
                        if let Err(_) = write_half.send(msg).await {
                            break;
                        }
                    }
                    Ok(()) = video_rx.changed() => {
                        let maybe_frame = video_rx.borrow_and_update().clone();
                        if let Some(frame) = maybe_frame {
                            let frame_len = frame.len();
                            let t_sock = std::time::Instant::now();
                            if let Err(e) = write_half.send(Message::Binary(frame)).await {
                                tracing::error!("❌ [Socket Video] Error al enviar fotograma por WebSocket: {}", e);
                                break;
                            }
                            let sock_us = t_sock.elapsed().as_micros() as u32;
                            sock_metric.store(sock_us, std::sync::atomic::Ordering::Relaxed);
                            let sock_ms = sock_us as f32 / 1000.0;
                            if sock_ms > 20.0 {
                                tracing::warn!(
                                    "⚠️ [Congestión Socket TCP] write_half.send tardó {:.1}ms para {} KB (Buffer TCP kernel saturado / cuello de botella WAN)",
                                    sock_ms,
                                    frame_len / 1024
                                );
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
        let incoming_video_tx_ws = incoming_video_tx.clone();
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
                                    if matches!(input_evt, InputEvent::MouseMove { .. }) {
                                        // Movimiento de ratón: try_send para no bloquear jamás el lector WebSocket
                                        let _ = tx_clone.try_send(SignalingEvent::Input(input_evt));
                                    } else {
                                        let _ = tx_clone.send(SignalingEvent::Input(input_evt)).await;
                                    }
                                }
                            } else {
                                let _ = tx_clone.send(SignalingEvent::Message(sig_msg)).await;
                            }
                        } else if let Ok(input_event) =
                            serde_json::from_str::<InputEvent>(clean_text)
                        {
                            if matches!(input_event, InputEvent::MouseMove { .. }) {
                                let _ = tx_clone.try_send(SignalingEvent::Input(input_event));
                            } else {
                                let _ = tx_clone.send(SignalingEvent::Input(input_event)).await;
                            }
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        // Enrutamiento directo al canal watch de video: CERO cola FIFO y CERO delay acumulado
                        let _ = incoming_video_tx_ws.send(Some(bin));
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

        // Inicializar socket UDP para canal P2P de ultra baja latencia
        let udp_sock = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => Some(Arc::new(s)),
            Err(_) => None,
        };
        let p2p_remote_addr = Arc::new(std::sync::RwLock::new(None));
        let p2p_public_addr = Arc::new(std::sync::RwLock::new(None));
        let is_p2p_active = Arc::new(AtomicBool::new(false));

        if let Some(ref sock) = udp_sock {
            // Sondeo STUN en segundo plano para descubrir IP pública y puerto NAT WAN (RFC 5389)
            let stun_sock = sock.clone();
            tokio::spawn(async move {
                if let Ok(mut addrs) = tokio::net::lookup_host("stun.l.google.com:19302").await {
                    if let Some(stun_target) = addrs.next() {
                        let mut stun_req = [0u8; 20];
                        stun_req[0] = 0x00;
                        stun_req[1] = 0x01; // Binding Request
                        stun_req[4] = 0x21;
                        stun_req[5] = 0x12;
                        stun_req[6] = 0xA4;
                        stun_req[7] = 0x42; // Magic Cookie
                        for i in 8..20 {
                            stun_req[i] = (i * 23 + 7) as u8;
                        }
                        for _ in 0..3 {
                            let _ = stun_sock.send_to(&stun_req, stun_target).await;
                            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                        }
                    }
                }
            });

            let sock_recv = sock.clone();
            let p2p_addr_clone = p2p_remote_addr.clone();
            let p2p_public_clone = p2p_public_addr.clone();
            let p2p_active_clone = is_p2p_active.clone();
            let video_tx_udp = incoming_video_tx.clone();

            tokio::spawn(async move {
                let mut buf = [0u8; 65535];
                let mut assembler = UdpFrameAssembler::new();

                while let Ok((len, from_addr)) = sock_recv.recv_from(&mut buf).await {
                    if len >= 20 && buf[0] == 0x01 && buf[1] == 0x01 && &buf[4..8] == [0x21, 0x12, 0xA4, 0x42] {
                        // Respuesta STUN exitosa: extraer XOR-MAPPED-ADDRESS
                        let mut offset = 20;
                        while offset + 4 <= len {
                            let attr_type = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
                            let attr_len = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
                            offset += 4;
                            if offset + attr_len > len {
                                break;
                            }
                            if attr_type == 0x0020 && attr_len >= 8 {
                                let family = buf[offset + 1];
                                if family == 0x01 {
                                    let port = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) ^ 0x2112;
                                    let ip = std::net::Ipv4Addr::new(
                                        buf[offset + 4] ^ 0x21,
                                        buf[offset + 5] ^ 0x12,
                                        buf[offset + 6] ^ 0xA4,
                                        buf[offset + 7] ^ 0x42,
                                    );
                                    let ext_addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ip), port);
                                    if let Ok(mut lock) = p2p_public_clone.write() {
                                        *lock = Some(ext_addr);
                                    }
                                    tracing::info!("🌐 [STUN NAT] IP Pública descubierta para P2P WAN: {}", ext_addr);
                                    break;
                                }
                            }
                            offset += (attr_len + 3) & !3;
                        }
                    } else if len >= 10 && &buf[..10] == b"NXP2P_PING" {
                        let _ = sock_recv.send_to(b"NXP2P_PONG", from_addr).await;
                        if let Ok(mut lock) = p2p_addr_clone.write() {
                            *lock = Some(from_addr);
                        }
                        p2p_active_clone.store(true, Ordering::Relaxed);
                        tracing::info!("⚡ [P2P UDP] NXP2P_PING recibido de {}. Enlace activo.", from_addr);
                    } else if len >= 10 && &buf[..10] == b"NXP2P_PONG" {
                        if let Ok(mut lock) = p2p_addr_clone.write() {
                            *lock = Some(from_addr);
                        }
                        p2p_active_clone.store(true, Ordering::Relaxed);
                        tracing::info!("⚡ [P2P UDP] NXP2P_PONG recibido de {}. Enlace activo.", from_addr);
                    } else if len >= 13 && &buf[..5] == b"NXVID" {
                        let seq = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]);
                        let chunk_idx = u16::from_be_bytes([buf[9], buf[10]]) as usize;
                        let total_chunks = u16::from_be_bytes([buf[11], buf[12]]) as usize;
                        if let Some(full_frame) = assembler.add_chunk(seq, chunk_idx, total_chunks, &buf[13..len]) {
                            let _ = video_tx_udp.send(Some(full_frame));
                        }
                    }
                }
            });
        }

        let client = Self {
            control_tx,
            video_tx,
            incoming_video_rx,
            room_id,
            last_socket_send_us,
            p2p_socket: udp_sock,
            p2p_remote_addr,
            p2p_public_addr,
            is_p2p_active,
            local_ip,
        };

        Ok((client, event_rx))
    }

    /// Indica si el canal P2P UDP directo está actualmente activo
    pub fn is_p2p_active(&self) -> bool {
        self.is_p2p_active.load(Ordering::Relaxed)
    }

    /// Retorna la dirección IP:puerto remota del par P2P si está conectada
    pub fn p2p_remote_addr(&self) -> Option<std::net::SocketAddr> {
        self.p2p_remote_addr.read().ok().and_then(|g| *g)
    }

    /// Retorna el puerto UDP local asignado para la conexión P2P
    pub fn p2p_local_port(&self) -> Option<u16> {
        self.p2p_socket.as_ref().and_then(|s| s.local_addr().ok().map(|a| a.port()))
    }

    /// Retorna los candidatos de endpoint UDP (Público STUN y Local LAN) para enlace WAN/LAN directo
    pub fn p2p_local_endpoint(&self) -> Option<String> {
        let mut candidates = Vec::new();
        if let Ok(guard) = self.p2p_public_addr.read() {
            if let Some(pub_addr) = *guard {
                candidates.push(pub_addr.to_string());
            }
        }
        if let Some(ref sock) = self.p2p_socket {
            if let Ok(local_udp) = sock.local_addr() {
                let ip_str = match self.local_ip {
                    Some(ip) if !ip.is_unspecified() => ip.to_string(),
                    _ => "127.0.0.1".to_string(),
                };
                let local_ep = format!("{}:{}", ip_str, local_udp.port());
                if !candidates.contains(&local_ep) {
                    candidates.push(local_ep);
                }
            }
        }
        if candidates.is_empty() {
            None
        } else {
            Some(candidates.join("|"))
        }
    }

    /// Envía paquetes UDP punch para abrir NAT y establecer enlace P2P directo (soporta múltiples candidatos)
    pub async fn punch_p2p(&self, remote_endpoints_str: &str) {
        if let Some(ref sock) = self.p2p_socket {
            for ep in remote_endpoints_str.split('|') {
                let clean = ep.trim();
                if let Ok(addr) = clean.parse::<std::net::SocketAddr>() {
                    if let Ok(mut lock) = self.p2p_remote_addr.write() {
                        *lock = Some(addr);
                    }
                    for _ in 0..4 {
                        let _ = sock.send_to(b"NXP2P_PING", addr).await;
                        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                    }
                }
            }
        }
    }

    /// Retorna el tiempo en microsegundos del último envío de fotograma por socket TCP
    pub fn last_socket_send_us(&self) -> u32 {
        self.last_socket_send_us.load(std::sync::atomic::Ordering::Relaxed)
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

    /// Envía un evento de ratón o teclado exclusivamente por el canal WebSocket TCP (cero HoL blocking)
    pub fn send_input_direct(
        &self,
        event: &InputEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sig_msg = SignalingMessage::new_input(event)?;
        let json = serde_json::to_string(&sig_msg)?;
        let formatted = format!("{}\u{1e}", json);
        self.control_tx
            .send(Message::Text(formatted))
            .map_err(|e| format!("Error enviando input por TCP: {}", e))?;
        Ok(())
    }

    /// Envía un fotograma de video: vía UDP si el enlace P2P directo está activo, con fallback a WebSocket TCP
    pub fn send_video_frame_direct(&self, frame_bytes: Vec<u8>) {
        if self.is_p2p_active() {
            if let (Some(ref sock), Some(addr)) = (&self.p2p_socket, self.p2p_remote_addr()) {
                let seq = UDP_FRAME_SEQ.fetch_add(1, Ordering::Relaxed);
                let chunk_size = 1200;
                let total_chunks = (frame_bytes.len() + chunk_size - 1) / chunk_size;
                if total_chunks > 0 && total_chunks <= 1024 {
                    for (idx, chunk) in frame_bytes.chunks(chunk_size).enumerate() {
                        let mut packet = Vec::with_capacity(13 + chunk.len());
                        packet.extend_from_slice(b"NXVID");
                        packet.extend_from_slice(&seq.to_be_bytes());
                        packet.extend_from_slice(&(idx as u16).to_be_bytes());
                        packet.extend_from_slice(&(total_chunks as u16).to_be_bytes());
                        packet.extend_from_slice(chunk);
                        let _ = sock.try_send_to(&packet, addr);
                    }
                    return;
                }
            }
        }

        // Fallback al canal WebSocket TCP si P2P UDP no está disponible
        let _ = self.video_tx.send(Some(frame_bytes));
    }

    /// Envía un fotograma de video con descarte automático de fotogramas atrasados (Zero Bufferbloat)
    pub async fn send_video_frame(
        &self,
        frame_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_video_frame_direct(frame_bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udp_assembler_ordered() {
        let mut assembler = UdpFrameAssembler::new();
        let chunk0 = b"HELLO_";
        let chunk1 = b"WORLD!";

        assert!(assembler.add_chunk(1, 0, 2, chunk0).is_none());
        let full = assembler.add_chunk(1, 1, 2, chunk1);
        assert!(full.is_some());
        assert_eq!(full.unwrap(), b"HELLO_WORLD!");
    }

    #[test]
    fn test_udp_assembler_out_of_order() {
        let mut assembler = UdpFrameAssembler::new();
        let chunk0 = b"PART_A_";
        let chunk1 = b"PART_B_";
        let chunk2 = b"PART_C";

        assert!(assembler.add_chunk(42, 2, 3, chunk2).is_none());
        assert!(assembler.add_chunk(42, 0, 3, chunk0).is_none());
        let full = assembler.add_chunk(42, 1, 3, chunk1);
        assert!(full.is_some());
        assert_eq!(full.unwrap(), b"PART_A_PART_B_PART_C");
    }

    #[test]
    fn test_udp_assembler_drops_stale_on_newer_seq() {
        let mut assembler = UdpFrameAssembler::new();
        // Recibe chunk 0 de seq 10
        assert!(assembler.add_chunk(10, 0, 3, b"OLD_0").is_none());

        // Llega seq 11 antes de que seq 10 termine -> debe reiniciar para seq 11 sin bufferbloat
        assert!(assembler.add_chunk(11, 0, 2, b"NEW_0").is_none());
        // Llega chunk 1 de la vieja seq 10 -> debe ser ignorado
        assert!(assembler.add_chunk(10, 1, 3, b"OLD_1").is_none());

        // Completa seq 11
        let full = assembler.add_chunk(11, 1, 2, b"NEW_1");
        assert!(full.is_some());
        assert_eq!(full.unwrap(), b"NEW_0NEW_1");
    }
}
