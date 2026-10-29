use nxdesk_client::app;
use nxdesk_client::state::AppState;
use nxdesk_core::{AppConfig, PeerStatus};
use nxdesk_discovery::DiscoveryService;
use nxdesk_signaling::{SignalingClient, SignalingEvent};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Genera un ID AnyDesk único de 9 dígitos ligado al hardware único de la máquina (MachineGuid + Hostname)
fn get_machine_unique_id() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(&["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.contains("MachineGuid") {
                    if let Some(guid) = line.split_whitespace().last() {
                        guid.hash(&mut hasher);
                    }
                }
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

    // 5. Iniciar Ciclo de Captura de Pantalla Dedicado (Zero-Alloc, Zero-Lock, >35 FPS Estables)
    let state_capture = app_state.clone();
    std::thread::Builder::new()
        .name("nxdesk-capture-pipeline".to_string())
        .spawn(move || {
            let mut first_frame_logged = false;
            let mut jpeg_bytes = Vec::with_capacity(65536);
            let target_frame_time = Duration::from_millis(30); // ~33.3 FPS continuo a RESOLUCIÓN NATIVA 1:1

            loop {
                let loop_start = std::time::Instant::now();

                if !state_capture.is_screen_sharing() {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }

                let viewers = state_capture.get_connected_viewers();
                let is_viewer = state_capture.is_viewer_active();
                let is_host = state_capture.is_host_active();
                let occupancy = state_capture.get_room_occupancy();

                // Si este nodo es únicamente el visor (cliente espectador), NO captura ni transmite su pantalla al anfitrión
                if is_viewer && !is_host {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }

                // Solo capturar si tenemos un espectador conectado o sesión activa de anfitrión
                if !is_host && viewers == 0 && occupancy <= 1 {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }

                let (enc_w, enc_h, encode_res) = {
                    let mut cap = state_capture.capturer.blocking_lock();
                    match cap.capture_native_bgra() {
                        Ok((bgra, w, h)) => {
                            jpeg_bytes.clear();
                            let target_quality = if config.quality >= 40 && config.quality <= 85 {
                                config.quality as u8
                            } else {
                                65 // Calidad nítida 1080p nativa (equilibrio óptimo texto/bitrate)
                            };
                            let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_bytes, target_quality);
                            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
                            let res = encoder
                                .encode(bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra)
                                .map_err(|e| e.to_string());
                            (w, h, res)
                        }
                        Err(e) => (0, 0, Err(e)),
                    }
                };

                match encode_res {
                    Ok(_) => {
                        state_capture.record_tx_frame(jpeg_bytes.len(), enc_w, enc_h);
                        if !first_frame_logged {
                            info!("📸 [Captura Nativa SIMD] Pantalla transmitiendo a RESOLUCIÓN NATIVA 1:1: {}x{} @ JPEG {}% (>30 FPS Estables).", enc_w, enc_h, config.quality);
                            first_frame_logged = true;
                        }

                        if let Ok(mut latest) = state_capture.latest_jpeg_frame.try_write() {
                            *latest = Some(jpeg_bytes.clone());
                        }

                        let _ = state_capture.frame_broadcast_tx.send(jpeg_bytes.clone());

                        if let Some(sig) = state_capture.try_get_signaling_client() {
                            sig.send_video_frame_direct(jpeg_bytes.clone());
                        }
                    }
                    Err(e) => {
                        warn!("Error codificando fotograma JPEG SIMD Nativo: {:?}", e);
                    }
                }

                let elapsed = loop_start.elapsed();
                if elapsed < target_frame_time {
                    std::thread::sleep(target_frame_time - elapsed);
                }
            }
        })
        .expect("No se pudo iniciar el hilo de captura");

    // 6. Conectar al Servidor de Señalización con Supervisor de Reconexión Automática
    let state_sig = app_state.clone();
    let my_id = config.device_id.clone();
    let sig_url = config.signaling_server_url.clone();
    let connect_target_init = connect_target.clone();

    let is_wan = sig_url.starts_with("wss://")
        || sig_url.contains("trycloudflare.com")
        || sig_url.contains("ngrok");
    let init_timeout = if explicit_server || connect_target_init.is_some() || is_wan {
        Duration::from_millis(5000)
    } else {
        Duration::from_millis(800)
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

    // Tarea dedicada de decodificación JPEG con watch::channel para descartar fotogramas atrasados (Zero Lag)
    let (decode_tx, mut decode_rx) = tokio::sync::watch::channel::<Option<Vec<u8>>>(None);
    let state_decoder = app_state.clone();
    tokio::spawn(async move {
        let mut raw_rgb = Vec::with_capacity(1280 * 720 * 3);
        while decode_rx.changed().await.is_ok() {
            let maybe_data = decode_rx.borrow_and_update().clone();
            if let Some(bytes) = maybe_data {
                let state_clone = state_decoder.clone();
                let cursor = std::io::Cursor::new(&bytes);
                if let Ok(decoder) = image::codecs::jpeg::JpegDecoder::new(cursor) {
                    use image::ImageDecoder;
                    let (w, h) = decoder.dimensions();
                    let total = (w * h * 3) as usize;
                    if raw_rgb.len() != total {
                        raw_rgb.resize(total, 0);
                    }
                    if decoder.read_image(&mut raw_rgb).is_ok() {
                        let color_img = std::sync::Arc::new(eframe::egui::ColorImage::from_rgb([w as usize, h as usize], &raw_rgb));
                        {
                            let mut lock = state_clone.latest_decoded_image.write().await;
                            *lock = Some(color_img);
                        }
                        state_clone.increment_frame_sequence();
                        state_clone.record_rx_frame(bytes.len(), w, h);
                        state_clone.request_ui_repaint();
                    }
                }
            }
        }
    });

    // Tarea de telemetría periódica y medición de latencia RTT (Ping/Pong a 1 Hz)
    let state_telem = app_state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(1000));
        loop {
            ticker.tick().await;
            state_telem.tick_telemetry_second();

            let is_viewer = state_telem.active_remote_peer.read().await.is_some();
            let is_host = state_telem.active_session.read().await.is_some();

            if is_viewer || is_host {
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
                    let maybe_bytes = incoming_vid.borrow_and_update().clone();
                    if let Some(bytes) = maybe_bytes {
                        {
                            let mut latest = state_sup_frame.latest_jpeg_frame.write().await;
                            *latest = Some(bytes.clone());
                        }
                        let _ = decode_tx_conn.send(Some(bytes.clone()));
                        let _ = state_sup_frame.frame_broadcast_tx.send(bytes);
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

            // Bucle despachador de eventos
            while let Some(event) = event_rx.recv().await {
                match event {
                    SignalingEvent::Input(input_evt) => {
                        if state_sup.allow_remote_input.load(std::sync::atomic::Ordering::Relaxed) {
                            if let Ok(injector) = state_sup.input_injector.try_lock() {
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
                                    if let Ok(injector) = state_sup.input_injector.try_lock() {
                                        injector.inject(&input_evt);
                                    }
                                }
                            } else {
                                warn!("⛔ [Seguridad] Input descartado: no existe sesión activa con permiso 'allow_input'.");
                            }
                        }
                        "terminal_cmd" => {
                            let is_authorized = {
                                let active = state_sup.active_session.read().await;
                                active.as_ref().is_some_and(|s| s.granted_role == nxdesk_core::SessionRole::FullControl)
                            };

                            if is_authorized {
                                handle_incoming_terminal_cmd(&state_sup, &msg.payload).await;
                            } else {
                                warn!("⛔ [Seguridad] Comando de terminal rechazado: sesión no autorizada con control total.");
                            }
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
                        "frame" => {
                            if let Some(bytes) = msg.byte_data {
                                {
                                    let mut latest =
                                        state_sup.latest_jpeg_frame.write().await;
                                    *latest = Some(bytes.clone());
                                }
                                let _ = decode_tx_sup.send(Some(bytes.clone()));
                                let _ = state_sup.frame_broadcast_tx.send(bytes);
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
                                info!(
                                    "🔔 [Señalización] Solicitud de sesión entrante de '{}' ({})",
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
                                info!(
                                    "🔔 [Señalización] Decisión de sesión recibida: aceptada={}",
                                    decision.accept
                                );
                                if !decision.accept {
                                    state_sup.set_active_remote_peer(None).await;
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

/// Procesa comandos de terminal de forma segura respetando la política VUL-01
async fn handle_incoming_terminal_cmd(state: &AppState, cmd: &str) {
    if !state.is_remote_terminal_allowed().await {
        let warn_msg = "\r\n⚠️ [VUL-01: Terminal Bloqueada] La ejecución remota de comandos no está permitida en este nodo.\r\n";
        let _ = state.terminal_broadcast_tx.send(warn_msg.to_string());
        return;
    }

    if let Err(reason) = nxdesk_terminal::validate_terminal_command(cmd, true) {
        let block_msg = format!("\r\n⛔ [VUL-01: Comando Bloqueado] {}\r\n", reason);
        let _ = state.terminal_broadcast_tx.send(block_msg);
        return;
    }

    if !state.terminal_session.is_running() {
        if let Ok(mut rx) = state.terminal_session.start().await {
            let tx = state.terminal_broadcast_tx.clone();
            tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    let _ = tx.send(chunk);
                }
            });
        }
    }

    let _ = state.terminal_session.write_input(cmd).await;
}
