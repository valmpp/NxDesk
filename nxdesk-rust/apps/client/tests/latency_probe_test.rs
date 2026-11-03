//! Pruebas de latencia con mediciones reales sobre el stack completo:
//! binario real de `nxdesk-server` + 2 clientes con buffers de kernel acotados (192 KiB).
//!
//! Presupuestos validados:
//!   - RTT del plano de control (ping_rtt → pong_rtt, igual que el HUD): < 60 ms en localhost.
//!   - Latencia de llegada del stream de video a 30 FPS (frames ~96 KiB): el frame MÁS FRESCO
//!     nunca lleva más de 120 ms en tránsito (los buffers acotados no permiten backlog).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nxdesk_core::SignalingMessage;
use nxdesk_signaling::{SignalingClient, SignalingEvent};

fn server_binary_path() -> std::path::PathBuf {
    let exe_name = if cfg!(windows) { "nxdesk-server.exe" } else { "nxdesk-server" };
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join(profile)
        .join(exe_name)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rtt_y_pipeline_video_mantienen_presupuesto() {
    // 0. El binario del servidor debe existir (verificación reproducible)
    let server_bin = server_binary_path();
    assert!(
        server_bin.exists(),
        "Binario del servidor no encontrado en {:?}. Ejecuta: cargo build -p nxdesk-server",
        server_bin
    );

    // 1. Levantar el servidor de señalización REAL
    let mut child = std::process::Command::new(&server_bin)
        .args(["--bind", "127.0.0.1:50991"])
        .env("RUST_LOG", "error")
        .spawn()
        .expect("No se pudo iniciar el binario nxdesk-server");
    tokio::time::sleep(Duration::from_millis(800)).await;

    let result = run_measurements().await;

    let _ = child.kill();
    let _ = child.wait();

    let (rtts, max_video_lag, discards) = result.unwrap_or_else(|e| panic!("{}", e));
    println!("📊 RTT de control (ms): {:?}", rtts);
    println!("📊 Latencia máx. de llegada del video a 30 FPS (ms): {}", max_video_lag);
    println!("📊 Frames descartados por el watch (anti-bufferbloat): {}/60", discards);

    assert!(
        rtts.iter().all(|r| *r < 60),
        "El RTT del plano de control debe ser < 60 ms en localhost. Medido: {:?}",
        rtts
    );
    assert!(
        max_video_lag < 120,
        "El frame más fresco no debe llevar más de 120 ms en tránsito. Máximo: {}",
        max_video_lag
    );
}

async fn run_measurements() -> Result<(Vec<u64>, u64, usize), String> {
    let url = "ws://127.0.0.1:50991/signalinghub";

    // 2. Dos clientes reales con el MISMO camino acotado que la app (buffers 192 KiB en connect())
    let (a, mut a_ev) = SignalingClient::connect(url).await.map_err(|e| e.to_string())?;
    let (b, mut b_ev) = SignalingClient::connect(url).await.map_err(|e| e.to_string())?;
    let a = Arc::new(a);
    let b = Arc::new(b);

    // Responder ping_rtt en B, igual que la lógica real de la app (main.rs "ping_rtt" => pong_rtt)
    let responder = {
        let b = b.clone();
        tokio::spawn(async move {
            while let Some(ev) = b_ev.recv().await {
                if let SignalingEvent::Message(msg) = ev {
                    if msg.msg_type == "ping_rtt" {
                        let _ = b.send_message(&SignalingMessage::new("pong_rtt", &msg.payload)).await;
                    }
                }
            }
        })
    };

    a.join_room("rtt-probe-room").await.map_err(|e| e.to_string())?;
    b.join_room("rtt-probe-room").await.map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(300)).await; // ambos ya se conocen en la sala

    // Preparar la suscripción de video de B ANTES de inyectar frames
    let mut video_rx = b.subscribe_incoming_video();

    // === Medición 1: RTT del plano de control (ping_rtt → pong_rtt, idéntico al HUD) ===
    // A envía ping con timestamp; B responde pong replicando el payload; A calcula RTT.
    let mut rtts: Vec<u64> = Vec::new();
    for _ in 0..5 {
        let sent_ms = now_ms();
        a.send_message(&SignalingMessage::new("ping_rtt", &sent_ms.to_string()))
            .await
            .map_err(|e| e.to_string())?;

        // Esperar el eco pong_rtt de vuelta en A
        let pong_sent_ts = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match tokio::time::timeout(Duration::from_millis(1500), a_ev.recv()).await {
                    Ok(Some(SignalingEvent::Message(msg))) => {
                        if msg.msg_type == "pong_rtt" {
                            return Ok(msg);
                        }
                        // ignora otros eventos (RoomOccupancy, etc.)
                    }
                    Ok(Some(_)) => continue,
                    Ok(None) => return Err("canal de eventos de A cerrado".to_string()),
                    Err(_) => return Err("timeout esperando pong_rtt".to_string()),
                }
            }
        })
        .await
        .map_err(|_: tokio::time::error::Elapsed| "timeout externo del RTT".to_string())?
        .and_then(|m: SignalingMessage| {
            m.payload.trim().parse::<u64>().map_err(|_| "payload de pong_rtt no numérico".to_string())
        })?;

        rtts.push(now_ms().saturating_sub(pong_sent_ts));
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    responder.abort();

    // === Medición 2: latencia de llegada del pipeline de video a 30 FPS real ===
    // A envía 60 frames de ~96 KiB con timestamp big-endian en bytes[0..8]; B mide
    // (ahora − enviado) del ÚLTIMO frame vía watch::borrow_and_update (solo lo fresco).
    const FRAMES: usize = 60;
    let interval = Duration::from_millis(1000 / 30); // 30 Hz exactos

    let sender = {
        let a = a.clone();
        tokio::spawn(async move {
            let payload = vec![0u8; 96 * 1024];
            for _ in 0..FRAMES {
                // Timestamp en el INSTANTE REAL del envío (no programado: si el ritmo del
                // emisor varía, ese desfase no debe contaminar la medición de tránsito).
                let mut frame = payload.clone();
                frame[..8].copy_from_slice(&now_ms().to_be_bytes());
                a.send_video_frame_direct(frame);
                tokio::time::sleep(interval).await;
            }
            FRAMES
        })
    };

    // Receptor: consume el watch en cuanto hay frame fresco y mide arrival lag
    let mut max_lag: u64 = 0;
    let mut received: usize = 0;
    loop {
        let changed = tokio::time::timeout(Duration::from_millis(500), video_rx.changed()).await;
        match changed {
            Ok(Ok(())) => {
                if let Some(frame) = video_rx.borrow_and_update().clone() {
                    if frame.len() >= 8 {
                        let sent_ts = u64::from_be_bytes(frame[..8].try_into().unwrap());
                        let lag = now_ms().saturating_sub(sent_ts);
                        if lag > max_lag {
                            max_lag = lag;
                        }
                        received += 1;
                    }
                }
            }
            Ok(Err(_)) => return Err("canal de video cerrado inesperadamente".to_string()),
            Err(_) => break, // 500 ms sin frames nuevos → el envío terminó
        }
        if sender.is_finished() && video_rx.has_changed().map(|c| !c).unwrap_or(true) {
            break;
        }
    }
    let sent_total = sender.await.map_err(|e| e.to_string())?;
    let discards = sent_total.saturating_sub(received);

    Ok((rtts, max_lag, discards))
}
