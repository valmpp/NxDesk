use nxdesk_core::{PeerIdentity, PeerStatus};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

pub const DISCOVERY_PORT: u16 = 5001;

/// Servicio de descubrimiento LAN mediante UDP Broadcast
pub struct DiscoveryService {
    my_id: String,
    my_alias: String,
    my_port: u16,
    my_status: Arc<RwLock<PeerStatus>>,
    is_running: Arc<AtomicBool>,
}

impl DiscoveryService {
    pub fn new(my_id: impl Into<String>, my_alias: impl Into<String>, my_port: u16) -> Self {
        Self {
            my_id: my_id.into(),
            my_alias: my_alias.into(),
            my_port,
            my_status: Arc::new(RwLock::new(PeerStatus::Background)),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Actualiza el estado actual (ej. Background -> Connected)
    pub async fn set_status(&self, status: PeerStatus) {
        let mut lock = self.my_status.write().await;
        *lock = status;
    }

    /// Detiene las tareas de escucha y difusión
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    /// Inicia el listener y el broadcaster en segundo plano.
    /// Retorna un canal `Receiver<PeerIdentity>` por donde fluyen los equipos descubiertos.
    pub async fn start(&self) -> Result<mpsc::Receiver<PeerIdentity>, std::io::Error> {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Discovery is already running",
            ));
        }

        let (tx, rx) = mpsc::channel::<PeerIdentity>(100);

        // 1. Configurar Listener UDP con Socket2 para SO_REUSEADDR
        let listener_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        #[cfg(not(windows))]
        listener_socket.set_reuse_port(true).ok();
        listener_socket.set_reuse_address(true)?;
        listener_socket.set_broadcast(true)?;
        listener_socket.set_nonblocking(true)?;

        let listen_addr: SocketAddr = format!("0.0.0.0:{}", DISCOVERY_PORT).parse().unwrap();
        listener_socket.bind(&listen_addr.into())?;

        let std_listener: std::net::UdpSocket = listener_socket.into();
        let tokio_listener = tokio::net::UdpSocket::from_std(std_listener)?;

        // 2. Configurar Broadcaster UDP
        let broadcaster_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        broadcaster_socket.set_reuse_address(true)?;
        broadcaster_socket.set_broadcast(true)?;
        broadcaster_socket.set_nonblocking(true)?;

        let bind_zero: SocketAddr = "0.0.0.0:0".parse().unwrap();
        broadcaster_socket.bind(&bind_zero.into())?;

        let std_broadcaster: std::net::UdpSocket = broadcaster_socket.into();
        let tokio_broadcaster = tokio::net::UdpSocket::from_std(std_broadcaster)?;

        let is_running_listener = Arc::clone(&self.is_running);
        let my_id_clone = self.my_id.clone();

        // 3. Tarea asíncrona de ESCUCHA (Listener)
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            while is_running_listener.load(Ordering::Relaxed) {
                match tokio_listener.recv_from(&mut buf).await {
                    Ok((len, peer_addr)) => {
                        if let Ok(raw_str) = std::str::from_utf8(&buf[..len]) {
                            if let Ok(mut peer) = serde_json::from_str::<PeerIdentity>(raw_str) {
                                // Ignorar nuestras propias transmisiones
                                if peer.id != my_id_clone {
                                    peer.ip_address = peer_addr.ip().to_string();
                                    let _ = tx.send(peer).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[Discovery] Error al recibir UDP: {}", e);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        });

        // 4. Tarea asíncrona de DIFUSIÓN (Broadcaster)
        let is_running_broadcaster = Arc::clone(&self.is_running);
        let my_id = self.my_id.clone();
        let my_alias = self.my_alias.clone();
        let my_port = self.my_port;
        let my_status = Arc::clone(&self.my_status);

        tokio::spawn(async move {
            let broadcast_target: SocketAddr = format!("255.255.255.255:{}", DISCOVERY_PORT)
                .parse()
                .unwrap();

            // Pequeña espera inicial
            tokio::time::sleep(Duration::from_millis(500)).await;

            while is_running_broadcaster.load(Ordering::Relaxed) {
                let current_status = *my_status.read().await;
                let payload = PeerIdentity {
                    id: my_id.clone(),
                    alias: my_alias.clone(),
                    ip_address: String::new(),
                    port: my_port,
                    status: current_status,
                    hostname: None,
                    os: Some(std::env::consts::OS.to_string()),
                };

                if let Ok(json_str) = serde_json::to_string(&payload) {
                    let bytes = json_str.as_bytes();
                    let _ = tokio_broadcaster.send_to(bytes, broadcast_target).await;
                }

                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        Ok(rx)
    }
}
