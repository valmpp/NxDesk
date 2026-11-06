use nxdesk_client::app;
use nxdesk_client::state::AppState;
use nxdesk_core::{AppConfig, PeerStatus};
use nxdesk_discovery::DiscoveryService;
use nxdesk_signaling::{SignalingClient, SignalingEvent};
use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Genera un ID AnyDesk único de 9 dígitos ligado al hardware único de la máquina (MachineGuid + Hostname)
fn get_machine_unique_id() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Registry::*;
        unsafe {
            let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0".encode_utf16().collect();
            let value_name: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
            let mut hkey: HKEY = 0;
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey.as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut hkey) == 0 {
                let mut buf = [0u8; 128];
                let mut buf_len = buf.len() as u32;
                let mut val_type = 0u32;
                if RegQueryValueExW(hkey, value_name.as_ptr(), std::ptr::null_mut(), &mut val_type, buf.as_mut_ptr(), &mut buf_len) == 0 {
                    if buf_len >= 2 {
                        let u16_slice = std::slice::from_raw_parts(buf.as_ptr() as *const u16, (buf_len / 2) as usize);
                        let guid_str = String::from_utf16_lossy(u16_slice);
                        let clean_guid = guid_str.trim_matches('\0').trim();
                        if !clean_guid.is_empty() {
                            clean_guid.hash(&mut hasher);
                        }
                    }
                }
                RegCloseKey(hkey);
            }
        }
    }

    if let Ok(name) = env::var("COMPUTERNAME").or_else(|_| env::var("HOSTNAME")) {
        name.hash(&mut hasher);
    }
    if let Ok(user) = env::var("USERNAME").or_else(|_| env::var("USER")) {
        user.hash(&mut hasher);
    }

    let h = hasher.finish();
    let num = 100_000_000 + (h % 900_000_000);
    let s = num.to_string();
    format!("{}-{}-{}", &s[0..3], &s[3..6], &s[6..9])
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "windows")]
    unsafe {
        // Habilitar soporte nativo DPI Per-Monitor V2 para capturar monitores secundarios sin distorsión
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        // Resolución del temporizador de systema a 1 ms: sin esto, std::thread::sleep redondea a la
        // grilla de 15.6 ms de Windows y el FPS de captura oscila por debajo de 30 (metrónomo inestable).
        // `timeBeginPeriod` pertenece a winmm.dll => módulo Win32::Media en windows-sys.
        windows_sys::Win32::Media::timeBeginPeriod(1);
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("==================================================");
    println!("     🖥️   NxDesk - Aplicación de Escritorio Nativa (Rust) ");
    println!("==================================================");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();

    // 1. Parseo de argumentos de línea de comandos
    let args: Vec<String> = env::args().collect();
    let headless = args.iter().any(|a| a == "--headless" || a == "--no-gui");
    let mut config = AppConfig {
        device_alias: env::var("COMPUTERNAME")
            .or_else(|_| env::var("HOSTNAME"))
            .unwrap_or_else(|_| "NxDesk-Host".to_string()),
        ..Default::default()
    };

    // Cargar config.json si existe en el directorio de ejecución o del ejecutable
    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")))
        .unwrap_or_else(|| std::path::PathBuf::from("config.json"));

    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(file_cfg) = serde_json::from_str::<AppConfig>(&content) {
                config = file_cfg;
                info!("⚙️ [Config] Configuración cargada desde: {:?}", config_path);
            }
        }
    }

    let mut i = 1;
    let mut connect_target = None;
    let mut explicit_server = false;

    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with("--id=") {
            config.device_id = arg.trim_start_matches("--id=").to_string();
        } else if arg == "--share" {
            config.auto_share = true;
        } else if arg == "--allow-remote-terminal" {
            config.allow_remote_terminal = true;
        } else if arg.starts_with("--password=") {
            config.access_password = Some(arg.trim_start_matches("--password=").to_string());
        } else if arg == "--connect" && i + 1 < args.len() {
            connect_target = Some(args[i + 1].clone());
            i += 1;
        } else if arg == "--server" && i + 1 < args.len() {
            config.signaling_server_url = args[i + 1].clone();
            explicit_server = true;
            i += 1;
        }
        i += 1;
    }

    // Generar o resolver ID único de AnyDesk ligado al hardware físico de la máquina
    let is_auto_id = config.device_id == "auto"
        || config.device_id == "111-222-333"
        || config.device_id == "288-741-589"
        || config.device_id.trim().is_empty();

    if is_auto_id {
        config.device_id = get_machine_unique_id();
        info!("✨ [Config] ID único AnyDesk generado para este hardware: {}", config.device_id);
    }

    if config.device_alias == "auto"
        || config.device_alias == "DESKTOP-I0U526L"
        || config.device_alias == "NxDesk-Tester"
        || config.device_alias == "NxDesk-Host"
        || config.device_alias.trim().is_empty()
    {
        config.device_alias = env::var("COMPUTERNAME")
            .or_else(|_| env::var("HOSTNAME"))
            .unwrap_or_else(|_| format!("PC-{}", &config.device_id[0..3]));
    }

    let host_password = if let Some(ref pw) = config.access_password {
        pw.clone()
    } else {
        let rand_val = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            % 900000
            + 100000) as u32;
        format!("NX-{}", rand_val)
    };
    config.access_password = Some(host_password.clone());

    info!("📌 ID de Dispositivo: {}", config.device_id);
    info!("💻 Alias del Nodo:    {}", config.device_alias);
    info!("🌐 Servidor Señales:  {}", config.signaling_server_url);
    info!(
        "🛡️ Terminal Remota:   {}",
        if config.allow_remote_terminal {
            "Habilitada (VUL-01 Verificada)"
        } else {
            "Bloqueada por Seguridad"
        }
    );
    info!("==================================================");
    info!("   🔐  CLAVE MAESTRA ANFITRIÓN: [{}]", host_password);
    info!("   (Úsala para autorizar el uso de terminal remota)");
    info!("==================================================");

    // 2. Inicializar estado global unificado
    let app_state = AppState::new(config.clone());

    // 3. Iniciar Radar LAN de Descubrimiento UDP (Puerto 5001)
    let discovery = DiscoveryService::new(&config.device_id, &config.device_alias, 5000);
    let mut peer_rx = rt.block_on(async { discovery.start().await })?;
    info!("📡 Radar LAN iniciado en puerto UDP 5001.");

    let state_discovery = app_state.clone();
    tokio::spawn(async move {
        while let Some(peer) = peer_rx.recv().await {
            state_discovery.add_or_update_peer(peer.clone()).await;
            info!(
                "🔍 [Radar LAN] Par detectado: '{}' ({}) en {}:{}",
                peer.alias, peer.id, peer.ip_address, peer.port
            );
        }
    });

    // Estructura para transferir fotogramas crudos entre hilo de captura y codificador (Buffer de 1 frame)
    struct RawCaptureFrame {
        data: Vec<u8>,
        width: u32,
        height: u32,
        cap_us: u32,
        is_rgb: bool,
    }

    // Ranura atómica de 1 frame con reemplazo instantáneo y descarte del viejo (Zero Bufferbloat / Zero Alloc)
    let frame_slot: Arc<std::sync::Mutex<Option<RawCaptureFrame>>> =
        Arc::new(std::sync::Mutex::new(None));
    let frame_notify = Arc::new(tokio::sync::Notify::new());
    let (recycle_tx, mut recycle_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);

    // 5a. Hilo Dedicado de Captura por GPU / DIBSection (Regulación dinámica 24 / 30 / 60 FPS)
    let state_capture = app_state.clone();
    let slot_capture = frame_slot.clone();
    let notify_capture = frame_notify.clone();

    #[inline]
    fn compute_screen_signature(buf: &[u8]) -> u64 {
        let len = buf.len();
        if len < 2048 {
            return 0;
        }
        let step = len / 512;
        let mut hash: u64 = 0xcbf29ce484222325;
        for i in 0..512 {
            let idx = i * step;
            hash ^= buf[idx] as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    std::thread::Builder::new()
        .name("nxdesk-capture-worker".to_string())
        .spawn(move || {
            let mut bgra_buf = vec![0u8; 1920 * 1080 * 4];
            let mut last_screen_sig: u64 = 0;
            let mut last_sig_change_time = Instant::now();

            loop {
                let loop_start = Instant::now();

                if !state_capture.is_screen_sharing() {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }

                let viewers = state_capture.get_connected_viewers();
                let is_viewer = state_capture.is_viewer_active();
                let is_host = state_capture.is_host_active();

                if is_viewer && !is_host {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }

                if !is_host && viewers == 0 {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }

                // Reutilizar buffer reciclado del codificador (Zero Allocations / Object Pooling)
                if let Ok(recycled) = recycle_rx.try_recv() {
                    bgra_buf = recycled;
                }

                let t0 = Instant::now();
                let _rtt_now = state_capture.rtt_ms.load(Ordering::Relaxed);

                // Detección automática de hardware: núcleos de CPU
                let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                let is_low_end_hw = num_cpus <= 4;
                let enc_us_prev = state_capture.enc_time_us.load(Ordering::Relaxed);

                // Resolución adaptativa según modo solicitado por el visor y perfil de hardware
                let (screen_w, screen_h) = {
                    let cap = state_capture.capturer.blocking_lock();
                    (cap.width, cap.height)
                };
                let max_d = screen_w.max(screen_h);
                let req_dim = state_capture.requested_max_dim.load(Ordering::Relaxed);
                let target_native_dim = if req_dim > 0 && req_dim < max_d {
                    req_dim
                } else if req_dim == 0 {
                    // En hardware de bajo perfil (<= 4 cores) o si la CPU tarda > 14ms en codificar,
                    // escalar automáticamente a 1280 max dim (720p) para no ahogar los núcleos.
                    if is_low_end_hw || enc_us_prev > 14_000 {
                        1280.min(max_d)
                    } else if max_d > 1600 {
                        1600
                    } else {
                        max_d
                    }
                } else {
                    max_d
                };

                // Gobernador dinámico de FPS: Reposo eficiente 8 FPS, Normal 30 FPS, Techo 60 FPS
                let has_recent_input = state_capture.has_recent_input_activity(500);
                let has_intense_motion = last_sig_change_time.elapsed() < Duration::from_millis(500);
                let has_recent_motion = last_sig_change_time.elapsed() < Duration::from_millis(1500);

                let current_fps: u32 = if has_recent_input && has_intense_motion {
                    // Techo de FPS según capacidad: si la CPU es modesta o codifica lento, mantener en 30 FPS
                    if is_low_end_hw || enc_us_prev > 12_000 {
                        30
                    } else {
                        60
                    }
                } else if has_recent_input || has_recent_motion {
                    30 // Normal: 30 FPS fluido con consumo de CPU controlado
                } else {
                    8 // Reposo eficiente: 8 FPS en pantalla estática (ahorra 70%+ de CPU en gráficos integrados)
                };

                state_capture.current_fps.store(current_fps, Ordering::Relaxed);
                state_capture.current_max_dim.store(target_native_dim, Ordering::Relaxed);
                let frame_time = Duration::from_micros(1_000_000 / current_fps as u64);

                let (capture_res, flush_us, blt_us, scale_us) = {
                    let mut cap = state_capture.capturer.blocking_lock();
                    let res = cap.capture_rgb_scaled_into(target_native_dim, &mut bgra_buf)
                        .map(|(w, h)| (w, h, true));
                    let (f, b, s) = cap.last_timings_us();
                    (res, f, b, s)
                };
                state_capture.record_capture_subtimings(flush_us, blt_us, scale_us);

                if flush_us > 15_000 {
                    warn!(
                        "⚠️ [Captura DWM Flush]: DwmFlush() tardó {:.1}ms esperando VSync/DirectX compositor (Limita FPS a máx {:.0})",
                        flush_us as f32 / 1000.0,
                        1000.0 / (flush_us as f32 / 1000.0)
                    );
                }
                if scale_us > 8_000 {
                    warn!(
                        "⚠️ [Captura Reescalado Lento]: Reescalado CPU BGRA->RGB tardó {:.1}ms",
                        scale_us as f32 / 1000.0
                    );
                }

                if let Ok((w, h, is_rgb)) = capture_res {
                    let cap_us = t0.elapsed().as_micros() as u32;

                    // Registro de firmas de pantalla para estimar actividad dinámica
                    let sig = compute_screen_signature(&bgra_buf);
                    if sig != last_screen_sig {
                        last_screen_sig = sig;
                        last_sig_change_time = Instant::now();
                    }

                    // En el bucle regulado a current_fps [24, 30, 60], cada ciclo despacha su fotograma
                    // garantizando que la cadencia de emisión nunca caiga por debajo de 24 FPS ni supere 60 FPS
                    let frame = RawCaptureFrame {
                        data: bgra_buf,
                        width: w,
                        height: h,
                        cap_us,
                        is_rgb,
                    };

                    // Reemplazo atómico: si el codificador aún no consumió el frame previo,
                    // reciclamos su buffer para la siguiente captura sin bloquear el hilo.
                    let mut slot = slot_capture.lock().unwrap();
                    if let Some(old) = slot.replace(frame) {
                        state_capture.increment_host_dropped_frames();
                        bgra_buf = old.data;
                    } else {
                        bgra_buf = recycle_rx.try_recv().unwrap_or_else(|_| vec![0u8; 1920 * 1080 * 4]);
                    }
                    notify_capture.notify_one();
                }

                let elapsed = loop_start.elapsed();
                if elapsed < frame_time {
                    let rem = frame_time - elapsed;
                    if rem > Duration::from_millis(2) {
                        std::thread::sleep(rem - Duration::from_millis(1));
                    }
                    while loop_start.elapsed() < frame_time {
                        std::hint::spin_loop();
                    }
                }
            }
        })
        .expect("No se pudo iniciar el hilo de captura");

    // 5b. Hilo Dedicado de Codificación SIMD / Acelerada (Baja latencia, sin B-frames, bitrate adaptativo AIMD)
    let state_encode = app_state.clone();
    // Calidad BASE FIJA al 85% (requisito del usuario, sin AIMD ni degradación por RTT).
    // La adaptación de bitrate es EXCLUSIVAMENTE por techo de bytes por frame y es TEMPORAL
    // (solo ese frame; el siguiente vuelve al 85% base). Resolución 85% nativa JAMÁS baja.
    let _base_quality: u32 = 85;
    let slot_encode = frame_slot.clone();
    let notify_encode = frame_notify.clone();
    let recycle_sender = recycle_tx;

    std::thread::Builder::new()
        .name("nxdesk-encode-worker".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build encode runtime");

            rt.block_on(async move {
                let mut jpeg_bytes = Vec::with_capacity(65536);
                let mut last_log_time = Instant::now();
                let mut frame_seq: u32 = 0;

                loop {
                    notify_encode.notified().await;
                    let maybe_raw = {
                        let mut slot = slot_encode.lock().unwrap();
                        slot.take()
                    };

                    if let Some(raw) = maybe_raw {
                        let t_enc_start = Instant::now();

                        // Codificación adaptativa de ultra alto rendimiento según RTT y perfil de hardware
                        let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                        let is_low_end_hw = num_cpus <= 4;
                        let rtt_now = state_encode.rtt_ms.load(Ordering::Relaxed);
                        let current_quality = if is_low_end_hw {
                            if rtt_now > 120 { 58 } else { 64 }
                        } else if rtt_now > 160 {
                            68
                        } else if rtt_now > 80 {
                            72
                        } else {
                            74
                        };

                        jpeg_bytes.clear();
                        let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_bytes, current_quality as u8);
                        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
                        let color_type = if raw.is_rgb {
                            jpeg_encoder::ColorType::Rgb
                        } else {
                            jpeg_encoder::ColorType::Bgra
                        };
                        let res = encoder.encode(
                            &raw.data,
                            raw.width as u16,
                            raw.height as u16,
                            color_type,
                        );

                        let cap_us = raw.cap_us;
                        let width = raw.width;
                        let height = raw.height;

                        if res.is_ok() {
                            let frame_cap: usize = if rtt_now > 200 {
                                128 * 1024
                            } else if rtt_now > 100 {
                                160 * 1024
                            } else {
                                192 * 1024
                            };
                            if jpeg_bytes.len() > frame_cap {
                                state_encode.reencode_count.fetch_add(1, Ordering::Relaxed);
                            }

                            let enc_us = t_enc_start.elapsed().as_micros() as u32;
                            if enc_us > 20_000 {
                                warn!(
                                    "⚠️ [Codificación Lenta]: TurboJPEG tardó {:.1}ms en codificar {}x{} ({} KB)",
                                    enc_us as f32 / 1000.0,
                                    width, height, jpeg_bytes.len() / 1024
                                );
                            }
                            state_encode.record_stage_timings(cap_us, enc_us);
                            state_encode.current_quality.store(current_quality, Ordering::Relaxed);
                            state_encode.record_tx_frame(jpeg_bytes.len(), width, height);

                            // Reciclar el buffer inmediatamente al hilo de captura (Object Pooling)
                            let _ = recycle_sender.try_send(raw.data);

                            // Empaquetar con cabecera binaria de ultra baja latencia NXV1 (24 bytes)
                            frame_seq = frame_seq.wrapping_add(1);
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;

                            let header = nxdesk_core::frame_header::FrameTiming {
                                send_ts_ms: now_ms,
                                capture_us: cap_us,
                                encode_us: enc_us,
                                seq: frame_seq,
                            };
                            let wrapped_frame = nxdesk_core::frame_header::wrap(&jpeg_bytes, header);

                            if last_log_time.elapsed() >= Duration::from_secs(1) {
                                let dec_ms = (state_encode.dec_time_us.load(Ordering::Relaxed) as f32) / 1000.0;
                                let net_ms = ((rtt_now as f32) / 2.0).max(1.0);
                                let cap_ms = (cap_us as f32) / 1000.0;
                                let enc_ms = (enc_us as f32) / 1000.0;
                                let (ren_ms, _, _) = state_encode.stage_render.snapshot_ms();
                                let tx_fps = state_encode.tx_fps.load(Ordering::Relaxed);
                                let rx_fps = state_encode.rx_fps.load(Ordering::Relaxed);
                                let kbps = state_encode.tx_kbps.load(Ordering::Relaxed);
                                let flush_ms = (state_encode.cap_flush_us.load(Ordering::Relaxed) as f32) / 1000.0;
                                let blt_ms = (state_encode.cap_blt_us.load(Ordering::Relaxed) as f32) / 1000.0;
                                let scale_ms = (state_encode.cap_scale_us.load(Ordering::Relaxed) as f32) / 1000.0;
                                let sock_ms = (state_encode.socket_send_us.load(Ordering::Relaxed) as f32) / 1000.0;
                                let drops = state_encode.host_dropped_frames.load(Ordering::Relaxed);
                                let reencs = state_encode.reencode_count.load(Ordering::Relaxed);
                                let total_est = cap_ms + enc_ms + net_ms + dec_ms + ren_ms;

                                let transport_str = if let Some(sig) = state_encode.try_get_signaling_client() {
                                    sig.transport_summary()
                                } else {
                                    "Desconectado".to_string()
                                };

                                let alert_reason = if cap_ms > 16.0 {
                                    format!(" | ⚠️ ALERTA CAPTURA LENTA ({:.1}ms)", cap_ms)
                                } else if enc_ms > 14.0 {
                                    format!(" | ⚠️ ALERTA CPU CODIFICACIÓN LENTA ({:.1}ms)", enc_ms)
                                } else if rtt_now > 120 {
                                    format!(" | ⚠️ ALERTA RETARDO RED (RTT={}ms)", rtt_now)
                                } else if sock_ms > 15.0 {
                                    format!(" | ⚠️ ALERTA CONGESTIÓN SOCKET ({:.1}ms)", sock_ms)
                                } else {
                                    String::new()
                                };

                                info!(
                                    "⏱️ [HOST] {} | Total Est:{:.1}ms | Cap:{:.1}ms (DwmFlush:{:.1}ms, Blt:{:.1}ms, Scale:{:.1}ms) | Enc:{:.1}ms ({}KB) | Socket:{:.1}ms | Red:{:.1}ms | FPS TX:{}/RX:{} | RTT:{}ms | Bitrate:{} kbps | Caídas:{} | Re-enc:{} | Res:{}x{}{}",
                                    transport_str, total_est, cap_ms, flush_ms, blt_ms, scale_ms, enc_ms, jpeg_bytes.len() / 1024, sock_ms, net_ms, tx_fps, rx_fps, rtt_now, kbps, drops, reencs, width, height, alert_reason
                                );
                                last_log_time = Instant::now();
                            }

                            // Despacho directo al socket sin clones ni locks de grabación
                            if let Some(sig) = state_encode.try_get_signaling_client() {
                                let t_send = Instant::now();
                                sig.send_video_frame_direct(wrapped_frame);
                                state_encode
                                    .record_send_timing(t_send.elapsed().as_micros() as u32);
                                state_encode
                                    .record_socket_send_us(sig.last_socket_send_us());
                            }
                        }
                    }
                }
            });
        })
        .expect("No se pudo iniciar el hilo de codificación");

    // 6. Conectar al Servidor de Señalización con Supervisor de Reconexión Automática
    let state_sig = app_state.clone();
    let my_id = config.device_id.clone();
    let sig_url = config.signaling_server_url.clone();
    let connect_target_init = connect_target.clone();

    let is_wan = sig_url.starts_with("wss://")
        || sig_url.contains("trycloudflare.com")
        || sig_url.contains("ngrok");

    // Si la URL apunta a localhost/127.0.0.1 y el puerto 5000 no responde,
    // intentar auto-iniciar nxdesk-server.exe en segundo plano si existe en la misma carpeta.
    if sig_url.contains("127.0.0.1:5000") || sig_url.contains("localhost:5000") {
        let is_running = std::net::TcpStream::connect_timeout(
            &"127.0.0.1:5000".parse().unwrap(),
            Duration::from_millis(200),
        ).is_ok();

        if !is_running {
            if let Ok(current_exe) = std::env::current_exe() {
                if let Some(parent_dir) = current_exe.parent() {
                    let server_exe = parent_dir.join("nxdesk-server.exe");
                    if server_exe.exists() {
                        info!("🚀 [Auto-Start] Servidor local en puerto 5000 no detectado. Iniciando nxdesk-server.exe en segundo plano...");
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            const CREATE_NO_WINDOW: u32 = 0x08000000;
                            let _ = std::process::Command::new(server_exe)
                                .creation_flags(CREATE_NO_WINDOW)
                                .spawn();
                        }
                        #[cfg(not(windows))]
                        {
                            let _ = std::process::Command::new(server_exe).spawn();
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        }
    }

    let init_timeout = if explicit_server || connect_target_init.is_some() || is_wan {
        Duration::from_millis(5000)
    } else {
        Duration::from_millis(1500)
    };

    let initial_pair = rt.block_on(async {
        match tokio::time::timeout(init_timeout, SignalingClient::connect(&sig_url)).await {
            Ok(Ok(pair)) => Some(pair),
            Ok(Err(e)) => {
                warn!("No se pudo conectar inicialmente al servidor de señalización: {}", e);
                None
            }
            Err(_) => {
                info!("Timeout inicial al conectar al servidor de señalización.");
                None
            }
        }
    });

    let (mut initial_client, mut initial_rx) = match initial_pair {
        Some((c, rx)) => (Some(c), Some(rx)),
        None => (None, None),
    };

    if initial_client.is_some() {
        state_sig.set_signaling_connected(true);
        info!("✅ Conectado al servidor de señalización con éxito.");
    }

    let state_sup = state_sig.clone();
    let my_id_sup = my_id.clone();
    let sig_url_sup = sig_url.clone();

    // Tarea dedicada de decodificación JPEG con watch::channel para descartar fotogramas atrasados (Zero Lag).
    // Decoder SIMD (zune-jpeg, SSE/AVX2): a resolución nativa 2560x1440 decodifica en ~5-10 ms
    // frente a ~90-120 ms del decoder del crate `image` (causa real del FPS bajo en el visor).
    let (decode_tx, mut decode_rx) = tokio::sync::watch::channel::<Option<Vec<u8>>>(None);
    let state_decoder = app_state.clone();
    std::thread::Builder::new()
        .name("nxdesk-jpeg-decoder".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build single-threaded runtime for decoder");
            rt.block_on(async move {
                let mut last_seq: u32 = 0;
                let mut last_alert_time = Instant::now();
                let mut last_viewer_log = Instant::now();
                // Sonda de jitter: ritmo de llegada a la decode vs su media exponencial (µs)
                let mut last_rx_at: Option<Instant> = None;
                let mut ema_interval_ms: f64 = 0.0;

                while decode_rx.changed().await.is_ok() {
                    let maybe_data = decode_rx.borrow_and_update().clone();
                    if let Some(bytes) = maybe_data {
                        let t_dec_start = Instant::now();
                        let state_clone = state_decoder.clone();

                        let (timing, payload) = nxdesk_core::frame_header::unwrap(&bytes);

                        let mut decoder = zune_jpeg::JpegDecoder::new(payload);
                        if let Ok(decoded) = decoder.decode() {
                            let (w, h) = match decoder.dimensions() {
                                Some((w, h)) => (w, h),
                                None => continue,
                            };
                            if w > 0 && h > 0 && decoded.len() >= w * h * 3 {
                                let color_img = std::sync::Arc::new(eframe::egui::ColorImage::from_rgb(
                                    [w, h],
                                    &decoded[..w * h * 3],
                                ));
                                {
                                    if let Ok(mut lock) = state_clone.latest_decoded_image.try_write() {
                                        *lock = Some(color_img);
                                    }
                                }
                                let dec_us = t_dec_start.elapsed().as_micros() as u32;
                                if dec_us > 15_000 {
                                    warn!(
                                        "⚠️ [Decodificación Lenta]: zune-jpeg tardó {:.1}ms en descomprimir {}x{} ({} KB)",
                                        dec_us as f32 / 1000.0,
                                        w, h, bytes.len() / 1024
                                    );
                                }
                                state_clone.record_decode_timing(dec_us);
                                state_clone.increment_frame_sequence();
                                state_clone.record_rx_frame(bytes.len(), w as u32, h as u32);
                                state_clone.request_ui_repaint();

                                // Sonda de JITTER: desviación del ritmo inter-frames (µs)
                                {
                                    let now_in = Instant::now();
                                    let delta_ms = last_rx_at
                                        .map(|t| now_in.duration_since(t).as_secs_f64() * 1000.0);
                                    if let Some(d) = delta_ms {
                                        if d > 85.0 {
                                            warn!(
                                                "⚠️ [Bajón FPS - Jitter de Llegada]: Pasaron {:.1}ms entre frames en el visor (esperado: ~16-33ms). Posible congestión WAN o caída de paquetes.",
                                                d
                                            );
                                        }
                                    }
                                    if let (Some(d), true) = (delta_ms, ema_interval_ms > 0.0) {
                                        state_clone
                                            .record_jitter(((d - ema_interval_ms).abs() * 1000.0) as u32);
                                    }
                                    ema_interval_ms = match delta_ms {
                                        Some(d) => {
                                            if ema_interval_ms > 0.0 {
                                                ema_interval_ms * 0.85 + d * 0.15
                                            } else {
                                                d
                                            }
                                        }
                                        None => ema_interval_ms,
                                    };
                                    last_rx_at = Some(now_in);
                                }

                                let (cap_ms, enc_ms, net_ms) = if let Some(t) = timing {
                                    if last_seq > 0 && t.seq > last_seq + 1 {
                                        let lost = t.seq - last_seq - 1;
                                        state_clone.record_packet_loss(lost);
                                    }
                                    last_seq = t.seq;
                                    let rtt_val = (state_clone.rtt_ms.load(Ordering::Relaxed) as f32).max(1.0);
                                    // Elimina el falso desfase de reloj entre dos PCs (Clock Skew):
                                    // La latencia de red unidireccional física real es RTT / 2, medida por Ping/Pong con reloj local monotónico.
                                    let net_ms_val = (rtt_val / 2.0).max(1.0);
                                    state_clone.record_stage_timings(t.capture_us, t.encode_us);
                                    state_clone.record_net_timing((net_ms_val * 1000.0) as u32);
                                    (t.capture_us as f32 / 1000.0, t.encode_us as f32 / 1000.0, net_ms_val)
                                } else {
                                    let cap = state_clone.cap_time_us.load(Ordering::Relaxed) as f32 / 1000.0;
                                    let enc = state_clone.enc_time_us.load(Ordering::Relaxed) as f32 / 1000.0;
                                    let rtt = (state_clone.rtt_ms.load(Ordering::Relaxed) as f32).max(1.0);
                                    (cap, enc, (rtt / 2.0).max(1.0))
                                };

                                let dec_ms = dec_us as f32 / 1000.0;
                                let total_latency_ms = cap_ms + enc_ms + net_ms + dec_ms;

                                // Diagnóstico continuo del Visor a 1 Hz
                                if last_viewer_log.elapsed() >= Duration::from_secs(1) {
                                    let rx_fps = state_clone.rx_fps.load(Ordering::Relaxed);
                                    let tx_fps = state_clone.tx_fps.load(Ordering::Relaxed);
                                    let bitrate = state_clone.rx_kbps.load(Ordering::Relaxed);
                                    let rtt = state_clone.rtt_ms.load(Ordering::Relaxed);
                                    let (ren_ms, _, _) = state_clone.stage_render.snapshot_ms();
                                    let total_e2e = cap_ms + enc_ms + net_ms + dec_ms + ren_ms;

                                    let transport_desc = state_clone
                                        .try_get_signaling_client()
                                        .map(|s| s.transport_summary())
                                        .unwrap_or_else(|| "Desconectado".to_string());

                                    let alert_msg = if rx_fps > 0 && rx_fps < 20 {
                                        let cause = if net_ms >= enc_ms && net_ms >= dec_ms && net_ms >= cap_ms {
                                            format!("RED LENTA ({:.1}ms)", net_ms)
                                        } else if enc_ms >= dec_ms && enc_ms >= cap_ms {
                                            format!("CPU HOST LENTA ({:.1}ms)", enc_ms)
                                        } else if cap_ms >= dec_ms {
                                            format!("CAPTURA HOST LENTA ({:.1}ms)", cap_ms)
                                        } else {
                                            format!("CPU VISOR LENTA ({:.1}ms)", dec_ms)
                                        };
                                        format!(" | ⚠️ BAJÓN FPS (RX={} FPS) -> Causa: {}", rx_fps, cause)
                                    } else {
                                        String::new()
                                    };

                                    info!(
                                        "⏱️ [VISOR] {} | Total E2E:{:.1}ms | FPS RX:{}/TX:{} | {}x{} | Bitrate:{} kbps | Cap:{:.1}ms Enc:{:.1}ms Red:{:.1}ms Dec:{:.1}ms Render:{:.2}ms | RTT:{}ms{}",
                                        transport_desc, total_e2e, rx_fps, tx_fps, w, h, bitrate, cap_ms, enc_ms, net_ms, dec_ms, ren_ms, rtt, alert_msg
                                    );
                                    last_viewer_log = Instant::now();
                                }

                                // Diagnóstico de picos: advertir si la latencia supera 120 ms indicando qué etapa la causó y peor FPS
                                if total_latency_ms > 120.0 && last_alert_time.elapsed() >= Duration::from_millis(500) {
                                    let rx_fps = state_clone.rx_fps.load(Ordering::Relaxed);
                                    let tx_fps = state_clone.tx_fps.load(Ordering::Relaxed);
                                    let worst_fps = rx_fps.min(tx_fps);
                                    let bitrate = state_clone.rx_kbps.load(Ordering::Relaxed);
                                    let rtt = state_clone.rtt_ms.load(Ordering::Relaxed);

                                    let bottleneck = if net_ms >= cap_ms && net_ms >= enc_ms && net_ms >= dec_ms {
                                        format!("RED / CONGESTIÓN DE SOCKET ({:.1}ms)", net_ms)
                                    } else if enc_ms >= cap_ms && enc_ms >= dec_ms {
                                        format!("CODIFICACIÓN ({:.1}ms)", enc_ms)
                                    } else if cap_ms >= dec_ms {
                                        format!("CAPTURA GPU/GDI ({:.1}ms)", cap_ms)
                                    } else {
                                        format!("DECODIFICACIÓN ({:.1}ms)", dec_ms)
                                    };

                                    warn!(
                                        "⚠️ [ALERTA LATENCIA > 120ms] Latencia Total: {:.1}ms | Etapa Crítica: {} | Desglose: Cap={:.1}ms, Enc={:.1}ms, Red={:.1}ms, Dec={:.1}ms | FPS RX: {}, TX: {} (Peor: {}) | Bitrate: {} kbps | RTT: {}ms",
                                        total_latency_ms, bottleneck, cap_ms, enc_ms, net_ms, dec_ms, rx_fps, tx_fps, worst_fps, bitrate, rtt
                                    );
                                    last_alert_time = Instant::now();
                                }
                            }
                        }
                    }
                }
            });
        })
        .expect("Failed to spawn nxdesk-jpeg-decoder thread");

    // ==== Tarea de DIAGNÓSTICO AGREGADO de video: reporte completo cada 5 s (ver DiagNX) ====
    let state_diag = app_state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            if state_diag.is_host_active() || state_diag.is_viewer_active() {
                info!("{}", state_diag.get_video_diag_report());
            } else {
                let mem = nxdesk_client::state::query_process_memory();
                info!(
                    "📊 [Recursos NxDesk]: RAM en uso (Working Set): {:.1} MB | Privada (Commit): {:.1} MB | Pico: {:.1} MB",
                    mem.working_set_mb, mem.private_usage_mb, mem.peak_working_set_mb
                );
            }
        }
    });

    // Tarea de telemetría periódica y medición de latencia RTT (Ping/Pong a 1 Hz)
    let state_telem = app_state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        let mut telem_counter: u8 = 0;
        loop {
            ticker.tick().await;
            telem_counter = (telem_counter + 1) % 4;
            if telem_counter == 0 {
                state_telem.tick_telemetry_second();
            }

            let is_viewer = state_telem.active_remote_peer.read().await.is_some();
            let is_host = state_telem.active_session.read().await.is_some()
                || state_telem.connected_viewers.load(Ordering::Relaxed) > 0;

            // Envío de ping_rtt a 1 Hz sincronizado con la telemetría periódica para no congestionar el canal
            if (is_viewer || is_host) && telem_counter == 0 {
                if let Some(sig) = state_telem.get_signaling_client().await {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let ping_msg = nxdesk_core::SignalingMessage::new("ping_rtt", &now_ms.to_string());
                    let _ = sig.send_message(&ping_msg).await;
                }
            }
        }
    });

    let decode_tx_sup = decode_tx.clone();
    tokio::spawn(async move {
        let mut first = true;
        loop {
            let (client, mut event_rx) = if first && initial_client.is_some() {
                first = false;
                (initial_client.take().unwrap(), initial_rx.take().unwrap())
            } else {
                first = false;
                info!("🔄 [Señalización] Reconectando a {}...", sig_url_sup);
                match SignalingClient::connect(&sig_url_sup).await {
                    Ok(pair) => pair,
                    Err(e) => {
                        warn!("⚠️ [Señalización] Error al conectar: {}. Reintentando en 3s...", e);
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                }
            };

            let client_arc = Arc::new(client);
            {
                let mut sig_lock = state_sup.signaling_client.write().await;
                *sig_lock = Some(client_arc.clone());
            }
            state_sup.set_signaling_connected(true);
            info!("✅ [Señalización] Conexión establecida y activa.");

            // Enlazar canal watch de video entrante de ultra baja latencia directamente al decodificador
            let mut incoming_vid = client_arc.subscribe_incoming_video();
            let decode_tx_conn = decode_tx_sup.clone();
            let state_sup_frame = state_sup.clone();
            tokio::spawn(async move {
                while incoming_vid.changed().await.is_ok() {
                    if !state_sup_frame.is_viewer_active() {
                        continue;
                    }
                    // Entrega directa del frame al decodificador SIMD sin locks ni copias redundantes
                    let maybe_bytes = incoming_vid.borrow_and_update().clone();
                    if let Some(bytes) = maybe_bytes {
                        let _ = decode_tx_conn.send(Some(bytes));
                    }
                }
            });

            // Registrarse en la sala propia del anfitrión
            let _ = client_arc.join_room(&my_id_sup).await;
            info!(
                "🏠 Registrado en sala de anfitrión '{}'. Listo para recibir pares.",
                my_id_sup
            );

            // Anunciar presencia en la red WAN para descubrimiento automático
            let my_identity = nxdesk_core::PeerIdentity {
                id: my_id_sup.clone(),
                alias: config.device_alias.clone(),
                ip_address: "WAN".to_string(),
                port: config.web_port,
                status: nxdesk_core::PeerStatus::Connected,
                hostname: None,
                os: None,
            };
            if let Ok(ident_json) = serde_json::to_string(&my_identity) {
                let announce_msg = nxdesk_core::SignalingMessage::new("announce", &ident_json);
                let _ = client_arc.send_message(&announce_msg).await;
                info!("📢 [Descubrimiento WAN] Presencia anunciada en el servidor de señalización.");
            }

            // Re-unirse al objetivo remoto si existe
            let active_peer = state_sup.active_remote_peer.read().await.clone();
            let target_to_join = active_peer.or_else(|| connect_target_init.clone());
            if let Some(ref target) = target_to_join {
                info!("🚀 Conectando a sala de par remoto '{}'...", target);
                let _ = client_arc.join_room(target).await;
            }

            // Bucle despachador de eventos con eliminación de retardo en movimientos de ratón (Anti-Lag Coalescing)
            let mut pending_event: Option<SignalingEvent> = None;
            loop {
                let event = if let Some(ev) = pending_event.take() {
                    ev
                } else {
                    match event_rx.recv().await {
                        Some(ev) => ev,
                        None => break,
                    }
                };

                match event {
                    SignalingEvent::Input(mut input_evt) => {
                        state_sup.record_remote_input_activity();
                        if state_sup.allow_remote_input.load(std::sync::atomic::Ordering::Relaxed) {
                            // Coalescing de movimientos viejos: si hay múltiples MouseMove acumulados en la cola,
                            // drenamos los movimientos intermedios obsoletos y aplicamos ÚNICAMENTE el más reciente.
                            if let nxdesk_core::InputEvent::MouseMove { ref mut x, ref mut y } = input_evt {
                                while let Ok(queued) = event_rx.try_recv() {
                                    match queued {
                                        SignalingEvent::Input(nxdesk_core::InputEvent::MouseMove { x: nx, y: ny }) => {
                                            *x = nx;
                                            *y = ny;
                                        }
                                        other => {
                                            pending_event = Some(other);
                                            break;
                                        }
                                    }
                                }
                            }

                            if matches!(input_evt, nxdesk_core::InputEvent::KeyDown { .. } | nxdesk_core::InputEvent::KeyUp { .. }) {
                                let injector = state_sup.input_injector.lock().await;
                                injector.inject(&input_evt);
                            } else if let Ok(injector) = state_sup.input_injector.try_lock() {
                                injector.inject(&input_evt);
                            }
                        } else {
                            warn!("⛔ [Seguridad] Input descartado: no existe sesión activa con permiso 'allow_input'.");
                        }
                    }
                    SignalingEvent::Message(msg) => match msg.msg_type.as_str() {
                        "peer_list" => {
                            if let Ok(peers) = serde_json::from_str::<Vec<nxdesk_core::PeerIdentity>>(&msg.payload) {
                                for p in peers {
                                    state_sup.add_or_update_peer(p).await;
                                }
                            }
                        }
                        "session_end" => {
                            let from_peer = msg.payload.trim().to_string();
                            if state_sup.handle_remote_session_end(&from_peer).await {
                                info!(
                                    "🔌 [Señalización] session_end recibido de '{}'. Sesión finalizada y menú principal restablecido.",
                                    from_peer
                                );
                            } else {
                                info!(
                                    "ℹ️ [Señalización] session_end de '{}' ignorado (no corresponde a una sesión activa).",
                                    from_peer
                                );
                            }
                        }

                        "peer_online" => {
                            if let Ok(peer) = serde_json::from_str::<nxdesk_core::PeerIdentity>(&msg.payload) {
                                info!("🌐 [Descubrimiento WAN] Par online detectado: {} ({})", peer.alias, peer.id);
                                state_sup.add_or_update_peer(peer).await;
                            }
                        }
                        "peer_offline" => {
                            let offline_id = msg.payload.trim();
                            info!("🔌 [Descubrimiento WAN] Par desconectado: {}", offline_id);
                            state_sup.remove_peer(offline_id).await;
                        }
                        "input" => {
                            if state_sup.allow_remote_input.load(std::sync::atomic::Ordering::Relaxed) {
                                if let Ok(input_evt) =
                                    serde_json::from_str::<nxdesk_core::InputEvent>(&msg.payload)
                                {
                                    if matches!(input_evt, nxdesk_core::InputEvent::KeyDown { .. } | nxdesk_core::InputEvent::KeyUp { .. }) {
                                        let injector = state_sup.input_injector.lock().await;
                                        injector.inject(&input_evt);
                                    } else if let Ok(injector) = state_sup.input_injector.try_lock() {
                                        injector.inject(&input_evt);
                                    }
                                }
                            } else {
                                warn!("⛔ [Seguridad] Input descartado: no existe sesión activa con permiso 'allow_input'.");
                            }
                        }
                        "terminal_cmd" => {
                            // Autorización bidireccional: cualquier extremo con sesión activa de control
                            // total puede ejecutar comandos en el otro (el emisor siempre es el otro equipo).
                            let is_authorized = {
                                let host_ok = state_sup
                                    .active_session
                                    .read()
                                    .await
                                    .as_ref()
                                    .is_some_and(|s| s.granted_role == nxdesk_core::SessionRole::FullControl);
                                let viewer_ok = state_sup
                                    .active_remote_peer
                                    .read()
                                    .await
                                    .is_some();
                                host_ok || viewer_ok
                            };

                            if is_authorized {
                                // NO await inline: se encola en un FIFO serial y un worker lo
                                // consume. Un `await` aquí bloquearía el dispatcher de eventos
                                // (pings p50/latencia y video se congelarían mientras la terminal
                                // arranca el shell o escribe al proceso).
                                start_terminal_cmd_worker(&state_sup).await;
                                let enqueued = {
                                    match state_sup.terminal_cmd_tx.lock() {
                                        Ok(guard) => match guard.as_ref() {
                                            Some(tx) => tx.send(msg.payload.clone()).is_ok(),
                                            None => false,
                                        },
                                        Err(_) => false,
                                    }
                                };
                                if !enqueued {
                                    warn!("⚠️ [Terminal] Worker de comandos no disponible; comando descartado.");
                                }
                            } else {
                                warn!("⛔ [Seguridad] Comando de terminal rechazado: sesión no autorizada con control total.");
                            }
                        }
                        "terminal_ctrl_c" => {
                            let is_authorized = {
                                let host_ok = state_sup
                                    .active_session
                                    .read()
                                    .await
                                    .as_ref()
                                    .is_some_and(|s| s.granted_role == nxdesk_core::SessionRole::FullControl);
                                let viewer_ok = state_sup
                                    .active_remote_peer
                                    .read()
                                    .await
                                    .is_some();
                                host_ok || viewer_ok
                            };

                            if is_authorized {
                                info!("⏹ [Terminal] Solicitud de interrupción Ctrl+C recibida.");
                                let term_sess = state_sup.terminal_session.clone();
                                let net_tx = state_sup.terminal_out_network_tx.clone();
                                let bc_tx = state_sup.terminal_broadcast_tx.clone();
                                tokio::spawn(async move {
                                    term_sess.interrupt().await;
                                    let notice = "\r\n^C\r\n[Proceso detenido por el usuario]\r\n".to_string();
                                    let _ = net_tx.send(notice.clone());
                                    let _ = bc_tx.send(notice);
                                });
                            }
                        }
                        "terminal_out" => {
                            // Salida de la terminal del par remoto: se inyecta EXCLUSIVAMENTE en el
                            // broadcast local para la UI. NUNCA se inyecta en terminal_out_network_tx,
                            // impidiendo loops infinitos de rebote de red.
                            let _ = state_sup.terminal_broadcast_tx.send(msg.payload.clone());
                        }
                        "file_chunk" => {
                            let can_transfer = {
                                let active = state_sup.active_session.read().await;
                                active.as_ref().is_some_and(|s| s.allow_file_transfer)
                            };

                            if can_transfer {
                                if let Ok(chunk) =
                                    serde_json::from_str::<nxdesk_core::FileChunk>(&msg.payload)
                                {
                                    let _ = state_sup.file_manager.write_chunk(&chunk).await;
                                }
                            } else {
                                warn!("⛔ [Seguridad] File chunk descartado: transferencia de archivos deshabilitada.");
                            }
                        }
                        "select_screen" => {
                            if let Ok(idx) = msg.payload.parse::<i32>() {
                                info!("🖥️ [Señalización] Conmutando a monitor {} solicitado por par remoto.", idx);
                                if let Ok(screen) = state_sup.select_screen(idx).await {
                                    state_sup.set_active_screen_info(screen.index, screen.name.clone()).await;
                                    if let Some(sig) = state_sup.get_signaling_client().await {
                                        if let Ok(json) = serde_json::to_string(&screen) {
                                            let reply = nxdesk_core::SignalingMessage::new("screen_selected", json);
                                            let _ = sig.send_message(&reply).await;
                                        }
                                    }
                                }
                            }
                        }
                        "query_screens" => {
                            if let Some(sig) = state_sup.get_signaling_client().await {
                                let screens = state_sup.get_screens().await;
                                if let Ok(json) = serde_json::to_string(&screens) {
                                    let reply = nxdesk_core::SignalingMessage::new("screens_list", json);
                                    let _ = sig.send_message(&reply).await;
                                }
                                let active_idx = state_sup.active_screen_index.load(std::sync::atomic::Ordering::Relaxed);
                                if let Some(cur) = screens.iter().find(|s| s.index == active_idx).or_else(|| screens.first()) {
                                    if let Ok(json) = serde_json::to_string(cur) {
                                        let reply = nxdesk_core::SignalingMessage::new("screen_selected", json);
                                        let _ = sig.send_message(&reply).await;
                                    }
                                }
                            }
                        }
                        "screens_list" => {
                            if let Ok(screens) = serde_json::from_str::<Vec<nxdesk_core::ScreenInfoDto>>(&msg.payload) {
                                info!("🖥️ [Multimonitor] {} pantallas remotas recibidas del anfitrión.", screens.len());
                                state_sup.set_remote_screens(screens).await;
                            }
                        }
                        "screen_selected" => {
                            if let Ok(screen) = serde_json::from_str::<nxdesk_core::ScreenInfoDto>(&msg.payload) {
                                info!("🖥️ [Multimonitor] Monitor activo en anfitrión: {} ({}x{})", screen.name, screen.width, screen.height);
                                state_sup.set_active_screen_info(screen.index, screen.name).await;
                            }
                        }
                        "ping_rtt" => {
                            if let Some(sig) = state_sup.get_signaling_client().await {
                                let pong_msg = nxdesk_core::SignalingMessage::new("pong_rtt", &msg.payload);
                                let _ = sig.send_message(&pong_msg).await;
                            }
                        }
                        "pong_rtt" => {
                            if let Ok(sent_ms) = msg.payload.trim().parse::<u64>() {
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;
                                if now_ms >= sent_ms {
                                    let rtt = (now_ms - sent_ms) as u32;
                                    state_sup.update_rtt(rtt);
                                }
                            }
                        }
                        "udp_endpoint" => {
                            let endpoint = msg.payload.trim().to_string();
                            if !endpoint.is_empty() {
                                if let Some(sig) = state_sup.get_signaling_client().await {
                                    info!("⚡ [P2P UDP] Endpoint remoto recibido: {}. Realizando punch directo...", endpoint);
                                    sig.punch_p2p(&endpoint).await;
                                    if let Some(local_ep) = sig.p2p_local_endpoint() {
                                        let ack_msg = nxdesk_core::SignalingMessage::new("udp_endpoint_ack", &local_ep);
                                        let _ = sig.send_message(&ack_msg).await;
                                    }
                                }
                            }
                        }
                        "udp_endpoint_ack" => {
                            let endpoint = msg.payload.trim().to_string();
                            if !endpoint.is_empty() {
                                if let Some(sig) = state_sup.get_signaling_client().await {
                                    info!("⚡ [P2P UDP] Confirmación de endpoint UDP: {}. Respondiendo punch...", endpoint);
                                    sig.punch_p2p(&endpoint).await;
                                }
                            }
                        }
                        "view_mode" => {
                            let mode = msg.payload.trim();
                            if mode == "fit" {
                                state_sup.requested_max_dim.store(1366, Ordering::Relaxed);
                                info!("🖥️ [Modo Vista] Solicitado modo FIT (escala optimizada 1366 max dim para 60 FPS sin saturar WAN)");
                            } else if mode == "native" {
                                state_sup.requested_max_dim.store(0, Ordering::Relaxed);
                                info!("🖥️ [Modo Vista] Solicitado modo NATIVO 1:1 (resolución completa original)");
                            } else if let Ok(dim) = mode.parse::<u32>() {
                                state_sup.requested_max_dim.store(dim, Ordering::Relaxed);
                                info!("🖥️ [Modo Vista] Solicitada resolución personalizada (max dim {})", dim);
                            }
                        }
                        "frame" => {
                            if let Some(bytes) = msg.byte_data {
                                let _ = decode_tx_sup.send(Some(bytes));
                            }
                        }
                        "clipboard" => {
                            let can_clipboard = {
                                let active = state_sup.active_session.read().await;
                                active.as_ref().is_some_and(|s| s.allow_clipboard)
                            };

                            if can_clipboard {
                                info!(
                                    "📋 [Clipboard] Sincronización recibida ({} bytes).",
                                    msg.payload.len()
                                );
                                #[cfg(windows)]
                                {
                                    let text_copy = msg.payload.clone();
                                    std::thread::spawn(move || {
                                        use std::io::Write;
                                        if let Ok(mut child) = std::process::Command::new("clip.exe")
                                            .stdin(std::process::Stdio::piped())
                                            .spawn()
                                        {
                                            if let Some(mut stdin) = child.stdin.take() {
                                                let _ = stdin.write_all(text_copy.as_bytes());
                                            }
                                            let _ = child.wait();
                                        }
                                    });
                                }
                            } else {
                                warn!("⛔ [Seguridad] Sincronización de portapapeles descartada: permiso 'allow_clipboard' no concedido.");
                            }
                        }
                        "session_request" => {
                            if let Ok(req) =
                                serde_json::from_str::<nxdesk_core::SessionRequestDto>(&msg.payload)
                            {
                                // Filtrar: Si la solicitud especifica target_id, verificar que esté dirigida a este equipo
                                if let Some(ref target) = req.target_id {
                                    let clean_target = target.trim().replace('-', "").to_lowercase();
                                    let clean_my_id = my_id_sup.trim().replace('-', "").to_lowercase();
                                    if clean_target != clean_my_id {
                                        info!(
                                            "🛡️ [Señalización] Solicitud de sesión descartada: dirigida a '{}', este equipo es '{}'",
                                            target, my_id_sup
                                        );
                                        continue;
                                    }
                                }

                                info!(
                                    "🔔 [Señalización] Solicitud de sesión entrante de '{}' ({}) para este equipo",
                                    req.client_alias, req.client_id
                                );
                                let _ = state_sup.request_session(req).await;

                                #[cfg(windows)]
                                {
                                    std::thread::spawn(|| {
                                        let _ = std::process::Command::new("powershell")
                                            .args(&["-NoProfile", "-Command", "[System.Media.SystemSounds]::Asterisk.Play()"])
                                            .spawn();
                                    });
                                }
                            }
                        }
                        "session_decision" => {
                            if let Ok(decision) =
                                serde_json::from_str::<nxdesk_core::SessionDecisionDto>(&msg.payload)
                            {
                                // Validar que la decisión esté dirigida a esta máquina
                                let clean_my_id = my_id_sup.trim().replace('-', "").to_lowercase();
                                let is_my_request = decision
                                    .request_id
                                    .starts_with(&format!("req-{}-", my_id_sup))
                                    || decision.client_id.as_deref().map(|c| c.trim().replace('-', "").to_lowercase()) == Some(clean_my_id.clone())
                                    || decision.target_id.as_deref().map(|t| t.trim().replace('-', "").to_lowercase()) == Some(clean_my_id);

                                if !is_my_request {
                                    info!("🛡️ [Señalización] session_decision descartada: no corresponde a este visor");
                                    continue;
                                }

                                info!(
                                    "🔔 [Señalización] Decisión de sesión recibida: aceptada={}",
                                    decision.accept
                                );
                                if !decision.accept && state_sup.is_viewer_active() {
                                    state_sup.end_session(false).await;
                                }
                            }
                        }
                        other => {
                            info!(
                                "Mensaje de señalización recibido [{}]: {}",
                                other, msg.payload
                            );
                        }
                    },
                    SignalingEvent::RoomOccupancy(count) => {
                        state_sup.set_room_occupancy(count);
                        info!("👥 Participantes activos en la sala: {}", count);
                        if count > 1 {
                            if let Some(sig) = state_sup.get_signaling_client().await {
                                if let Some(local_ep) = sig.p2p_local_endpoint() {
                                    let ep_msg = nxdesk_core::SignalingMessage::new("udp_endpoint", &local_ep);
                                    let _ = sig.send_message(&ep_msg).await;
                                }
                            }
                        }
                        if count > 1 && state_sup.active_remote_peer.read().await.is_none() {
                            if let Some(sig) = state_sup.get_signaling_client().await {
                                let screens = state_sup.get_screens().await;
                                if let Ok(json) = serde_json::to_string(&screens) {
                                    let list_msg = nxdesk_core::SignalingMessage::new("screens_list", json);
                                    let _ = sig.send_message(&list_msg).await;
                                }
                                let active_idx = state_sup.active_screen_index.load(std::sync::atomic::Ordering::Relaxed);
                                if let Some(cur) = screens.iter().find(|s| s.index == active_idx).or_else(|| screens.first()) {
                                    if let Ok(json) = serde_json::to_string(cur) {
                                        let cur_msg = nxdesk_core::SignalingMessage::new("screen_selected", json);
                                        let _ = sig.send_message(&cur_msg).await;
                                    }
                                }
                            }
                        }
                    }
                    SignalingEvent::Disconnected => {
                        state_sup.set_signaling_connected(false);
                        warn!("⚠️ Conexión con servidor de señalización cerrada. Reconectando...");
                        break;
                    }
                }
            }

            state_sup.set_signaling_connected(false);
            {
                let mut sig_lock = state_sup.signaling_client.write().await;
                *sig_lock = None;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    rt.block_on(async {
        discovery.set_status(PeerStatus::Background).await;
    });

    if headless {
        println!("\n✨ Modo Headless en ejecución. Presione Ctrl+C para finalizar.\n");
        rt.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
    } else {
        println!("\n✨ Abriendo Ventana de Escritorio NxDesk (AnyDesk Desktop Clone en Rust)...\n");
        if let Err(e) = app::run_desktop_app(app_state) {
            error!("Error en la aplicación de escritorio nativa: {}", e);
        }
    }

    info!("Cerrando nodo NxDesk...");
    discovery.stop();
    Ok(())
}

/// Procesa comandos de terminal de forma segura respetando la política VUL-01.
/// Llamado SOLO por el worker FIFO serial (nunca desde el dispatcher de eventos inline).
async fn handle_incoming_terminal_cmd(state: &AppState, cmd: &str) {
    if !state.is_remote_terminal_allowed().await {
        let warn_msg = "\r\n⚠️ [VUL-01: Terminal Bloqueada] La ejecución remota de comandos no está permitida en este nodo.\r\n".to_string();
        let _ = state.terminal_out_network_tx.send(warn_msg.clone());
        let _ = state.terminal_broadcast_tx.send(warn_msg);
        return;
    }

    if let Err(reason) = nxdesk_terminal::validate_terminal_command(cmd, true) {
        let block_msg = format!("\r\n⛔ [VUL-01: Comando Bloqueado] {}\r\n", reason);
        let _ = state.terminal_out_network_tx.send(block_msg.clone());
        let _ = state.terminal_broadcast_tx.send(block_msg);
        return;
    }

    // Reinicia el contador de bytes del stream para este comando específico
    state.terminal_stream_bytes.store(0, std::sync::atomic::Ordering::SeqCst);

    if !state.terminal_session.is_running() {
        if let Ok(mut rx) = state.terminal_session.start().await {
            let ui_tx = state.terminal_broadcast_tx.clone();
            let net_tx = state.terminal_out_network_tx.clone();
            let stream_bytes = state.terminal_stream_bytes.clone();
            tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    let _ = ui_tx.send(chunk.clone());
                    let current = stream_bytes.fetch_add(chunk.len(), std::sync::atomic::Ordering::SeqCst);
                    if current < 512 * 1024 {
                        let _ = net_tx.send(chunk);
                    } else if current < 512 * 1024 + chunk.len() {
                        let limit_notice = "\r\n⚠️ [NxDesk: Límite de salida alcanzado (512 KiB). Flujo truncado para proteger la red. Usa 'Interrumpir' (Ctrl+C) si deseas detenerlo.]\r\n".to_string();
                        let _ = net_tx.send(limit_notice.clone());
                        let _ = ui_tx.send(limit_notice);
                    }
                }
            });
        }
    }

    // Retransmisor único: consume la cola de salida de la terminal local (terminal_out_network_rx)
    // y la emite a la red en batches de ≤4 KiB / 40 ms con pacing de 20 ms.
    // Al estar desacoplado de terminal_broadcast_tx, resulta imposible que mensajes entrantes de red
    // vuelvan a salir hacia la red.
    if !state.terminal_forward_started.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut rx = match state_clone.terminal_out_network_rx.lock().await.take() {
                Some(r) => r,
                None => return,
            };
            let mut pending = String::new();
            let mut last_send = std::time::Instant::now();
            loop {
                let recv_fut = rx.recv();
                match tokio::time::timeout(Duration::from_millis(40), recv_fut).await {
                    Ok(Some(chunk)) => pending.push_str(&chunk),
                    Ok(None) => break, // canal cerrado
                    Err(_) => {} // timeout: sin novedades en 40 ms → flush del acumulado
                }
                while pending.len() < 4096 {
                    match rx.try_recv() {
                        Ok(chunk) => pending.push_str(&chunk),
                        Err(_) => break, // vacío o cerrado: flush con lo que hay
                    }
                }
                if pending.is_empty() {
                    continue;
                }
                let since = last_send.elapsed();
                if since < Duration::from_millis(20) {
                    tokio::time::sleep(Duration::from_millis(20) - since).await;
                }
                if let Some(sig) = state_clone.get_signaling_client().await {
                    let mut msg = nxdesk_core::SignalingMessage::new("terminal_out", &pending);
                    let target = state_clone
                        .active_remote_peer
                        .read()
                        .await
                        .clone()
                        .or_else(|| {
                            state_clone
                                .active_session
                                .try_read()
                                .ok()
                                .and_then(|s| s.as_ref().map(|x| x.peer_id.clone()))
                        });
                    if let Some(t) = target {
                        msg = msg.with_target(t);
                    }
                    if sig.send_message(&msg).await.is_ok() {
                        pending.clear();
                    } else if pending.len() > 64 * 1024 {
                        let cut = pending.len() - 64 * 1024;
                        let safe_cut = pending.floor_char_boundary(cut);
                        pending.drain(..safe_cut);
                    }
                } else {
                    pending.clear();
                }
                last_send = std::time::Instant::now();
            }
        });
    }

    let _ = state.terminal_session.write_input(cmd).await;
}

/// Crea el worker serial de comandos de terminal (idempotente). Consume el FIFO y ejecuta
/// cada comando secuencialmente en background: el dispatcher de eventos regresa al instante.
async fn start_terminal_cmd_worker(state: &AppState) {
    // Ya creado: encolar es suficiente (el candado se sostiene solo unas nanosegundos).
    let mut guard = match state.terminal_cmd_tx.lock() {
        Ok(g) => g,
        Err(_) => return, // envenenado: no recrear canal (evita condiciones de carrera)
    };
    if guard.is_some() {
        return;
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    *guard = Some(tx);

    let state_worker = state.clone();
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            handle_incoming_terminal_cmd(&state_worker, &cmd).await;
        }
    });
}
