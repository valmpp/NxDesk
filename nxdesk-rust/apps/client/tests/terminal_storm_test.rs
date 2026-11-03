//! Tormenta de terminal bajo video 30 FPS — valida que la salida de terminal AGRUPADA
//! (batch de 4 KiB @ ≤50 msg/s, el calendario exacto que emite el retransmisor con
//! coalescing de 40 ms/4 KiB + pacing 20 ms de main.rs) no degrada el video ni el RTT.
//!
//! Regresión que cubre: antes del batching, un `dir /s` producía mensajes de ~1 KB a
//! cientos msg/s; el `select! biased` del escritor priorizaba control y el video caía
//! a <10 FPS con RTT >1000 ms.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tormenta_terminal_agrupada_mantiene_video_y_rtt() {
    let server_bin = server_binary_path();
    assert!(
        server_bin.exists(),
        "Binario del servidor no encontrado en {:?}. Ejecuta: cargo build -p nxdesk-server",
        server_bin
    );

    let mut child = std::process::Command::new(&server_bin)
        .args(["--bind", "127.0.0.1:50997"])
        .env("RUST_LOG", "error")
        .spawn()
        .expect("No se pudo iniciar el binario nxdesk-server");
    tokio::time::sleep(Duration::from_millis(800)).await;

    let result = run_storm().await;
    let _ = child.kill();
    let _ = child.wait();

    let (max_rtt, max_video_lag, video_frames) = result.unwrap_or_else(|e| panic!("{}", e));
    println!(
        "📊 RTT máx. con tormenta de terminal (ms): {}\n📊 Lag máx. de video (ms): {}\n📊 Frames de video recibidos: {}",
        max_rtt, max_video_lag, video_frames
    );

    // Con la salida AGRUPADA: el RTT del control y el tránsito del video se mantienen en
    // orden de decenas de ms en localhost, NO >1000 ms como con mensajes de 1 KB sin batch.
    assert!(
        max_rtt < 300,
        "Durante la tormenta de terminal el RTT NO debe superar 300 ms. Máximo: {}",
        max_rtt
    );
    assert!(
        max_video_lag < 250,
        "Durante la tormenta de terminal el frame más fresco NO debe llevar >250 ms en tránsito. Máximo: {}",
        max_video_lag
    );
}

async fn run_storm() -> Result<(u64, u64, usize), String> {
    let url = "ws://127.0.0.1:50997/signalinghub";
    let (a, mut a_ev) = SignalingClient::connect(url).await.map_err(|e| e.to_string())?;
    let (b, mut b_ev) = SignalingClient::connect(url).await.map_err(|e| e.to_string())?;
    let a = Arc::new(a);
    let b = Arc::new(b);

    // B replica el pong_rtt (igual que la app real)
    let responder = {
        let b = b.clone();
        tokio::spawn(async move {
            while let Some(ev) = b_ev.recv().await {
                if let SignalingEvent::Message(msg) = ev {
                    if msg.msg_type == "ping_rtt" {
                        let _ = b.send_message(&SignalingMessage::new("pong_rtt", &msg.payload)).await;
                    }
                    // terminal_out llega a B: se descarta aquí (solo mide video)
                }
            }
        })
    };

    a.join_room("terminal-storm-room").await.map_err(|e| e.to_string())?;
    b.join_room("terminal-storm-room").await.map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut video_rx = b.subscribe_incoming_video();

    let max_rtt = Arc::new(AtomicU64::new(0));
    let max_lag = Arc::new(AtomicU64::new(0));

    // === Hilo de RTT: ping cada 100 ms midiendo eco (idéntico al HUD) ===
    let rtt_task = {
        let a = a.clone();
        let max_rtt = max_rtt.clone();
        tokio::spawn(async move {
            for _ in 0..20 {
                let sent = now_ms();
                if a.send_message(&SignalingMessage::new("ping_rtt", &sent.to_string())).await.is_err() {
                    break;
                }
                // Espera el pong drenando a_ev (ignora otros eventos de sala)
                let got = loop {
                    match tokio::time::timeout(Duration::from_millis(2000), a_ev.recv()).await {
                        Ok(Some(SignalingEvent::Message(m))) if m.msg_type == "pong_rtt" => {
                            break m.payload.trim().parse::<u64>().ok()
                        }
                        Ok(Some(_)) => continue,
                        Ok(None) => break None,
                        Err(_) => break None,
                    }
                };
                if let Some(pong_sent_ts) = got {
                    max_rtt.fetch_max(now_ms().saturating_sub(pong_sent_ts), Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
        })
    };

    // === Hilo de TORMENTA DE TERMINAL: emite EXACTAMENTE el calendario agrupado ===
    // 4 KiB por mensaje, pacing 20 ms (50 msg/s) durante 1.8 s = 360 KiB, el volumen
    // equivalente a la salida de un `dir /s` pero con la coalescing de la app corregida.
    let storm_task = {
        let a = a.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let chunk = "x".repeat(4096);
            let t0 = Instant::now();
            let mut n = 0;
            while t0.elapsed() < Duration::from_millis(1800) {
                let _ = a
                    .send_message(&SignalingMessage::new("terminal_out", &chunk))
                    .await;
                n += 1;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            n
        })
    };

    // === Hilo de VIDEO 30 FPS con timestamp big-endian (96 KiB, idéntico al pipeline real) ===
    let video_task = {
        let a = a.clone();
        tokio::spawn(async move {
            let payload = vec![0u8; 96 * 1024];
            let interval = Duration::from_millis(1000 / 30);
            for _ in 0..54 {
                let mut frame = payload.clone();
                frame[..8].copy_from_slice(&now_ms().to_be_bytes());
                a.send_video_frame_direct(frame);
                tokio::time::sleep(interval).await;
            }
        })
    };

    // Receptor: mide lag de llegada del frame más fresco mientras la tormenta corre
    let mut received: usize = 0;
    let t_end = Instant::now() + Duration::from_millis(3200);
    loop {
        if Instant::now() >= t_end {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(200), video_rx.changed()).await {
            Ok(Ok(())) => {
                if let Some(frame) = video_rx.borrow_and_update().clone() {
                    if frame.len() >= 8 {
                        let sent_ts = u64::from_be_bytes(frame[..8].try_into().unwrap());
                        max_lag.fetch_max(now_ms().saturating_sub(sent_ts), Ordering::Relaxed);
                        received += 1;
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => {}
        }
    }
    let storm_msgs = storm_task.await.map_err(|e| e.to_string())?;
    video_task.abort();
    let _ = video_task.await;
    // Última medición de RTT en curso antes de terminar
    rtt_task.await.map_err(|e| e.to_string())?;
    responder.abort();

    println!("📊 Mensajes terminal_out agrupados enviados: {}", storm_msgs);
    assert!(storm_msgs >= 50, "Debe emitirse una tormenta real de ≥50 mensajes");

    Ok((
        max_rtt.load(Ordering::Relaxed),
        max_lag.load(Ordering::Relaxed),
        received,
    ))
}
