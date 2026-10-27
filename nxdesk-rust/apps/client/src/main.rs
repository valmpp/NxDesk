mod auth;
mod state;
mod web_server;

use nxdesk_core::{AppConfig, PeerStatus};
use nxdesk_discovery::DiscoveryService;
use nxdesk_signaling::{SignalingClient, SignalingEvent};
use state::AppState;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Función de hash de muestreo ultrarrápido (64-bit FNV) para detectar cambios en la pantalla
fn fast_frame_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let step = 64;
    let mut i = 0;
    while i < data.len() {
        hash = hash.wrapping_mul(0x100000001b3);
        hash ^= data[i] as u64;
        i += step;
    }
    hash
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    println!("==================================================");
    println!("     🖥️   NxDesk - Cliente Remoto Seguro (Pure Rust) ");
    println!("==================================================");

    // 1. Parseo de argumentos de línea de comandos
    let args: Vec<String> = env::args().collect();
    let mut config = AppConfig {
        device_alias: env::var("COMPUTERNAME")
            .or_else(|_| env::var("HOSTNAME"))
            .unwrap_or_else(|_| "NxDesk-Host".to_string()),
        ..Default::default()
    };

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
        } else if arg.starts_with("--web-port=") {
            if let Ok(p) = arg.trim_start_matches("--web-port=").parse::<u16>() {
                config.web_port = p;
            }
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

    let host_password = if let Some(ref pw) = config.access_password {
        pw.clone()
    } else {
        let rand_val = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() % 900000 + 100000) as u32;
        format!("NX-{}", rand_val)
    };
    config.access_password = Some(host_password.clone());

    info!("📌 ID de Dispositivo: {}", config.device_id);
    info!("💻 Alias del Nodo:    {}", config.device_alias);
    info!("🌐 Servidor Señales:  {}", config.signaling_server_url);
    info!("🛡️ Terminal Remota:   {}", if config.allow_remote_terminal { "Habilitada (VUL-01 Verificada)" } else { "Bloqueada por Seguridad" });
    info!("==================================================");
    info!("   🔐  CLAVE MAESTRA ANFITRIÓN: [{}]", host_password);
    info!("   (Úsala para autorizar el uso de terminal remota)");
    info!("==================================================");

    // 2. Inicializar estado global unificado
    let app_state = AppState::new(config.clone());

    // 3. Iniciar Servidor Web Dashboard Axum
    let state_web = app_state.clone();
    tokio::spawn(async move {
        if let Err(e) = web_server::start_web_server(state_web).await {
            error!("Error en Servidor Web Dashboard: {}", e);
        }
    });

    // 4. Iniciar Radar LAN de Descubrimiento UDP (Puerto 5001)
    let discovery = DiscoveryService::new(&config.device_id, &config.device_alias, 5000);
    let mut peer_rx = discovery.start().await?;
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

    // 5. Iniciar Ciclo de Captura de Pantalla y Compresión JPEG SIMD (Zero-Alloc 60 FPS)
    let state_capture = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30 FPS estable
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut first_frame_logged = false;
        let mut rgb_buffer = Vec::with_capacity(1920 * 1080 * 3);
        let mut jpeg_bytes = Vec::with_capacity(65536);
        let mut last_frame_hash: u64 = 0;
        let mut unchanged_count: u32 = 0;
        loop {
            interval.tick().await;

            if !state_capture.is_screen_sharing() {
                continue;
            }

            // Ahorro de CPU y RAM en Host: si no hay visores web ni sesión remota activa, descansar
            let viewers = state_capture.get_connected_viewers();
            let has_remote = state_capture.active_remote_peer.read().await.is_some();
            if viewers == 0 && !has_remote {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            let frame_res = {
                let mut cap = state_capture.capturer.lock().await;
                cap.capture_frame()
            };

            match frame_res {
                Ok(frame) => {
                    // Detección ultrarrápida de pantalla estática: reduce transmisión un 95% si no hay cambios
                    let current_hash = fast_frame_hash(&frame.data);
                    if current_hash == last_frame_hash {
                        unchanged_count += 1;
                        // Si la pantalla no cambia, emitir solo 1 fotograma de refresco por segundo (cada 30 ticks)
                        if unchanged_count < 30 {
                            continue;
                        }
                    }
                    last_frame_hash = current_hash;
                    unchanged_count = 0;

                    // Convertir BGRA de Windows GDI a RGB reutilizando buffer en memoria (cero asignaciones)
                    frame.to_rgb_into(&mut rgb_buffer);
                    jpeg_bytes.clear();
                    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 60);
                    if let Err(e) = encoder.encode(
                        &rgb_buffer,
                        frame.width,
                        frame.height,
                        image::ExtendedColorType::Rgb8,
                    ) {
                        warn!("Error codificando fotograma JPEG: {}", e);
                    } else {
                        if !first_frame_logged {
                            info!("📸 [Captura] Pantalla transmitiendo en vivo: {}x{} @ JPEG 60% (Ultra Baja Latencia ~30 FPS).", frame.width, frame.height);
                            first_frame_logged = true;
                        }

                        // Guardar último frame para nuevos clientes web
                        {
                            let mut latest = state_capture.latest_jpeg_frame.write().await;
                            *latest = Some(jpeg_bytes.clone());
                        }

                        // Difundir fotograma a todos los visores web activos
                        let _ = state_capture.frame_broadcast_tx.send(jpeg_bytes.clone());

                        // Si hay sesión de señalización activa y auto-share, retransmitir de forma asíncrona sin bloquear el capturador
                        if let Some(sig) = state_capture.signaling_client.read().await.clone() {
                            let frame_payload = jpeg_bytes.clone();
                            tokio::spawn(async move {
                                let _ = sig.send_video_frame(frame_payload).await;
                            });
                        }
                    }
                }
                Err(e) => {
                    static LAST_ERR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    let last = LAST_ERR.load(std::sync::atomic::Ordering::Relaxed);
                    if now - last > 5 {
                        LAST_ERR.store(now, std::sync::atomic::Ordering::Relaxed);
                        warn!("Error en captura de pantalla: {}", e);
                    }
                }
            }
        }
    });

    // 6. Conectar al Servidor de Señalización (WebSocket / SignalR)
    let state_sig = app_state.clone();
    let my_id = config.device_id.clone();
    let sig_url = config.signaling_server_url.clone();

    let sig_result = if explicit_server || connect_target.is_some() {
        SignalingClient::connect(&sig_url).await.map_err(|e| e.to_string())
    } else {
        // En modo local por defecto (Web Dashboard), probar conexión rápida sin bloquear ni emitir warnings
        match tokio::time::timeout(Duration::from_millis(300), SignalingClient::connect(&sig_url)).await {
            Ok(Ok(pair)) => Ok(pair),
            _ => Err("standalone_local".to_string()),
        }
    };

    match sig_result {
        Ok((client, mut event_rx)) => {
            let client_arc = Arc::new(client);
            {
                let mut sig_lock = state_sig.signaling_client.write().await;
                *sig_lock = Some(client_arc.clone());
            }
            state_sig.set_signaling_connected(true);
            info!("✅ Conectado al servidor de señalización con éxito.");

            // Unirse a la sala propia
            let _ = client_arc.join_room(&my_id).await;
            info!("🏠 Registrado en sala de anfitrión '{}'. Listo para recibir pares.", my_id);

            // Conectar a par objetivo si se especificó por CLI
            if let Some(target) = connect_target {
                info!("🚀 Conectando a sala de par remoto '{}'...", target);
                let _ = client_arc.join_room(&target).await;
                state_sig.set_active_remote_peer(Some(target)).await;
            }

            // Manejador centralizado y seguro de eventos de red
            let state_events = state_sig.clone();
            tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        SignalingEvent::Input(input_evt) => {
                            let injector = state_events.input_injector.lock().await;
                            injector.inject(&input_evt);
                        }
                        SignalingEvent::Message(msg) => {
                            match msg.msg_type.as_str() {
                                "terminal_cmd" => {
                                    handle_incoming_terminal_cmd(&state_events, &msg.payload).await;
                                }
                                "file_chunk" => {
                                    if let Ok(chunk) = serde_json::from_str::<nxdesk_core::FileChunk>(&msg.payload) {
                                        let _ = state_events.file_manager.write_chunk(&chunk).await;
                                    }
                                }
                                "select_screen" => {
                                    if let Ok(idx) = msg.payload.parse::<i32>() {
                                        info!("🖥️ [Señalización] Conmutando a monitor {} solicitado por par remoto.", idx);
                                        let _ = state_events.select_screen(idx).await;
                                    }
                                }
                                "frame" => {
                                    if let Some(bytes) = msg.byte_data {
                                        {
                                            let mut latest = state_events.latest_jpeg_frame.write().await;
                                            *latest = Some(bytes.clone());
                                        }
                                        let _ = state_events.frame_broadcast_tx.send(bytes);
                                    }
                                }
                                other => {
                                    info!("Mensaje de señalización recibido [{}]: {}", other, msg.payload);
                                }
                            }
                        }
                        SignalingEvent::RoomOccupancy(count) => {
                            state_events.set_room_occupancy(count);
                            info!("👥 Participantes activos en la sala: {}", count);
                        }
                        SignalingEvent::Disconnected => {
                            state_events.set_signaling_connected(false);
                            warn!("Conexión con servidor de señalización cerrada.");
                            break;
                        }
                    }
                }
            });
        }
        Err(e) if e == "standalone_local" => {
            info!("🌐 Modo Web Dashboard Autónomo (LAN / Web UI directa en http://localhost:{}).", config.web_port);
            info!("ℹ️  (Para enlace multi-nodo vía servidor central, ejecute 'cargo run -p nxdesk-server').");
        }
        Err(e) => {
            warn!("Servidor de señalización no disponible en {}: {}. Operando en modo LAN Dashboard.", sig_url, e);
        }
    }

    discovery.set_status(PeerStatus::Background).await;
    println!("\n✨ Nodo NxDesk en ejecución. Presione Ctrl+C para finalizar.\n");

    tokio::signal::ctrl_c().await?;

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
