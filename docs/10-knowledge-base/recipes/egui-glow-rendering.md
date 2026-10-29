# 💡 Receta: Renderizado y Pintado de Lienzo Remoto con egui y Glow — NxDesk

| Código | KB-RCP-001 | Versión | v1.0 |
|--------|------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Desktop Engineering Team |
| Estado | Aprobado | Clasificación | Base de Conocimiento (KB) |

---

## Patrón de Carga de Textura Dinámica en egui

Para renderizar un stream de pantalla continuo a 30-60 FPS sin fugas de VRAM ni lag de la UI:

```rust
// 1. Obtener los últimos bytes JPEG del búfer
if let Some(bytes) = state.latest_jpeg_frame.read().await.as_ref() {
    // 2. Decodificar a ColorImage en memoria
    if let Ok(image) = image::load_from_memory(bytes) {
        let size = [image.width() as usize, image.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            size,
            &image.to_rgba8().into_raw(),
        );

        // 3. Actualizar la textura GPU existente en lugar de recrearla
        let texture = ctx.load_texture("remote_screen", color_image, egui::TextureOptions::LINEAR);
        
        // 4. Pintar en el canvas disponible
        ui.image(&texture);
    }
}
```

---

## Regla de Oro de Rendimiento
Reutilizar el identificador de textura (`"remote_screen"`) para que `egui` sobreescriba la textura en la GPU en lugar de acumular descriptores de textura huérfanos en la memoria de video.
