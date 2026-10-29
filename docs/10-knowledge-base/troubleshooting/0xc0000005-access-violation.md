# 💡 Solución a la Violación de Acceso STATUS_ACCESS_VIOLATION (0xc0000005) — NxDesk

| Código | KB-TRB-001 | Versión | v1.0 |
|--------|------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Desktop Engineering Team |
| Estado | Aprobado | Clasificación | Base de Conocimiento (KB) |

---

## Cuándo Usar Esta Guía
Cuando el binario de Rust falle al iniciar con el código de error `0xc0000005` en Windows.

---

## Diagnóstico
El crash ocurre dentro del sondeo de adaptadores gráficos de `wgpu` (`vulkan-1.dll`, `igvk64.dll`). Ocurre típicamente en laptops con doble GPU (Intel integrada + NVIDIA dedicada) o en sistemas con capas de captura de pantalla de software de terceros.

---

## Solución Paso a Paso
1. En `nxdesk-rust/apps/client/Cargo.toml`, forzar el backend `glow`:
   ```toml
   eframe = { version = "0.37.0", default-features = false, features = ["glow", "default_fonts", "accesskit"] }
   ```
2. En `nxdesk-rust/apps/client/src/main.rs`, pasar `Renderer::Glow`:
   ```rust
   let options = eframe::NativeOptions {
       renderer: eframe::Renderer::Glow,
       ..Default::default()
   };
   ```
3. Recompilar el cliente:
   ```powershell
   cargo build -p nxdesk-client
   ```

---

## Resultado Esperado
La aplicación arranca inmediatamente con aceleración OpenGL sin interactuar con los drivers de Vulkan.

---

## Origen
* Reporte de Incidente: [BUG-001](../../11-incidents/bugs/BUG-001-status-access-violation-0xc0000005.md)
* Registro de Decisión: [ADR-002](../../01-architecture/decisions/ADR-002-glow-opengl-renderer.md)
