//! Sonda de auditoría de experiencia de usuario (UX) y rendimiento para NxDesk.
//! Mide:
//! 1. Tiempos reales de codificación JPEG para 1920x1080 y 1366x768 a calidades Q85, Q78, Q70 y Q50.
//! 2. Tamaños de fotograma y si activan el umbral de re-codificación (frame_cap = 192 KB / 128 KB).
//! 3. Tasa de cuadros por segundo máxima sostenible (FPS potencial).
//! 4. Efecto de downscaling 1920x1080 -> 1366x768 (pérdida de pixeles y ratio de escala).
//! 5. Dispersión y precisión de coordenadas de ratón (sub-pixel jumping).

use std::time::Instant;

#[test]
fn test_ux_codec_and_fps_audit() {
    println!("\n==================================================================");
    println!("🔍 [AUDITORÍA UX] RENDIMIENTO DE CÓDEC Y CADENCIA DE FPS");
    println!("==================================================================");

    // Resoluciones a auditar:
    // A) Anfitrión nativo: 1920x1080
    // B) Cliente laptop:   1366x768
    // C) 720p optimizado:  1280x720
    let resolutions = [
        (1920, 1080, "Anfitrión Nativo 1080p (1920x1080)"),
        (1366, 768, "Laptop Cliente (1366x768)"),
        (1280, 720, "Resolución HD Optimizada (1280x720)"),
    ];

    // Calidades relevantes en el pipeline de main.rs y config.json:
    // - 85: Calidad configurada en dist/config.json ("quality": 85)
    // - 78: Calidad base normal en main.rs (rtt <= 80)
    // - 72: Calidad bajo rtt > 160
    // - 70: Calidad forzada en el pase de re-codificación (frame_cap excedido)
    let qualities = [85, 78, 70];

    for &(w, h, desc) in &resolutions {
        println!("\n--- Pantalla: {} ---", desc);
        // Generar un fotograma realista de escritorio (texto, ventanas, fondos degradados)
        let mut bgra = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 4;
                // Simulación de patrones de escritorio (barras, ventanas, contrastes)
                let is_window_border = x % 400 == 0 || y % 300 == 0;
                let is_text_pattern = (x % 20 < 10) && (y % 14 < 3);
                if is_window_border {
                    bgra[idx] = 200;
                    bgra[idx + 1] = 200;
                    bgra[idx + 2] = 200;
                } else if is_text_pattern {
                    bgra[idx] = 20;
                    bgra[idx + 1] = 20;
                    bgra[idx + 2] = 20;
                } else {
                    bgra[idx] = ((x ^ y) & 0x7F) as u8 + 120;
                    bgra[idx + 1] = ((x * 2 + y) & 0x7F) as u8 + 80;
                    bgra[idx + 2] = (y & 0x7F) as u8 + 40;
                }
                bgra[idx + 3] = 255;
            }
        }

        for &q in &qualities {
            let mut jpeg_buf = Vec::with_capacity(512 * 1024);
            let mut encode_times = Vec::new();

            // Medir 15 pasadas
            for _ in 0..15 {
                jpeg_buf.clear();
                let t0 = Instant::now();
                let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, q);
                encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
                encoder.encode(&bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra).expect("encode");
                encode_times.push(t0.elapsed());
            }

            let avg_enc = encode_times.iter().sum::<std::time::Duration>() / encode_times.len() as u32;
            let kb = jpeg_buf.len() / 1024;
            let enc_ms = avg_enc.as_secs_f64() * 1000.0;

            // Umbrales de frame_cap en main.rs: 192 KB (RTT bajo), 160 KB (RTT medio), 128 KB (RTT alto)
            let triggers_reencode_192 = jpeg_buf.len() > 192 * 1024;
            let triggers_reencode_128 = jpeg_buf.len() > 128 * 1024;

            // Tiempo estimado con captura (~8-15ms en GDI/DIBSection)
            let total_frame_ms_no_reenc = enc_ms + 12.0; // 12ms captura promedio
            let potential_fps_no_reenc = 1000.0 / total_frame_ms_no_reenc;

            // Si hay re-encode a Q70:
            let reenc_penalty_ms = if triggers_reencode_192 { enc_ms * 0.85 } else { 0.0 };
            let total_frame_ms_with_reenc = total_frame_ms_no_reenc + reenc_penalty_ms;
            let effective_fps = 1000.0 / total_frame_ms_with_reenc;

            println!(
                "  Q{:02}: {:>3} KB | Enc: {:>5.2} ms | Re-encode? {:>5} (192K) / {:>5} (128K) | FPS Teórico: {:>4.1} -> Efectivo: {:>4.1} FPS",
                q, kb, enc_ms,
                if triggers_reencode_192 { "SÍ ⚠️" } else { "NO" },
                if triggers_reencode_128 { "SÍ ⚠️" } else { "NO" },
                potential_fps_no_reenc,
                effective_fps
            );
        }
    }

    println!("\n==================================================================");
    println!("🔍 [AUDITORÍA UX] ESCALADO Y MAPEADO DE ENTRADA (1366x768 vs 1920x1080)");
    println!("==================================================================");

    let host_w = 1920.0f32;
    let host_h = 1080.0f32;
    let client_w = 1366.0f32;
    let client_h = 768.0f32;
    let topbar_h = 50.0f32;
    let avail_h = client_h - topbar_h; // 718.0

    let host_aspect = host_w / host_h; // 1.7778
    let avail_aspect = client_w / avail_h; // 1.9025

    // Lógica en app.rs:
    // let target_size = if avail_aspect > tex_aspect {
    //     egui::vec2((avail.y * tex_aspect).floor(), avail.y)
    // } else {
    //     egui::vec2(avail.x, (avail.x / tex_aspect).floor())
    // };
    let render_w = (avail_h * host_aspect).floor(); // 718 * 1.7778 = 1276 px
    let render_h = avail_h; // 718 px
    let pillarbox_each_side = (client_w - render_w) / 2.0;
    let scale_ratio = render_h / host_h;

    println!("• Pantalla Anfitrión: {:.0}x{:.0} (Aspect Ratio: {:.4})", host_w, host_h, host_aspect);
    println!("• Laptop Cliente:    {:.0}x{:.0} | Área útil: {:.0}x{:.0} (Aspect Ratio: {:.4})", client_w, client_h, client_w, avail_h, avail_aspect);
    println!("• Renderizado Fit:   {:.0}x{:.0} (Escala: {:.1}%)", render_w, render_h, scale_ratio * 100.0);
    println!("• Bandas negras:     {:.0} px en cada lateral (Pillarbox)", pillarbox_each_side);
    println!("• Desperdicio datos: {:.1}% de los pixeles enviados por el host son descartados al reescalar",
        (1.0 - (render_w * render_h) / (host_w * host_h)) * 100.0
    );

    // Mapeo de ratón:
    let step_client_px = 1.0f32;
    let jump_host_x = (step_client_px / render_w) * host_w;
    let jump_host_y = (step_client_px / render_h) * host_h;
    println!("• Sensibilidad Ratón: 1 px en laptop = {:.2} px en host X | {:.2} px en host Y", jump_host_x, jump_host_y);
    println!("  -> Salto de cursor: El ratón se desplaza {:.2} píxeles por cada píxel de pantalla física del cliente.", jump_host_x);
}
