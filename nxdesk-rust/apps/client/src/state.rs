use crate::auth::AuthManager;
use nxdesk_capture::ScreenCapturer;
use nxdesk_core::{
    ActiveSessionInfo, AppConfig, PeerIdentity, SessionDecisionDto, SessionRequestDto,
    SessionResponseDto, SessionRole,
};
use nxdesk_file_transfer::FileTransferManager;
use nxdesk_input::InputInjector;
use nxdesk_signaling::SignalingClient;
use nxdesk_terminal::TerminalSession;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, Mutex, RwLock};

use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Instantánea de métricas de telemetría de rendimiento en tiempo real
#[derive(Clone, Debug, Default)]
pub struct TelemetrySnapshot {
    pub rtt_ms: u32,
    pub rx_fps: u32,
    pub tx_fps: u32,
    pub rx_kbps: u32,
    pub tx_kbps: u32,
    pub remote_resolution: (u32, u32),
    pub local_resolution: (u32, u32),
}

/// Limitador de tasa en memoria por IP para mitigar fuerza bruta y saturación
#[derive(Clone, Default)]
pub struct InMemoryRateLimiter {
    attempts: Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>,
}

impl InMemoryRateLimiter {
    pub async fn check(&self, ip: IpAddr, max_attempts: u32, window: Duration) -> bool {
        let mut map = self.attempts.lock().await;
        let now = Instant::now();
        if map.len() > 1000 {
            map.retain(|_, (_, start)| now.duration_since(*start) < window);
        }
        let entry = map.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) > window {
            *entry = (1, now);
            true
        } else if entry.0 < max_attempts {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}

/// Estado global del cliente NxDesk
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub auth: AuthManager,
    pub rate_limiter: Arc<InMemoryRateLimiter>,
    pub input_injector: Arc<Mutex<InputInjector>>,
    pub capturer: Arc<Mutex<ScreenCapturer>>,
    pub file_manager: Arc<FileTransferManager>,
    pub terminal_session: Arc<TerminalSession>,
    pub discovered_peers: Arc<RwLock<HashMap<String, PeerIdentity>>>,
    pub is_signaling_connected: Arc<AtomicBool>,
    pub is_screen_sharing: Arc<AtomicBool>,
    pub is_host_active: Arc<AtomicBool>,
    pub is_viewer_active: Arc<AtomicBool>,
    pub active_remote_peer: Arc<RwLock<Option<String>>>,
    pub latest_jpeg_frame: Arc<RwLock<Option<Vec<u8>>>>,
    pub latest_decoded_image: Arc<RwLock<Option<Arc<eframe::egui::ColorImage>>>>,
    pub egui_ctx: Arc<RwLock<Option<eframe::egui::Context>>>,
    pub frame_sequence: Arc<AtomicU64>,
    pub frame_broadcast_tx: broadcast::Sender<Vec<u8>>,
    pub terminal_broadcast_tx: broadcast::Sender<String>,
    pub connected_viewers: Arc<AtomicUsize>,
    pub room_occupancy: Arc<AtomicUsize>,
    pub signaling_client: Arc<RwLock<Option<Arc<SignalingClient>>>>,
    // Protocolo AnyDesk: Petición / Aceptación de Sesión en Caliente
    pub pending_session: Arc<RwLock<Option<SessionRequestDto>>>,
    pub active_session: Arc<RwLock<Option<ActiveSessionInfo>>>,
    pub session_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<SessionResponseDto>>>>,

    // Telemetría de rendimiento en vivo (HUD FPS, Latencia RTT ms, Bitrate)
    pub rtt_ms: Arc<AtomicU32>,
    pub rx_frame_counter: Arc<AtomicU32>,
    pub rx_bytes_counter: Arc<AtomicU64>,
    pub rx_fps: Arc<AtomicU32>,
    pub rx_kbps: Arc<AtomicU32>,

    pub tx_frame_counter: Arc<AtomicU32>,
    pub tx_bytes_counter: Arc<AtomicU64>,
    pub tx_fps: Arc<AtomicU32>,
    pub tx_kbps: Arc<AtomicU32>,

