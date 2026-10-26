use crate::auth::AuthManager;
use nxdesk_capture::ScreenCapturer;
use nxdesk_core::{AppConfig, PeerIdentity};
use nxdesk_file_transfer::FileTransferManager;
use nxdesk_input::InputInjector;
use nxdesk_signaling::SignalingClient;
use nxdesk_terminal::TerminalSession;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

/// Estado global del cliente NxDesk
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    #[allow(dead_code)]
    pub auth: AuthManager,
    pub input_injector: Arc<Mutex<InputInjector>>,
    pub capturer: Arc<Mutex<ScreenCapturer>>,
    pub file_manager: Arc<FileTransferManager>,
    pub terminal_session: Arc<TerminalSession>,
    pub discovered_peers: Arc<RwLock<HashMap<String, PeerIdentity>>>,
    pub is_signaling_connected: Arc<AtomicBool>,
    pub active_remote_peer: Arc<RwLock<Option<String>>>,
    pub latest_jpeg_frame: Arc<RwLock<Option<Vec<u8>>>>,
    pub frame_broadcast_tx: broadcast::Sender<Vec<u8>>,
    pub terminal_broadcast_tx: broadcast::Sender<String>,
    pub connected_viewers: Arc<AtomicUsize>,
    pub room_occupancy: Arc<AtomicUsize>,
    pub signaling_client: Arc<RwLock<Option<Arc<SignalingClient>>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let auth = AuthManager::new(config.access_password.clone());
        let (frame_tx, _) = broadcast::channel::<Vec<u8>>(32);
        let (term_tx, _) = broadcast::channel::<String>(128);

        let capturer = ScreenCapturer::new();
        let (w, h) = capturer.dimensions();
        let mut injector = InputInjector::new();
        injector.set_screen_bounds(0, 0, w as i32, h as i32);

        Self {
            config: Arc::new(RwLock::new(config)),
            auth,
            input_injector: Arc::new(Mutex::new(injector)),
            capturer: Arc::new(Mutex::new(capturer)),
            file_manager: Arc::new(FileTransferManager::new()),
            terminal_session: Arc::new(TerminalSession::new()),
            discovered_peers: Arc::new(RwLock::new(HashMap::new())),
            is_signaling_connected: Arc::new(AtomicBool::new(false)),
            active_remote_peer: Arc::new(RwLock::new(None)),
            latest_jpeg_frame: Arc::new(RwLock::new(None)),
            frame_broadcast_tx: frame_tx,
            terminal_broadcast_tx: term_tx,
            connected_viewers: Arc::new(AtomicUsize::new(0)),
            room_occupancy: Arc::new(AtomicUsize::new(0)),
            signaling_client: Arc::new(RwLock::new(None)),
        }
    }

    pub fn increment_connected_viewers(&self) -> usize {
        self.connected_viewers.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn decrement_connected_viewers(&self) -> usize {
        self.connected_viewers.fetch_sub(1, Ordering::SeqCst).saturating_sub(1)
    }

    pub fn get_connected_viewers(&self) -> usize {
        self.connected_viewers.load(Ordering::SeqCst)
    }

    pub fn set_room_occupancy(&self, count: usize) {
        self.room_occupancy.store(count, Ordering::SeqCst);
    }

    pub fn get_room_occupancy(&self) -> usize {
        self.room_occupancy.load(Ordering::SeqCst)
    }

    pub fn set_signaling_connected(&self, connected: bool) {
        self.is_signaling_connected.store(connected, Ordering::SeqCst);
    }

    pub fn is_signaling_connected(&self) -> bool {
        self.is_signaling_connected.load(Ordering::SeqCst)
    }

    pub async fn add_or_update_peer(&self, peer: PeerIdentity) {
        let mut peers = self.discovered_peers.write().await;
        peers.insert(peer.id.clone(), peer);
    }

    pub async fn get_discovered_peers(&self) -> Vec<PeerIdentity> {
        let peers = self.discovered_peers.read().await;
        peers.values().cloned().collect()
    }

    /// Retorna la lista de pantallas disponibles (incluyendo monitores secundarios y mosaico)
    pub async fn get_screens(&self) -> Vec<nxdesk_core::ScreenInfoDto> {
        let capturer = self.capturer.lock().await;
        capturer.get_screens()
    }

    /// Cambia la pantalla capturada e sincroniza las coordenadas del inyector de ratón
    pub async fn select_screen(&self, index: i32) -> Result<nxdesk_core::ScreenInfoDto, String> {
        let mut capturer = self.capturer.lock().await;
        let screen = capturer.select_screen(index)?;
        let mut injector = self.input_injector.lock().await;
        injector.set_screen_bounds(screen.x, screen.y, screen.width, screen.height);
        Ok(screen)
    }

    /// Verifica si la terminal remota está autorizada por política de seguridad VUL-01
    pub async fn is_remote_terminal_allowed(&self) -> bool {
        self.config.read().await.allow_remote_terminal
    }

    /// Habilita o deshabilita la terminal remota en caliente
    pub async fn set_remote_terminal_allowed(&self, allowed: bool) {
        let mut cfg = self.config.write().await;
        cfg.allow_remote_terminal = allowed;
    }

    /// Retorna el ID del par remoto actualmente activo en sesión
    pub async fn get_active_remote_peer(&self) -> Option<String> {
        self.active_remote_peer.read().await.clone()
    }

    /// Establece el par remoto activo
    pub async fn set_active_remote_peer(&self, peer: Option<String>) {
        let mut p = self.active_remote_peer.write().await;
        *p = peer;
    }

    /// Retorna una referencia al cliente de señalización activo si existe
    pub async fn get_signaling_client(&self) -> Option<Arc<SignalingClient>> {
        self.signaling_client.read().await.clone()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_initialization() {
        let cfg = AppConfig::default();
        let state = AppState::new(cfg);

        assert!(!state.is_signaling_connected());
        state.set_signaling_connected(true);
        assert!(state.is_signaling_connected());

        let peer = PeerIdentity {
            id: "peer-1".to_string(),
            alias: "Node-1".to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 5000,
            status: nxdesk_core::PeerStatus::Connected,
            hostname: None,
            os: None,
        };
        state.add_or_update_peer(peer).await;
        let peers = state.get_discovered_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].id, "peer-1");

        let screens = state.get_screens().await;
        assert!(!screens.is_empty());
        let res = state.select_screen(screens[0].index).await;
        assert!(res.is_ok());
    }
}


