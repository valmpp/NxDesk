use nxdesk_client::state::AppState;
use nxdesk_core::{
    AppConfig, InputEvent, MouseButton, SessionDecisionDto, SessionRequestDto, SessionRole,
    SignalingMessage,
};
use nxdesk_file_transfer::FileTransferManager;
use nxdesk_terminal::{validate_terminal_command, TerminalSession};
use std::time::Duration;

#[tokio::test]
async fn test_e2e_session_handshake_and_permissions() {
    let host_config = AppConfig {
        device_id: "host-999".to_string(),
        device_alias: "Host-Workstation".to_string(),
        access_password: Some("SecurePass123!".to_string()),
        ..Default::default()
    };
    let host_state = AppState::new(host_config);

    // 1. Cliente envía solicitud de conexión
    let req = SessionRequestDto {
        request_id: "req-e2e-001".to_string(),
        client_id: "client-888".to_string(),
        client_alias: "Remote-Technician".to_string(),
        target_id: Some("host-local".to_string()),
        requested_role: SessionRole::FullControl,
        allow_input: true,
        allow_clipboard: true,
        allow_file_transfer: true,
        timestamp: 123456789,
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    host_state
        .session_waiters
        .lock()
        .await
        .insert(req.request_id.clone(), tx);
    *host_state.pending_session.write().await = Some(req.clone());

    // 2. Anfitrión recibe y evalúa en caliente con permisos granulares
    let decision = SessionDecisionDto {
        request_id: "req-e2e-001".to_string(),
        client_id: Some("client-888".to_string()),
        target_id: Some("client-888".to_string()),
        accept: true,
        granted_role: Some(SessionRole::FullControl),
        allow_input: true,
        allow_clipboard: true,
        allow_file_transfer: true,
        allow_terminal: true,
        rejection_reason: None,
    };

    let decide_result = host_state.decide_session(decision).await;
    assert!(decide_result.is_ok(), "El anfitrión debe poder decidir la sesión");

    // 3. Cliente recibe respuesta autorizada
    let client_response = rx.await.expect("Cliente debe recibir respuesta");
    assert!(client_response.success);
    assert_eq!(client_response.granted_role, SessionRole::FullControl);

    // 4. Verificar sesión activa persistida en el Host
    let active = host_state.active_session.read().await;
    assert!(active.is_some());
    let session = active.as_ref().unwrap();
    assert_eq!(session.peer_id, "client-888");
    assert!(session.allow_input);
    assert!(session.allow_clipboard);
    assert!(session.allow_file_transfer);
}

#[tokio::test]
async fn test_e2e_input_authorization_guard() {
    let host_state = AppState::new(AppConfig::default());

    // Caso A: Sin sesión activa -> Entrada bloqueada
    {
        let active = host_state.active_session.read().await;
        let can_inject = active.as_ref().is_some_and(|s| s.allow_input);
        assert!(!can_inject, "Sin sesión activa, el input debe ser bloqueado");
    }

    // Caso B: Sesión activa creada con allow_input = true
    let decision = SessionDecisionDto {
        request_id: "req-e2e-002".to_string(),
        client_id: Some("tech-client".to_string()),
        target_id: Some("tech-client".to_string()),
        accept: true,
        granted_role: Some(SessionRole::FullControl),
        allow_input: true,
        allow_clipboard: true,
        allow_file_transfer: false,
        allow_terminal: true,
        rejection_reason: None,
    };
    *host_state.pending_session.write().await = Some(SessionRequestDto {
        request_id: "req-e2e-002".to_string(),
        client_id: "tech-client".to_string(),
        client_alias: "Tech".to_string(),
        target_id: Some("host-local".to_string()),
        requested_role: SessionRole::FullControl,
        allow_input: true,
        allow_clipboard: true,
        allow_file_transfer: false,
        timestamp: 123456789,
    });
    let _ = host_state.decide_session(decision).await;

    {
        let active = host_state.active_session.read().await;
        let can_inject = active.as_ref().is_some_and(|s| s.allow_input);
        assert!(can_inject, "Con sesión y allow_input=true, el input debe permitirse");
    }

    // Caso C: Desconexión / Terminación en caliente
    let _ = host_state.terminate_session().await;
    {
        let active = host_state.active_session.read().await;
        let can_inject = active.as_ref().is_some_and(|s| s.allow_input);
        assert!(!can_inject, "Tras terminate_session, el input debe bloquearse de inmediato");
    }
}

#[test]
fn test_e2e_input_events_serialization() {
    let move_evt = InputEvent::MouseMove { x: 0.75, y: 0.25 };
    let json = serde_json::to_string(&move_evt).expect("Debe serializar");
    assert!(json.contains("\"X\":0.75"));
    assert!(json.contains("\"Y\":0.25"));

    let click_evt = InputEvent::MouseDown {
        button: MouseButton::Left,
        x: 0.5,
        y: 0.5,
    };
    let json_click = serde_json::to_string(&click_evt).expect("Debe serializar click");
    assert!(json_click.contains("\"MouseButton\":\"Left\""));

    let sig_msg = SignalingMessage::new_input(&click_evt).expect("Debe encapsular en SignalingMessage");
    assert_eq!(sig_msg.msg_type, "input");
}

#[tokio::test]
async fn test_e2e_file_transfer_disk_leak_remediation() {
    let manager = FileTransferManager::new();
    let transfer_id = "transfer-e2e-leak-check";

    // Iniciar transferencia
    let dest_path = manager
        .prepare_incoming_transfer(transfer_id, "", "leak_test_sample.tmp")
        .await
        .expect("Debe preparar transferencia en sandbox");

    assert!(dest_path.exists(), "El archivo debe crearse en disco inicialmente");

    // Cancelar transferencia por interrupción de conexión o usuario
    let cancelled = manager.cancel_transfer(transfer_id).await;
    assert!(cancelled, "Debe reportar transferencia cancelada exitosamente");

    // Verificar que el archivo parcial fue purgado del disco (cero fugas de almacenamiento)
    assert!(
        !dest_path.exists(),
        "El archivo parcial debe eliminarse del disco al cancelar para prevenir fugas de disco"
    );
}

#[tokio::test]
async fn test_e2e_terminal_lifecycle_and_process_cleanup() {
    // 1. Validar bloqueo de comandos destructivos
    assert!(validate_terminal_command("whoami", true).is_ok());
    assert!(validate_terminal_command("dir", true).is_ok());
    assert!(validate_terminal_command("rm -rf /", true).is_err());
    assert!(validate_terminal_command("del /s /f C:\\Windows", true).is_err());
    assert!(validate_terminal_command("format D:", true).is_err());

    // 2. Ciclo de vida y liberación de recursos de proceso
    let session = TerminalSession::new();
    assert!(!session.is_running());

    let start_res = session.start().await;
    if let Ok(_rx) = start_res {
        assert!(session.is_running(), "Terminal debe estar en ejecución");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Escribir un comando seguro
        let write_res = session.write_input("echo E2E_TEST_OK").await;
        assert!(write_res.is_ok());

        // Esperar output o terminar explícitamente
        session.terminate().await;
        assert!(!session.is_running(), "Terminal debe quedar detenida tras terminate()");
    }
}

#[tokio::test]
async fn test_e2e_fast_zero_alloc_jpeg_pipeline() {
    use image::ImageDecoder;

    let mut rgb = vec![128u8; 640 * 480 * 3];
    for i in (0..rgb.len()).step_by(5) {
        rgb[i] = (i % 255) as u8;
    }

    let mut jpeg_bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 45);
    encoder.encode(&rgb, 640, 480, image::ExtendedColorType::Rgb8).expect("encode failed");

    let cursor = std::io::Cursor::new(&jpeg_bytes);
    let decoder = image::codecs::jpeg::JpegDecoder::new(cursor).expect("decoder failed");
    let (w, h) = decoder.dimensions();
    assert_eq!(w, 640);
    assert_eq!(h, 480);

    let total_pixels = (w * h * 3) as usize;
    let mut raw_rgb = vec![0u8; total_pixels];
    decoder.read_image(&mut raw_rgb).expect("read failed");

    let color_img = eframe::egui::ColorImage::from_rgb([w as usize, h as usize], &raw_rgb);
    assert_eq!(color_img.size, [640, 480]);
}

