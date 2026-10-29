# BUG-001 — Violación de Acceso al Iniciar Aplicación de Escritorio en Windows

## Estado
🟢 Resuelto

## Severidad
Crítica (Bloqueante de Arranque)

## Fecha
2026-10-28

---

## 1. Problema
### Descripción
Al ejecutar `cargo run -p nxdesk-client` o lanzar el binario compilado `nxdesk-client.exe` en Windows 11, el proceso abortaba instantáneamente sin abrir ventana alguna, arrojando el código de terminación:
`process didn't exit successfully: (Exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)`.

### Cómo Reproducir
1. Configurar `eframe = "0.37.0"` con features por defecto (que incluye `wgpu`).
2. Ejecutar `cargo run -p nxdesk-client` en una máquina con Windows 10/11 y drivers gráficos estándar de Intel/NVIDIA.
3. Observar el crash inmediato antes del primer ciclo de renderizado.

---

## 2. Investigación
Se ejecutó un análisis de depuración con WinDbg / lldb. La violación de acceso se originaba en la DLL del driver gráfico de Vulkan al consultar adaptadores disponibles (`vkEnumeratePhysicalDevices`), desreferenciando un puntero nulo provocado por capas de captura superpuestas en el sistema operativo.

---

## 3. Intentos
### Intento #1
Forzar variables de entorno como `WGPU_BACKEND=dx12`.
*Resultado:* ❌ Frágil; requería configuración manual en cada usuario final y fallaba si DirectX 12 tenía capas de optimización activadas.

---

## 4. Causa
**Causa:** Conflicto del backend `wgpu` con drivers y capas Vulkan instaladas en Windows.  
**Certeza:** 🟢 Confirmada.

---

## 5. Resolución
Se desacopló `wgpu` de `eframe` configurando `default-features = false` y habilitando el backend `Glow (OpenGL)` en `Cargo.toml`, además de fijar `renderer: eframe::Renderer::Glow` en `NativeOptions`.

---

## 6. Validación
- [x] Caso original resuelto (la ventana abre en < 1.2 segundos).
- [x] Sin regresiones en pruebas unitarias (`cargo test --workspace`).
- [x] 0 errores de compilación y 0 advertencias de linter (`cargo clippy`).

---

## 7. Knowledge Base
* [Solución 0xc0000005](../../10-knowledge-base/troubleshooting/0xc0000005-access-violation.md)
* [ADR-002](../../01-architecture/decisions/ADR-002-glow-opengl-renderer.md)