    pub remote_width: Arc<AtomicU32>,
    pub remote_height: Arc<AtomicU32>,
    pub local_width: Arc<AtomicU32>,
    pub local_height: Arc<AtomicU32>,

    // Multimonitor remoto y local
    pub remote_screens: Arc<RwLock<Vec<nxdesk_core::ScreenInfoDto>>>,
    pub active_screen_name: Arc<RwLock<String>>,
    pub active_screen_index: Arc<std::sync::atomic::AtomicI32>,

    // Acceso atómico ultrarrápido a permisos de control de entrada (Zero-Lock ~0 ns)
    pub allow_remote_input: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let auth = AuthManager::new(config.access_password.clone());
        let (frame_tx, _) = broadcast::channel::<Vec<u8>>(8);
        let (term_tx, _) = broadcast::channel::<String>(128);

        let capturer = ScreenCapturer::new();
        let (w, h) = capturer.dimensions();
        let active_index = capturer.active_index;
        let mut injector = InputInjector::new();
        injector.set_screen_bounds(capturer.origin_x, capturer.origin_y, w as i32, h as i32);

        let auto_share = config.auto_share;

        Self {
            config: Arc::new(RwLock::new(config)),
            auth,
            rate_limiter: Arc::new(InMemoryRateLimiter::default()),
            input_injector: Arc::new(Mutex::new(injector)),
            capturer: Arc::new(Mutex::new(capturer)),
            file_manager: Arc::new(FileTransferManager::new()),
            terminal_session: Arc::new(TerminalSession::new()),
            discovered_peers: Arc::new(RwLock::new(HashMap::new())),
            is_signaling_connected: Arc::new(AtomicBool::new(false)),
            is_screen_sharing: Arc::new(AtomicBool::new(auto_share)),
            is_host_active: Arc::new(AtomicBool::new(false)),
            is_viewer_active: Arc::new(AtomicBool::new(false)),
            active_remote_peer: Arc::new(RwLock::new(None)),
            latest_jpeg_frame: Arc::new(RwLock::new(None)),
            latest_decoded_image: Arc::new(RwLock::new(None)),
            egui_ctx: Arc::new(RwLock::new(None)),
            frame_sequence: Arc::new(AtomicU64::new(0)),
            frame_broadcast_tx: frame_tx,
            terminal_broadcast_tx: term_tx,
            connected_viewers: Arc::new(AtomicUsize::new(0)),
            room_occupancy: Arc::new(AtomicUsize::new(0)),
            signaling_client: Arc::new(RwLock::new(None)),
            pending_session: Arc::new(RwLock::new(None)),
            active_session: Arc::new(RwLock::new(None)),
            session_waiters: Arc::new(Mutex::new(HashMap::new())),

            rtt_ms: Arc::new(AtomicU32::new(0)),
            rx_frame_counter: Arc::new(AtomicU32::new(0)),
            rx_bytes_counter: Arc::new(AtomicU64::new(0)),
            rx_fps: Arc::new(AtomicU32::new(0)),
            rx_kbps: Arc::new(AtomicU32::new(0)),

            tx_frame_counter: Arc::new(AtomicU32::new(0)),
            tx_bytes_counter: Arc::new(AtomicU64::new(0)),
            tx_fps: Arc::new(AtomicU32::new(0)),
            tx_kbps: Arc::new(AtomicU32::new(0)),

            remote_width: Arc::new(AtomicU32::new(0)),
            remote_height: Arc::new(AtomicU32::new(0)),
            local_width: Arc::new(AtomicU32::new(w)),
            local_height: Arc::new(AtomicU32::new(h)),

            remote_screens: Arc::new(RwLock::new(Vec::new())),
            active_screen_name: Arc::new(RwLock::new("Pantalla 1".to_string())),
            active_screen_index: Arc::new(std::sync::atomic::AtomicI32::new(active_index)),
            allow_remote_input: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn set_remote_screens(&self, screens: Vec<nxdesk_core::ScreenInfoDto>) {
        let mut lock = self.remote_screens.write().await;
        *lock = screens;
    }

    pub async fn set_active_screen_info(&self, index: i32, name: String) {
        self.active_screen_index.store(index, Ordering::Relaxed);
        let mut lock = self.active_screen_name.write().await;
        *lock = name;
    }

    pub fn update_rtt(&self, ms: u32) {
        self.rtt_ms.store(ms, Ordering::Relaxed);
    }

    pub fn record_rx_frame(&self, bytes_len: usize, width: u32, height: u32) {
        self.rx_frame_counter.fetch_add(1, Ordering::Relaxed);
        self.rx_bytes_counter.fetch_add(bytes_len as u64, Ordering::Relaxed);
        if width > 0 && height > 0 {
            self.remote_width.store(width, Ordering::Relaxed);
            self.remote_height.store(height, Ordering::Relaxed);
        }
    }

    pub fn record_tx_frame(&self, bytes_len: usize, width: u32, height: u32) {
        self.tx_frame_counter.fetch_add(1, Ordering::Relaxed);
        self.tx_bytes_counter.fetch_add(bytes_len as u64, Ordering::Relaxed);
        if width > 0 && height > 0 {
            self.local_width.store(width, Ordering::Relaxed);
            self.local_height.store(height, Ordering::Relaxed);
        }
    }

    pub fn tick_telemetry_second(&self) {
        let rx_frames = self.rx_frame_counter.swap(0, Ordering::Relaxed);
        let rx_bytes = self.rx_bytes_counter.swap(0, Ordering::Relaxed);
        self.rx_fps.store(rx_frames, Ordering::Relaxed);
        self.rx_kbps.store(((rx_bytes * 8) / 1000) as u32, Ordering::Relaxed);

        let tx_frames = self.tx_frame_counter.swap(0, Ordering::Relaxed);
        let tx_bytes = self.tx_bytes_counter.swap(0, Ordering::Relaxed);
        self.tx_fps.store(tx_frames, Ordering::Relaxed);
        self.tx_kbps.store(((tx_bytes * 8) / 1000) as u32, Ordering::Relaxed);
    }

    pub fn get_telemetry_snapshot(&self) -> TelemetrySnapshot {
        TelemetrySnapshot {
            rtt_ms: self.rtt_ms.load(Ordering::Relaxed),
            rx_fps: self.rx_fps.load(Ordering::Relaxed),
            tx_fps: self.tx_fps.load(Ordering::Relaxed),
            rx_kbps: self.rx_kbps.load(Ordering::Relaxed),
            tx_kbps: self.tx_kbps.load(Ordering::Relaxed),
            remote_resolution: (
                self.remote_width.load(Ordering::Relaxed),
                self.remote_height.load(Ordering::Relaxed),
            ),
            local_resolution: (
                self.local_width.load(Ordering::Relaxed),
                self.local_height.load(Ordering::Relaxed),
            ),
        }
    }

    pub fn increment_frame_sequence(&self) -> u64 {
        self.frame_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_screen_sharing(&self) -> bool {
        self.is_screen_sharing.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub fn set_screen_sharing(&self, sharing: bool) {
        self.is_screen_sharing.store(sharing, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn toggle_screen_sharing(&self) -> bool {
        let prev = self.is_screen_sharing.fetch_xor(true, Ordering::SeqCst);
        !prev
    }

    pub fn increment_connected_viewers(&self) -> usize {
        self.connected_viewers.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn decrement_connected_viewers(&self) -> usize {
        self.connected_viewers
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1)
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
        self.is_signaling_connected
            .store(connected, Ordering::SeqCst);
    }

    pub fn is_signaling_connected(&self) -> bool {
        self.is_signaling_connected.load(Ordering::SeqCst)
    }

    pub async fn add_or_update_peer(&self, peer: PeerIdentity) {
        let my_id = self.config.read().await.device_id.clone();
        if peer.id != my_id {
            let mut peers = self.discovered_peers.write().await;
            peers.insert(peer.id.clone(), peer);
        }
    }

    pub async fn remove_peer(&self, peer_id: &str) {
        let mut peers = self.discovered_peers.write().await;
        peers.remove(peer_id);
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

    /// Cambia la pantalla capturada, sincroniza las coordenadas del inyector de ratón e invalida caché
    pub async fn select_screen(&self, index: i32) -> Result<nxdesk_core::ScreenInfoDto, String> {
        let mut capturer = self.capturer.lock().await;
        let screen = capturer.select_screen(index)?;
        let mut injector = self.input_injector.lock().await;
        injector.set_screen_bounds(
            screen.x,
            screen.y,
            screen.width as i32,
            screen.height as i32,
        );

        // Limpiar inmediatamente el fotograma antiguo en memoria para evitar ver el monitor anterior
        {
            let mut latest = self.latest_jpeg_frame.write().await;
            *latest = None;
            let mut img_lock = self.latest_decoded_image.write().await;
            *img_lock = None;
        }

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
        self.is_viewer_active.store(peer.is_some(), Ordering::Relaxed);
        let mut p = self.active_remote_peer.write().await;
        *p = peer;
    }

    pub fn is_host_active(&self) -> bool {
        self.is_host_active.load(Ordering::Relaxed)
    }

    pub fn is_viewer_active(&self) -> bool {
        self.is_viewer_active.load(Ordering::Relaxed)
    }

    pub fn try_get_signaling_client(&self) -> Option<Arc<SignalingClient>> {
        self.signaling_client.try_read().ok().and_then(|g| g.clone())
    }

    pub fn set_egui_ctx(&self, ctx: eframe::egui::Context) {
        if let Ok(mut lock) = self.egui_ctx.try_write() {
            *lock = Some(ctx);
        }
    }

    pub fn request_ui_repaint(&self) {
        if let Ok(guard) = self.egui_ctx.try_read() {
            if let Some(ref ctx) = *guard {
                ctx.request_repaint();
            }
        }
    }

    /// Retorna una referencia al cliente de señalización activo si existe
    pub async fn get_signaling_client(&self) -> Option<Arc<SignalingClient>> {
        self.signaling_client.read().await.clone()
    }

    /// Registra una solicitud de sesión AnyDesk entrante y retorna un canal oneshot para esperar la decisión del anfitrión
    pub async fn request_session(
        &self,
        req: SessionRequestDto,
    ) -> oneshot::Receiver<SessionResponseDto> {
        let (tx, rx) = oneshot::channel();
        {
            let mut waiters = self.session_waiters.lock().await;
            waiters.insert(req.request_id.clone(), tx);
        }
        {
            let mut pending = self.pending_session.write().await;
            *pending = Some(req);
        }
        rx
    }

    /// Retorna la solicitud de conexión entrante pendiente de aprobación
    pub async fn get_pending_session(&self) -> Option<SessionRequestDto> {
        let pending = self.pending_session.read().await;
        pending.clone()
    }

    /// Retorna los detalles de la sesión remota actualmente activa
    pub async fn get_active_session(&self) -> Option<ActiveSessionInfo> {
        let active = self.active_session.read().await;
        active.clone()
    }

    /// El anfitrión toma una decisión (Aceptar / Rechazar) sobre la solicitud de conexión entrante
    pub async fn decide_session(
        &self,
        decision: SessionDecisionDto,
    ) -> Result<SessionResponseDto, String> {
        let mut pending = self.pending_session.write().await;
        let req = pending
            .take()
            .ok_or_else(|| "No hay solicitudes de sesión pendientes.".to_string())?;

        if req.request_id != decision.request_id {
            *pending = Some(req);
            return Err("El ID de solicitud no coincide con la solicitud pendiente.".to_string());
        }

        let response = if decision.accept {
            let token = crate::auth::generate_secure_token();
            let granted_role = decision.granted_role.unwrap_or(req.requested_role);
            self.auth.grant_token(token.clone(), granted_role).await;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let session_info = ActiveSessionInfo {
                session_id: format!("sess_{}", req.request_id),
                peer_id: req.client_id.clone(),
                peer_alias: req.client_alias.clone(),
                granted_role,
                allow_input: decision.allow_input,
                allow_clipboard: decision.allow_clipboard,
                allow_file_transfer: decision.allow_file_transfer,
                start_time: now,
            };

            {
                let mut active = self.active_session.write().await;
                *active = Some(session_info);
            }
            self.is_host_active.store(true, Ordering::Relaxed);
            self.allow_remote_input.store(decision.allow_input, Ordering::Relaxed);
            // En el anfitrión no asignamos active_remote_peer para que permanezca en su pantalla local
            // con el widget flotante de desconexión, y solo el cliente remoto abra el visor (RemoteView).

            SessionResponseDto {
                success: true,
                request_id: req.request_id.clone(),
                session_token: Some(token),
                granted_role,
                allow_input: decision.allow_input,
                allow_clipboard: decision.allow_clipboard,
                allow_file_transfer: decision.allow_file_transfer,
                error_message: None,
            }
        } else {
            self.is_host_active.store(false, Ordering::Relaxed);
            self.allow_remote_input.store(false, Ordering::Relaxed);
            SessionResponseDto {
                success: false,
                request_id: req.request_id.clone(),
                session_token: None,
                granted_role: SessionRole::ViewOnly,
                allow_input: false,
                allow_clipboard: false,
                allow_file_transfer: false,
                error_message: Some(
                    decision
                        .rejection_reason
                        .unwrap_or_else(|| "Conexión rechazada por el anfitrión.".to_string()),
                ),
            }
        };

        // Despertar hilo que espera la respuesta del handshake
        let mut waiters = self.session_waiters.lock().await;
        if let Some(tx) = waiters.remove(&req.request_id) {
            let _ = tx.send(response.clone());
        }

        Ok(response)
    }

    /// Finaliza la sesión remota activa actual (HostDisconnectWindow)
    pub async fn terminate_session(&self) -> bool {
        self.is_host_active.store(false, Ordering::Relaxed);
        self.is_viewer_active.store(false, Ordering::Relaxed);
        self.allow_remote_input.store(false, Ordering::Relaxed);
        let mut active = self.active_session.write().await;
        if active.take().is_some() {
            let mut remote_peer = self.active_remote_peer.write().await;
            *remote_peer = None;
            true
        } else {
            false
        }
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

    #[tokio::test]
    async fn test_anydesk_session_handshake_flow() {
        let cfg = AppConfig::default();
        let state = AppState::new(cfg);

        // 1. Cliente envía solicitud
        let req = SessionRequestDto {
            request_id: "req-123".to_string(),
            client_id: "client-456".to_string(),
            client_alias: "Soporte-TI".to_string(),
            requested_role: SessionRole::FullControl,
            allow_input: true,
            allow_clipboard: true,
            allow_file_transfer: false,
            timestamp: 1000,
        };

        let mut waiter_rx = state.request_session(req).await;

        // 2. Verificar que el anfitrión ve la solicitud pendiente
        let pending = state.get_pending_session().await.expect("Pending expected");
        assert_eq!(pending.request_id, "req-123");
        assert_eq!(pending.client_alias, "Soporte-TI");

        // 3. Anfitrión acepta la solicitud
        let decision = SessionDecisionDto {
            request_id: "req-123".to_string(),
            accept: true,
            granted_role: Some(SessionRole::FullControl),
            allow_input: true,
            allow_clipboard: true,
            allow_file_transfer: false,
            rejection_reason: None,
        };

        let decide_res = state.decide_session(decision).await.expect("Decide failed");
        assert!(decide_res.success);
        let token = decide_res.session_token.expect("Token expected");

        // 4. Verificar que el canal oneshot del cliente recibió la aceptación
        let client_received = waiter_rx.try_recv().expect("Channel should resolve");
        assert!(client_received.success);
        assert_eq!(client_received.session_token.unwrap(), token);

        // 5. Verificar sesión activa
        let active = state.get_active_session().await.expect("Active expected");
        assert_eq!(active.peer_alias, "Soporte-TI");
        assert!(active.allow_input);

        // 6. Terminar sesión
        assert!(state.terminate_session().await);
        assert!(state.get_active_session().await.is_none());
    }
}
