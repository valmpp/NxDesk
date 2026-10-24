use nxdesk_capture::ScreenCapturer;
use nxdesk_core::PeerStatus;
use nxdesk_discovery::DiscoveryService;
use nxdesk_input::InputInjector;
use nxdesk_signaling::{SignalingClient, SignalingEvent};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    println!("==================================================");
    println!("        🖥️   NxDesk - Cliente Remoto (Rust)        ");
    println!("==================================================");

    // Argumentos de línea de comandos (compatibles con CLI de C#)
    // --id=111-222-333 | --share | --connect 123-456-789 | --server ws://...
    let args: Vec<String> = env::args().collect();
    let mut custom_id = None;
    let mut auto_share = false;
    let mut connect_target = None;
    let mut server_url = "ws://127.0.0.1:5000/signalinghub".to_string();

    let mut i = 1;
    while i < args.len() {
        if args[i].starts_with("--id=") {
            custom_id = Some(args[i].trim_start_matches("--id=").to_string());
        } else if args[i] == "--share" {
            auto_share = true;
        } else if args[i] == "--connect" && i + 1 < args.len() {
            connect_target = Some(args[i + 1].clone());
            i += 1;
        } else if args[i] == "--server" && i + 1 < args.len() {
            server_url = args[i + 1].clone();
            i += 1;
        }
        i += 1;
    }

    let my_id = custom_id.unwrap_or_else(|| "111-222-333".to_string());
    let my_alias = env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "NxDesk-Rust-Node".to_string());

    info!("📌 ID Asignado: {}", my_id);
    info!("💻 Nombre de Equipo: {}", my_alias);
    info!("🌐 Servidor de Señalización: {}", server_url);

    // 1. Iniciar Descubrimiento Local LAN (Puerto UDP 5001)
    let discovery = DiscoveryService::new(&my_id, &my_alias, 5000);
    let mut peer_rx = discovery.start().await?;
    info!("📡 Radar LAN iniciado en puerto UDP 5001.");

    // Tarea en segundo plano para escuchar equipos en LAN
    tokio::spawn(async move {
        while let Some(peer) = peer_rx.recv().await {
            info!(
                "🔍 [LAN Encontrado] '{}' (ID: {}) en {}:{} - Estado: {:?}",
                peer.alias, peer.id, peer.ip_address, peer.port, peer.status
            );
        }
    });

    // 2. Iniciar inyector nativo de ratón y teclado
    let input_injector = Arc::new(Mutex::new(InputInjector::new()));

    // 3. Iniciar capturador de pantalla
    let capturer = Arc::new(Mutex::new(ScreenCapturer::new()));
    {
        let cap = capturer.lock().await;
        let (w, h) = cap.dimensions();
        input_injector.lock().await.set_screen_bounds(0, 0, w as i32, h as i32);
        info!("🖥️ Pantalla principal detectada: {}x{} píxeles.", w, h);
    }

    // 4. Conexión al servidor de señalización (WebSocket)
    info!("Conectando a servidor de señalización...");
    match SignalingClient::connect(&server_url).await {
        Ok((client, mut event_rx)) => {
            let client = Arc::new(client);
            info!("✅ Conectado al servidor de señalización con éxito.");

            // Registrarse en la sala propia (para recibir conexiones entrantes)
            client.join_room(&my_id).await?;
            info!("🏠 Registrado en sala de anfitrión '{}'. Esperando clientes...", my_id);

            // Si se especificó conectarse a un host remoto
            if let Some(target) = connect_target {
                info!("🚀 Iniciando conexión hacia el equipo remoto '{}'...", target);
                client.join_room(&target).await?;
            }

            // Manejador de eventos entrantes de la red
            let injector_clone = Arc::clone(&input_injector);
            tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        SignalingEvent::Input(input_evt) => {
                            let injector = injector_clone.lock().await;
                            injector.inject(&input_evt);
                        }
                        SignalingEvent::Message(msg) => {
                            match msg.msg_type.as_str() {
                                "terminal_cmd" => {
                                    handle_remote_terminal_cmd(&msg.payload);
                                }
                                "file_request" => {
                                    info!("Recibida solicitud de transferencia de archivo: {}", msg.payload);
                                }
                                "frame" => {
                                    if let Some(bytes) = msg.byte_data {
                                        info!("🖼️ Fotograma recibido: {} bytes", bytes.len());
                                    }
                                }
                                other => {
                                    info!("Mensaje recibido [{}]: {}", other, msg.payload);
                                }
                            }
                        }
                        SignalingEvent::RoomOccupancy(count) => {
                            info!("👥 Participantes en la sesión: {}", count);
                        }
                        SignalingEvent::Disconnected => {
                            warn!("Conexión de señalización cerrada.");
                            break;
                        }
                    }
                }
            });

            // Si está en modo compartir (--share), transmitir pantalla a 30 FPS
            if auto_share {
                info!("▶️ Transmisión de pantalla activada (--share).");
                let client_share = Arc::clone(&client);
                let capturer_share = Arc::clone(&capturer);
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30 fps
                    loop {
                        interval.tick().await;
                        let frame_opt = {
                            let mut cap = capturer_share.lock().await;
                            cap.capture_frame().ok()
                        };

                        if let Some(frame) = frame_opt {
                            let _ = client_share.send_video_frame(frame.data).await;
                        }
                    }
                });
            }
        }
        Err(e) => {
            warn!("No se pudo conectar al servidor de señalización: {}. Operando en modo LAN offline.", e);
        }
    }

    discovery.set_status(PeerStatus::Background).await;
    println!("\n[NxDesk Rust Node en ejecución. Presione Ctrl+C para salir]\n");

    tokio::signal::ctrl_c().await?;

    info!("Cerrando nodo...");
    discovery.stop();
    Ok(())
}

/// Ejecuta comandos de terminal de forma segura
fn handle_remote_terminal_cmd(cmd: &str) {
    info!("💻 Comando remoto recibido: {}", cmd);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", cmd])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", cmd])
            .spawn();
    }
}
