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
    pub cap_ms: f32,
    pub enc_ms: f32,
    pub dec_ms: f32,
    pub net_ms: f32,
    pub packet_loss: u32,
    pub ram_mb: f32,
}

/// Información de consumo de memoria RAM del proceso
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessMemoryInfo {
    /// Memoria física real en uso por el proceso (Working Set)
    pub working_set_mb: f32,
    /// Memoria privada asignada al proceso (Commit charge)
    pub private_usage_mb: f32,
    /// Pico máximo histórico de memoria física (Peak Working Set)
    pub peak_working_set_mb: f32,
}

/// Consulta el consumo de memoria RAM del proceso en curso mediante la API del sistema operativo
pub fn query_process_memory() -> ProcessMemoryInfo {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut pmc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc as *mut _ as *mut _, cb) != 0 {
            ProcessMemoryInfo {
                working_set_mb: pmc.WorkingSetSize as f32 / (1024.0 * 1024.0),
                private_usage_mb: pmc.PrivateUsage as f32 / (1024.0 * 1024.0),
                peak_working_set_mb: pmc.PeakWorkingSetSize as f32 / (1024.0 * 1024.0),
            }
        } else {
            ProcessMemoryInfo::default()
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        ProcessMemoryInfo::default()
    }
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

/// Compara IDs de equipo ignorando guiones, espacios y mayúsculas (482-195-302 == 482195302)
fn same_peer_id(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (na, nb) = (norm(a), norm(b));
    !na.is_empty() && na == nb
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
    /// Cola FIFO de comandos de terminal entrantes (Some tras el primer comando; el worker la consume).
    pub terminal_cmd_tx: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
    /// Canal exclusivo para la salida de la terminal local destinada a ser enviada por la red.
    /// Aísla la retransmisión de red del broadcast de UI para impedir loops infinitos de eco.
    pub terminal_out_network_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub terminal_out_network_rx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<String>>>>,
    /// Límite acumulado de bytes por comando para evitar torrentes de datos infinitos.
    pub terminal_stream_bytes: Arc<std::sync::atomic::AtomicUsize>,
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
    pub last_input_ts_ms: Arc<AtomicU64>,

    // Época de sesión: se incrementa cada vez que una sesión termina. Permite que los intentos de
    // conexión en vuelo (tareas async de `initiate_connect`) detecten que fueron cancelados.
    pub session_epoch: Arc<AtomicU64>,

    // Retransmisor de salida de terminal hacia la red ("terminal_out"): se activa una única vez
    // al procesar el primer comando autorizado; evita tareas duplicadas y eco en loop.
    pub terminal_forward_started: Arc<AtomicBool>,

    // Grabación de sesión: Some(recorder) mientras se graba (frames JPEG del visor → AVI MJPEG).
    // Mutex síncrono (std): escrituras a 30 FPS sin tocar el runtime async.
    pub session_recorder: Arc<std::sync::Mutex<Option<crate::recorder::SessionRecorder>>>,
    // Nota de resultado (path guardado o error) que la UI consume en el siguiente frame.
    pub recording_note: Arc<std::sync::Mutex<Option<String>>>,

    // Tiempos de latencia por etapa (microsegundos) para telemetría en tiempo real
    pub cap_time_us: Arc<AtomicU32>,
    pub enc_time_us: Arc<AtomicU32>,
    pub dec_time_us: Arc<AtomicU32>,
    pub net_time_us: Arc<AtomicU32>,
    pub packet_loss_counter: Arc<AtomicU32>,

    // ==== DIAGNÓSTICO DE LATENCIA (anillos de muestras por etapa, µs; ventana rodante) ====
    pub stage_cap: SampleRing,
    pub stage_enc: SampleRing,
    pub stage_send: SampleRing,
    pub stage_dec: SampleRing,
    pub stage_net: SampleRing,
    /// Subida del frame decodificado a la textura GPU (render de video)
    pub stage_render: SampleRing,
    /// Edad del frame al pintarse (decode → UI paint, ms) — lag del tramo del render
    pub stage_paint_age: SampleRing,
    /// Jitter de llegada (desviación del ritmo inter-frames, µs)
    pub stage_jitter: SampleRing,
    /// Ocupación del pipeline TX (captura+encode, ns por ventana de reporte)
    pub tx_pipeline_busy_ns: Arc<AtomicU64>,
    /// Ocupación del decoder (ns por ventana de reporte)
    pub decode_busy_ns: Arc<AtomicU64>,
    /// Pérdida por ventana de reporte (huecos en la secuencia de frames)
    pub packet_loss_window: Arc<AtomicU32>,
    /// Unix ms del último frame decodificado (para edad al pintar)
    pub last_decoded_unix_ms: Arc<AtomicU64>,
    /// Parámetros efectivos actuales del emisor (para el reporte)Publication
    pub current_quality: Arc<AtomicU32>,
    pub current_fps: Arc<AtomicU32>,
    pub current_max_dim: Arc<AtomicU32>,
    pub cap_flush_us: Arc<AtomicU32>,
    pub cap_blt_us: Arc<AtomicU32>,
    pub cap_scale_us: Arc<AtomicU32>,
    pub socket_send_us: Arc<AtomicU32>,
    pub host_dropped_frames: Arc<AtomicU32>,
    pub reencode_count: Arc<AtomicU32>,
    pub requested_max_dim: Arc<AtomicU32>,
}