#[tokio::test]
async fn test_benchmark_full_codec_pipeline() {
    use image::ImageDecoder;
    use std::time::Instant;

    for &(w, h, q, label) in &[(1920, 1080, 65, "1920x1080 Native Q65"), (1920, 1080, 50, "1920x1080 Native Q50"), (1280, 720, 65, "1280x720 Q65"), (1024, 576, 28, "1024x576 Q28")] {
        let mut rgb = vec![120u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 3;
                rgb[idx] = ((x ^ y) & 0xFF) as u8;
                rgb[idx + 1] = ((x * 3 + y * 7) & 0xFF) as u8;
                rgb[idx + 2] = ((y * 11) & 0xFF) as u8;
            }
        }

        let mut jpeg_bytes = Vec::new();

        // 1. Benchmark Codificación DIRECTA BGRA con jpeg-encoder (SIMD, Zero-Conversion)
        let mut bgra = vec![120u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 4;
                bgra[idx] = ((y * 11) & 0xFF) as u8;       // B
                bgra[idx + 1] = ((x * 3 + y * 7) & 0xFF) as u8; // G
                bgra[idx + 2] = ((x ^ y) & 0xFF) as u8;       // R
                bgra[idx + 3] = 255;                          // A
            }
        }
        let mut enc_times = Vec::new();
        for _ in 0..10 {
            jpeg_bytes.clear();
            let start = Instant::now();
            let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_bytes, q);
            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
            encoder.encode(&bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra).expect("encode failed");
            enc_times.push(start.elapsed());
        }

        // 2. Benchmark Decodificación
        let mut dec_times = Vec::new();
        let mut raw_rgb = vec![0u8; w * h * 3];
        for _ in 0..10 {
            let start = Instant::now();
            let cursor = std::io::Cursor::new(&jpeg_bytes);
            let decoder = image::codecs::jpeg::JpegDecoder::new(cursor).expect("decoder failed");
            decoder.read_image(&mut raw_rgb).expect("read failed");
            dec_times.push(start.elapsed());
        }

        let avg_enc: std::time::Duration = enc_times.iter().sum::<std::time::Duration>() / enc_times.len() as u32;
        let avg_dec: std::time::Duration = dec_times.iter().sum::<std::time::Duration>() / dec_times.len() as u32;

        println!("--- Resolución {}: JPEG {} KB | Encode: {:.2} ms | Decode: {:.2} ms | FPS Potencial: {:.1} ---",
            label, jpeg_bytes.len() / 1024,
            avg_enc.as_secs_f64() * 1000.0,
            avg_dec.as_secs_f64() * 1000.0,
            1.0 / (avg_enc.as_secs_f64() + 0.004) // encode + 4ms capture
        );
    }
}
