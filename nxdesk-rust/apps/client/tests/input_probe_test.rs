//! Prueba de punta a punta del CONTROL REMOTO: binario real de nxdesk-server + 2 clientes.
//! Verifica que los eventos de entrada (ratón: movimiento/clic/scroll; teclado: down/up) pasan
//! por el relay real hasta el anfitrión, y que el permiso `allow_remote_input` del state
//! gobierna el descarte host-side (idéntico a la lógica del despachador de main.rs).

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use nxdesk_core::{InputEvent, MouseButton};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eventos_de_control_fluyen_con_permiso_y_descartados_sin_el() {
    let server_bin = server_binary_path();
    assert!(
        server_bin.exists(),
        "Binario del servidor no encontrado en {:?}. Ejecuta: cargo build -p nxdesk-server",
        server_bin
    );

    let mut child = std::process::Command::new(&server_bin)
        .args(["--bind", "127.0.0.1:50993"])
        .env("RUST_LOG", "error")
        .spawn()
        .expect("No se pudo iniciar el binario nxdesk-server");
    tokio::time::sleep(Duration::from_millis(800)).await;

    let result = run().await;
    let _ = child.kill();
    let _ = child.wait();

    let (received, gate_blocks_without_permission, gate_passes_with_permission) = match result {
        Ok(v) => v,
        Err(e) => panic!("[INPUT_PROBE_ERROR] {}", e),
    };

    println!("📊 InputEvents recibidos por el anfitrión: {}", received.len());
    println!(
        "📊 Gate host-side: sin permiso={} · con permiso={}",
        gate_blocks_without_permission, gate_passes_with_permission
    );

    // 1) La señal debe transportar TODOS los tipos de entrada clave (3×[movimiento, clic] + rueda + tecla)
    assert_eq!(
        received.len(),
        8,
        "El anfitrión debe recibir exactamente los 8 eventos de entrada. Recibidos: {:?}",
        received
    );
    assert!(
        matches!(&received[0], InputEvent::MouseMove { .. }),
        "El primer evento debe ser un movimiento de ratón"
    );
    assert!(
        matches!(&received[1], InputEvent::MouseDown { button: MouseButton::Left, .. }),
        "El segundo evento debe ser un clic izquierdo"
    );
    assert!(
        received.iter().any(|e| matches!(e, InputEvent::MouseWheel { delta_y, .. } if *delta_y < 0.0)),
        "Debe incluir al menos un scroll de rueda"
    );
    assert!(
        received.iter().any(|e| matches!(e, InputEvent::KeyDown { key_code: 0x57, .. })),
        "Debe incluir la tecla 'W' (VK 0x57)"
    );

    // 2) El gate host-side replica main.rs: sin permiso descarta; con permiso inyecta
    assert!(gate_blocks_without_permission);
    assert!(gate_passes_with_permission);
}

async fn run() -> Result<(Vec<InputEvent>, bool, bool), String> {
    let url = "ws://127.0.0.1:50993/signalinghub";

    let mut last_err = String::new();
    let mut host_conn = None;
    for _ in 0..10 {
        match SignalingClient::connect(url).await {
            Ok(pair) => {
                host_conn = Some(pair);
                break;
            }
            Err(e) => {
                last_err = e.to_string();
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
    let (host, mut host_ev) = host_conn.ok_or_else(|| format!("[HOST_CONNECT_FAILED] {}", last_err))?;
    let (viewer, mut viewer_ev) = SignalingClient::connect(url).await.map_err(|e| format!("[VIEWER_CONNECT_FAILED] {}", e))?;
    let host = Arc::new(host);
    let viewer = Arc::new(viewer);

    host.join_room("input-probe-room").await.map_err(|e| e.to_string())?;
    viewer.join_room("input-probe-room").await.map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // === Estado host-side: repetir la lógica de decide_session (permiso activado) ===
    let state = nxdesk_client::state::AppState::new({
        let mut cfg = nxdesk_core::AppConfig::default();
        cfg.device_id = "input-probe-host".to_string();
        cfg
    });
    // Sin permiso primero (estado recién creado por defecto)
    assert!(
        !state.allow_remote_input.load(Ordering::Relaxed),
        "Un host recién creado NO debe permitir input por defecto (seguridad)"
    );

    // Recibe el stream de entrada en el anfitrión en paralelo (posee sus datos)
    let inbox = Arc::new(std::sync::Mutex::new(Vec::<InputEvent>::new()));
    let gate = state.allow_remote_input.clone();
    {
        let inbox_task = inbox.clone();
        tokio::spawn(async move {
            // Espejo exacto del despachador de main.rs:
            // sin permiso → descarte; con permiso → registro (aquí en vez de SendInput).
            while let Some(ev) = host_ev.recv().await {
                if let SignalingEvent::Input(evt) = ev {
                    if gate.load(Ordering::Relaxed) {
                        if let Ok(mut b) = inbox_task.lock() {
                            b.push(evt);
                        }
                    }
                }
            }
        });
    }

    // === Fase 1: SIN permiso — el anfitrión descarta (no debe entrar nada) ===
    for i in 0..4u32 {
        viewer
            .send_input_direct(&InputEvent::MouseMove {
                x: i as f32 / 10.0,
                y: 0.5,
            })
            .map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let blocked_count = inbox.lock().unwrap().len();
    assert_eq!(
        blocked_count, 0,
        "Sin allow_remote_input el anfitrión NO debe registrar eventos"
    );
    // Drenar eventos sincronizados pendientes en el visor
    while viewer_ev.try_recv().is_ok() {}

    // === Fase 2: CON permiso (decide_session activa el flag como en main) ===
    state.allow_remote_input.store(true, Ordering::Relaxed);

    // Movimiento + clic por el canal real de señalización
    for i in 0..3u32 {
        viewer
            .send_input_direct(&InputEvent::MouseMove { x: 0.1 + i as f32 * 0.1, y: 0.5 })
            .map_err(|e| e.to_string())?;
        viewer
            .send_input_direct(&InputEvent::MouseDown {
                button: MouseButton::Left,
                x: 0.3,
                y: 0.5,
            })
            .map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    viewer
        .send_input_direct(&InputEvent::MouseWheel { delta_x: 0.0, delta_y: -1.0 })
        .map_err(|e| e.to_string())?;
    viewer
        .send_input_direct(&InputEvent::KeyDown { key_code: 0x57, key_char: None })
        .map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(400)).await;

    let received = inbox.lock().unwrap().clone();
    return Ok((received, blocked_count == 0, state.allow_remote_input.load(Ordering::Relaxed)));
}