/// Anillo de muestras (µs) de una etapa del pipeline de video: ventana rodante de 150
/// muestras. Devuelve media igual y máximo en ms con costo O(n) solo al reportar.
#[derive(Default, Clone)]
pub struct SampleRing {
    samples: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
}

impl SampleRing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, us: u32) {
        let mut g = match self.samples.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if g.len() >= 150 {
            g.remove(0);
        }
        g.push(us);
    }

    /// (media ms, máx ms, n muestras)
    pub fn snapshot_ms(&self) -> (f32, f32, usize) {
        let g = match self.samples.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if g.is_empty() {
            return (0.0, 0.0, 0);
        }
        let sum: u64 = g.iter().map(|&v| v as u64).sum();
        let max = *g.iter().max().unwrap_or(&0);
        (
            (sum as f32 / g.len() as f32) / 1000.0,
            max as f32 / 1000.0,
            g.len(),
        )
    }
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let auth = AuthManager::new(config.access_password.clone());
        let (frame_tx, _) = broadcast::channel::<Vec<u8>>(8);
        let (term_tx, _) = broadcast::channel::<String>(512);
        let (term_net_tx, term_net_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        // Cola FIFO serial para comandos de terminal recibidos por red: el dispatcher de eventos
        // los ENCOLA (nunca await inline) y un worker los ejecuta uno por uno, sin bloquear el
        // bucle de señalización mientras la terminal arranca o escribe al shell.
        let terminal_cmd_tx: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>> =
            Arc::new(std::sync::Mutex::new(None));

        let capturer = ScreenCapturer::new();
        let (w, h) = capturer.dimensions();
        let active_index = capturer.active_index;
        let mut injector = InputInjector::new();
        injector.set_screen_bounds(capturer.origin_x, capturer.origin_y, w as i32, h as i32);

        let auto_share = config.auto_share;
        let is_wan = config.signaling_server_url.starts_with("wss://")
            || config.signaling_server_url.contains("trycloudflare.com")
            || config.signaling_server_url.contains("ngrok");
        let initial_rtt = if is_wan { 85 } else { 0 };

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
            terminal_cmd_tx,
            terminal_out_network_tx: term_net_tx,
            terminal_out_network_rx: Arc::new(tokio::sync::Mutex::new(Some(term_net_rx))),
            terminal_stream_bytes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            connected_viewers: Arc::new(AtomicUsize::new(0)),
            room_occupancy: Arc::new(AtomicUsize::new(0)),
            signaling_client: Arc::new(RwLock::new(None)),
            pending_session: Arc::new(RwLock::new(None)),
            active_session: Arc::new(RwLock::new(None)),
            session_waiters: Arc::new(Mutex::new(HashMap::new())),

            rtt_ms: Arc::new(AtomicU32::new(initial_rtt)),
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
            last_input_ts_ms: Arc::new(AtomicU64::new(0)),
            session_epoch: Arc::new(AtomicU64::new(0)),
            terminal_forward_started: Arc::new(AtomicBool::new(false)),
            session_recorder: Arc::new(std::sync::Mutex::new(None)),
            recording_note: Arc::new(std::sync::Mutex::new(None)),
            cap_time_us: Arc::new(AtomicU32::new(0)),
            enc_time_us: Arc::new(AtomicU32::new(0)),
            dec_time_us: Arc::new(AtomicU32::new(0)),
            net_time_us: Arc::new(AtomicU32::new(0)),
            packet_loss_counter: Arc::new(AtomicU32::new(0)),
            stage_cap: SampleRing::new(),
            stage_enc: SampleRing::new(),
            stage_send: SampleRing::new(),
            stage_dec: SampleRing::new(),
            stage_net: SampleRing::new(),
            stage_render: SampleRing::new(),
            stage_paint_age: SampleRing::new(),
            stage_jitter: SampleRing::new(),
            tx_pipeline_busy_ns: Arc::new(AtomicU64::new(0)),
            decode_busy_ns: Arc::new(AtomicU64::new(0)),
            packet_loss_window: Arc::new(AtomicU32::new(0)),
            last_decoded_unix_ms: Arc::new(AtomicU64::new(0)),
            current_quality: Arc::new(AtomicU32::new(0)),
            current_fps: Arc::new(AtomicU32::new(0)),
            current_max_dim: Arc::new(AtomicU32::new(0)),
            cap_flush_us: Arc::new(AtomicU32::new(0)),
            cap_blt_us: Arc::new(AtomicU32::new(0)),
            cap_scale_us: Arc::new(AtomicU32::new(0)),
            socket_send_us: Arc::new(AtomicU32::new(0)),
            host_dropped_frames: Arc::new(AtomicU32::new(0)),
            reencode_count: Arc::new(AtomicU32::new(0)),
            requested_max_dim: Arc::new(AtomicU32::new(1366)),
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
        // EMA ligera (α≈0.5): un pico suelto de IO local del propio equipo (GC del UI, un
        // context switch) no estorba ni al HUD ni al control de congestión; la cola REAL
        // sostenida sigue penetrando el promedio en 2–3 segundos.
        let prev = self.rtt_ms.load(Ordering::Relaxed);
        let blended = if prev == 0 { ms } else { (prev / 2) + (ms / 2) };
        self.rtt_ms.store(blended, Ordering::Relaxed);
    }

    /// Registra interacción remota activa (ratón o teclado) para priorizar FPS en ráfaga (AnyDesk Mode)
    pub fn record_remote_input_activity(&self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_input_ts_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Comprueba si hubo interacción remota en los últimos `window_ms` milisegundos
    pub fn has_recent_input_activity(&self, window_ms: u64) -> bool {
        let last = self.last_input_ts_ms.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now_ms.saturating_sub(last) <= window_ms
    }

    /// Espejo de grabación de sesión: desactivado temporalmente para eliminar bloqueos de disco,
    /// contención de mutex y degradación de latencia en el pipeline de video.
    #[inline(always)]
    pub fn mirror_frame_to_recorder(&self, _frame_bytes: &[u8]) {
        // No-op para latencia ultrabaja (<120ms)
    }

    pub fn record_capture_subtimings(&self, flush_us: u32, blt_us: u32, scale_us: u32) {
        self.cap_flush_us.store(flush_us, Ordering::Relaxed);
        self.cap_blt_us.store(blt_us, Ordering::Relaxed);
        self.cap_scale_us.store(scale_us, Ordering::Relaxed);
    }

    pub fn record_socket_send_us(&self, us: u32) {
        self.socket_send_us.store(us, Ordering::Relaxed);
    }

    pub fn increment_host_dropped_frames(&self) {
        self.host_dropped_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_rx_frame(&self, bytes_len: usize, width: u32, height: u32) {        self.rx_frame_counter.fetch_add(1, Ordering::Relaxed);
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
            cap_ms: (self.cap_time_us.load(Ordering::Relaxed) as f32) / 1000.0,
            enc_ms: (self.enc_time_us.load(Ordering::Relaxed) as f32) / 1000.0,
            dec_ms: (self.dec_time_us.load(Ordering::Relaxed) as f32) / 1000.0,
            net_ms: (self.net_time_us.load(Ordering::Relaxed) as f32) / 1000.0,
            packet_loss: self.packet_loss_counter.load(Ordering::Relaxed),
            ram_mb: query_process_memory().working_set_mb,
        }
    }

    pub fn record_stage_timings(&self, cap_us: u32, enc_us: u32) {
        self.cap_time_us.store(cap_us, Ordering::Relaxed);
        self.enc_time_us.store(enc_us, Ordering::Relaxed);
        self.stage_cap.push(cap_us);
        self.stage_enc.push(enc_us);
        // Ocupación del pipeline TX (captura + codificación) para el reporte
        self.tx_pipeline_busy_ns
            .fetch_add((cap_us as u64 + enc_us as u64) * 1000, Ordering::Relaxed);
    }

    pub fn record_decode_timing(&self, dec_us: u32) {
        self.dec_time_us.store(dec_us, Ordering::Relaxed);
        self.stage_dec.push(dec_us);
        self.decode_busy_ns
            .fetch_add(dec_us as u64 * 1000, Ordering::Relaxed);
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            self.last_decoded_unix_ms
                .store(now.as_millis() as u64, Ordering::Relaxed);
        }
    }

    pub fn record_net_timing(&self, net_us: u32) {
        self.net_time_us.store(net_us, Ordering::Relaxed);
        self.stage_net.push(net_us);
    }

    pub fn record_packet_loss(&self, count: u32) {
        self.packet_loss_counter.fetch_add(count, Ordering::Relaxed);
        self.packet_loss_window.fetch_add(count, Ordering::Relaxed);
    }

    /// Sonda de ENVÍO: costo de entregar el frame al canal de red (watch/escritor).
    pub fn record_send_timing(&self, us: u32) {
        self.stage_send.push(us);
    }

    /// Sonda de RENDER: subida a textura GPU + edad del frame al pintarse (decode→paint).
    pub fn record_render_timing(&self, upload_us: u32, painted_age_ms: u32) {
        self.stage_render.push(upload_us);
        self.stage_paint_age.push(painted_age_ms);
    }

    /// Sonda de JITTER de llegada: desviación del ritmo inter-frames en el receptor.
    pub fn record_jitter(&self, us: u32) {
        self.stage_jitter.push(us);
    }

    /// Pérdida por ventana (para el reporte; el acumulado total queda intacto).
    pub fn take_window_packet_loss(&self) -> u32 {
        self.packet_loss_window.swap(0, Ordering::Relaxed)
    }

    /// Reporte de diagnóstico agregado (ventana rodante ~150 frames por etapa). NO bloquea
    /// el pipeline: solo lectura de anillos y contadores atómicos.
    pub fn get_video_diag_report(&self) -> String {
        let (cap_avg, cap_max, _) = self.stage_cap.snapshot_ms();
        let (enc_avg, enc_max, _) = self.stage_enc.snapshot_ms();
        let (send_avg, send_max, _) = self.stage_send.snapshot_ms();
        let (dec_avg, dec_max, _) = self.stage_dec.snapshot_ms();
        let (net_avg, net_max, _) = self.stage_net.snapshot_ms();
        let (ren_avg, ren_max, _) = self.stage_render.snapshot_ms();
        let (age_avg, age_max, _) = self.stage_paint_age.snapshot_ms();
        let (jit_avg, jit_max, _) = self.stage_jitter.snapshot_ms();

        // Ocupación sobre ventana de reporte de 5 s
        const WINDOW_NS: f64 = 5_000_000_000.0;
        let tx_busy_pct = (self.tx_pipeline_busy_ns.swap(0, Ordering::Relaxed) as f64
            / WINDOW_NS
            * 100.0)
            .min(100.0);
        let dec_busy_pct = (self.decode_busy_ns.swap(0, Ordering::Relaxed) as f64
            / WINDOW_NS
            * 100.0)
            .min(100.0);
        let loss = self.take_window_packet_loss();
        let mem = query_process_memory();
        let transport_desc = self
            .try_get_signaling_client()
            .map(|s| s.transport_summary())
            .unwrap_or_else(|| "Desconectado".to_string());

        let bottleneck_eval = if net_avg > 50.0 || self.rtt_ms.load(Ordering::Relaxed) > 120 {
            "⚠️ CUELLO DE BOTELLA: Red lenta / Latencia RTT elevada"
        } else if enc_avg > 14.0 {
            "⚠️ CUELLO DE BOTELLA: CPU del Host sobrecargada (Compresión JPEG >14ms)"
        } else if cap_avg > 16.0 {
            "⚠️ CUELLO DE BOTELLA: Captura de Pantalla en Host lenta (DWM/GDI >16ms)"
        } else if dec_avg > 14.0 {
            "⚠️ CUELLO DE BOTELLA: CPU del Visor lenta (Descompresión SIMD >14ms)"
        } else if loss > 0 {
            "⚠️ ADVERTENCIA: Paquetes perdidos en el enlace"
        } else {
            "✅ RENDIMIENTO: Óptimo y Fluido"
        };

        let role_desc = if self.is_host_active() {
            "📡 ANFITRIÓN (Transmitiendo escritorio local)"
        } else if self.is_viewer_active() {
            "🖥️ VISOR REMOTO (Recibiendo y mostrando pantalla)"
        } else {
            "⏸️ EN REPOSO (Esperando conexión)"
        };

        let tx_str = if self.is_viewer_active() && !self.is_host_active() {
            "0 FPS (En reposo: este equipo actúa como Visor)".to_string()
        } else {
            format!(
                "{} FPS · {} Kbps · res {}x{} · Q{} · cadencia {} · max_dim {}{}",
                self.tx_fps.load(Ordering::Relaxed),
                self.tx_kbps.load(Ordering::Relaxed),
                self.local_width.load(Ordering::Relaxed),
                self.local_height.load(Ordering::Relaxed),
                self.current_quality.load(Ordering::Relaxed),
                self.current_fps.load(Ordering::Relaxed),
                self.current_max_dim.load(Ordering::Relaxed),
                if self.current_max_dim.load(Ordering::Relaxed) > 0 { " (escalado)" } else { " (100% nativo)" }
            )
        };

        let rx_str = if self.is_host_active() && !self.is_viewer_active() {
            "0 FPS (En reposo: este equipo actúa como Anfitrión)".to_string()
        } else {
            format!(
                "{} FPS · {} Kbps · res {}x{}",
                self.rx_fps.load(Ordering::Relaxed),
                self.rx_kbps.load(Ordering::Relaxed),
                self.remote_width.load(Ordering::Relaxed),
                self.remote_height.load(Ordering::Relaxed)
            )
        };

        format!(
            "🩺 [DiagNX 5s] ───────────────────────────────────────────────────────────────\n\
             🩺 ROL ACTIVO: {}\n\
             🩺 MEMORIA (RAM): Física (Working Set): {:.1} MB · Privada: {:.1} MB · Pico: {:.1} MB\n\
             🩺 TX: {}\n\
             🩺 RX: {}\n\
             🩺 ETAPAS media|máx ms → Cap {:.1}|{:.1} · Enc {:.1}|{:.1} · Send {:.2}|{:.2} · Net(RTT/2) {:.1}|{:.1} · Dec {:.1}|{:.1} · Render(subida) {:.2}|{:.2} · Edad al pintar {:.1}|{:.1} ms\n\
             🩺 RTT: {} ms (EMA) · Jitter RX: {:.1}|{:.1} ms · Secuencia perdida (5 s): {} · Ocupación TX {}% · Decoder {}%\n\
             🩺 Transporte: {} · Diagnóstico: {}\n\
             🩺 ─────────────────────────────────────────────────────────────────────────",
            role_desc,
            mem.working_set_mb,
            mem.private_usage_mb,
            mem.peak_working_set_mb,
            tx_str,
            rx_str,
            cap_avg, cap_max,
            enc_avg, enc_max,
            send_avg, send_max,
            net_avg, net_max,
            dec_avg, dec_max,
            ren_avg, ren_max,
            age_avg, age_max,
            self.rtt_ms.load(Ordering::Relaxed),
            jit_avg, jit_max,
            loss,
            tx_busy_pct, dec_busy_pct,
            transport_desc,
            bottleneck_eval
        )
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
                allow_terminal: decision.allow_terminal,
                start_time: now,
            };

            {
                let mut active = self.active_session.write().await;
                *active = Some(session_info);
            }
            self.is_host_active.store(true, Ordering::Relaxed);
            self.is_screen_sharing.store(true, Ordering::Relaxed);
            self.connected_viewers.fetch_add(1, Ordering::Relaxed);
            self.allow_remote_input.store(decision.allow_input, Ordering::Relaxed);
            self.set_remote_terminal_allowed(decision.allow_terminal).await;
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
                allow_terminal: decision.allow_terminal,
                error_message: None,
            }
        } else {
            self.is_host_active.store(false, Ordering::Relaxed);
            self.allow_remote_input.store(false, Ordering::Relaxed);
            self.set_remote_terminal_allowed(false).await;
            SessionResponseDto {
                success: false,
                request_id: req.request_id.clone(),
                session_token: None,
                granted_role: SessionRole::ViewOnly,
                allow_input: false,
                allow_clipboard: false,
                allow_file_transfer: false,
                allow_terminal: false,
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

    /// Descarta la solicitud de sesión pendiente (si existe) y su canal de espera.
    pub async fn clear_pending_session(&self) {
        let pending = {
            let mut p = self.pending_session.write().await;
            p.take()
        };
        if let Some(req) = pending {
            let mut waiters = self.session_waiters.lock().await;
            waiters.remove(&req.request_id);
        }
    }

    /// Limpia TODO el estado local de sesión (anfitrión y visor). No toca la red.
    /// Devuelve `true` si había algún estado de sesión que limpiar.
    pub async fn terminate_session(&self) -> bool {
        let had_active_session = {
            let mut active = self.active_session.write().await;
            active.take().is_some()
        };

        let was_host_active = self.is_host_active.swap(false, Ordering::SeqCst);
        let was_viewer_active = self.is_viewer_active.swap(false, Ordering::SeqCst);
        self.is_screen_sharing.store(false, Ordering::SeqCst);
        self.connected_viewers.store(0, Ordering::SeqCst);
        self.allow_remote_input.store(false, Ordering::SeqCst);

        let had_remote_peer = {
            let mut remote_peer = self.active_remote_peer.write().await;
            remote_peer.take().is_some()
        };

        // Evitar que queden imágenes de la sesión anterior.
        {
            let mut frame = self.latest_jpeg_frame.write().await;
            *frame = None;
        }
        {
            let mut image = self.latest_decoded_image.write().await;
            *image = None;
        }

        // Catálogo de pantallas y telemetría del par anterior.
        {
            let mut screens = self.remote_screens.write().await;
            screens.clear();
        }
        self.remote_width.store(0, Ordering::Relaxed);
        let is_wan = {
            if let Ok(cfg) = self.config.try_read() {
                cfg.signaling_server_url.starts_with("wss://")
                    || cfg.signaling_server_url.contains("trycloudflare.com")
                    || cfg.signaling_server_url.contains("ngrok")
            } else {
                false
            }
        };
        self.rtt_ms.store(if is_wan { 85 } else { 0 }, Ordering::Relaxed);

        had_active_session || had_remote_peer || was_host_active || was_viewer_active
    }

    /// Finaliza la sesión (anfitrión y/o visor), notifica al par y deja la app en estado inicial.
    ///
    /// CAUSA RAÍZ DEL BUG: antes la desconexión era solo local; nadie enviaba `session_end`, así que
    /// el otro extremo mantenía `active_session`/`is_host_active` (anfitrión) o `active_remote_peer`
    /// (visor) y su interfaz seguía mostrando los controles de sesión.
    ///
    /// `notify_peer = false` se usa cuando el cierre lo provoca el propio par (evita eco).
    pub async fn end_session(&self, notify_peer: bool) -> bool {
        // Invalida cualquier `initiate_connect` en vuelo.
        self.session_epoch.fetch_add(1, Ordering::SeqCst);

        let my_id = self.config.read().await.device_id.clone();
        let old_remote = self.active_remote_peer.read().await.clone();
        let host_session = self.active_session.read().await.clone();
        let was_viewer = old_remote.is_some() || self.is_viewer_active.load(Ordering::Relaxed);
        let was_host = host_session.is_some() || self.is_host_active.load(Ordering::Relaxed);

        let target_peer = if let Some(ref r) = old_remote {
            Some(r.clone())
        } else if let Some(ref s) = host_session {
            Some(s.peer_id.clone())
        } else {
            None
        };

        // 1) Avisar al par ANTES de salir de la sala (el canal de control preserva el orden).
        if notify_peer && (was_viewer || was_host) {
            if let Some(sig) = self.get_signaling_client().await {
                let mut msg = nxdesk_core::SignalingMessage::new("session_end", &my_id);
                if let Some(ref t) = target_peer {
                    msg = msg.with_target(t);
                }
                let _ = sig.send_message(&msg).await;
                tracing::info!("📢 [Señalización] Notificación 'session_end' enviada a {:?}", target_peer);
            }
        }

        // 2) Reseteo inmediato y completo del socket y keepalive UDP P2P para no dejar puertos ni pings activos
        if let Some(sig) = self.get_signaling_client().await {
            sig.reset_p2p();
        }

        // 3) Limpiar todo el estado local (texturas, buffers, sesiones activas, flags).
        let had_state = self.terminate_session().await;

        // 4) El visor o anfitrión abandona las salas compartidas y regresa a su sala propia
        if let Some(sig) = self.get_signaling_client().await {
            if let Some(ref remote) = old_remote {
                let _ = sig.leave_room(remote).await;
            }
            if let Some(ref s) = host_session {
                let _ = sig.leave_room(&s.peer_id).await;
            }
            let _ = sig.join_room(&my_id).await;
        }

        self.request_ui_repaint();
        had_state || was_viewer || was_host
    }

    /// Procesa un `session_end` recibido del par. Ignora mensajes de equipos ajenos a esta sesión.
    pub async fn handle_remote_session_end(&self, from_peer: &str) -> bool {
        let from = from_peer.trim();

        let (host_side, pending_side, viewer_side) = if from.is_empty() {
            (true, false, false) // emisor desconocido: se aplica a la sesión vigente
        } else {
            let host_side = self
                .active_session
                .read()
                .await
                .as_ref()
                .is_some_and(|s| same_peer_id(&s.peer_id, from));
            let pending_side = self
                .pending_session
                .read()
                .await
                .as_ref()
                .is_some_and(|r| same_peer_id(&r.client_id, from));
            let viewer_side = self
                .active_remote_peer
                .read()
                .await
                .as_deref()
                .is_some_and(|p| same_peer_id(p, from));
            (host_side, pending_side, viewer_side)
        };

        if !(host_side || pending_side || viewer_side) {
            return false;
        }

        // El visor canceló antes de que el anfitrión aceptara: retirar el diálogo de solicitud.
        if pending_side {
            self.clear_pending_session().await;
            if !host_side && !viewer_side {
                self.request_ui_repaint();
                return true;
            }
        }

        self.end_session(false).await;
        true
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
            target_id: Some("host-999".to_string()),
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
            client_id: Some("client-456".to_string()),
            target_id: Some("client-456".to_string()),
            accept: true,
            granted_role: Some(SessionRole::FullControl),
            allow_input: true,
            allow_clipboard: true,
            allow_file_transfer: false,
            allow_terminal: true,
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

    #[tokio::test]
    async fn test_end_session_resets_all_session_state() {
        let state = AppState::new(AppConfig::default());
        state
            .set_active_remote_peer(Some("host-1".to_string()))
            .await;
        state.is_host_active.store(true, Ordering::SeqCst);
        let epoch_before = state.session_epoch.load(Ordering::SeqCst);

        assert!(state.end_session(true).await);
        assert!(state.get_active_remote_peer().await.is_none());
        assert!(!state.is_viewer_active());
        assert!(!state.is_host_active());
        assert!(state.session_epoch.load(Ordering::SeqCst) > epoch_before);
    }

    #[tokio::test]
    async fn test_remote_session_end_ignores_unrelated_peer() {
        let state = AppState::new(AppConfig::default());
        state
            .set_active_remote_peer(Some("482-195-302".to_string()))
            .await;

        assert!(!state.handle_remote_session_end("999-999-999").await);
        assert!(state.is_viewer_active());

        // Mismo ID aunque se haya escrito sin guiones.
        assert!(state.handle_remote_session_end("482195302").await);
        assert!(!state.is_viewer_active());
        assert!(state.get_active_remote_peer().await.is_none());
    }
}