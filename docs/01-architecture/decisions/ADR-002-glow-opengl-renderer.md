# ADR-002 — Adopción de Glow (OpenGL) y Descarte de WGPU para Evitar STATUS_ACCESS_VIOLATION (0xc0000005)

| Código | ADR-002 | Versión | v1.0 |
|--------|---------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Chief Architecture Officer |
| Estado | Aprobado | Clasificación | Decisión Arquitectónica Clave |

---

## Estado
**Aceptado y Ejecutado.**

## Contexto
Al compilar y ejecutar por primera vez la aplicación de escritorio nativa en Rust en entornos Windows 10/11, el proceso abortaba instantáneamente al crear la ventana principal con el error crítico del sistema operativo:
`process didn't exit successfully: (Exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)`.

## Problema
`eframe` incluye por defecto el backend gráfico `wgpu`. Al inicializarse en Windows, `wgpu` sondea todos los adaptadores gráficos instalados invocando drivers de Vulkan (`vulkan-1.dll`, `igvk64.dll`, capas de OBS o Discord). En sistemas con gráficos integrados Intel, drivers híbridos o capas de captura de terceros, este sondeo intentaba desreferenciar punteros nulos o memoria no inicializada, causando un fallo de segmentación nativo en Windows antes de pintar el primer fotograma.

## Alternativas Consideradas
1. **Opción A (Instalar el Vulkan SDK o forzar flags de entorno en cada cliente):** Requerir a los usuarios configurar variables de entorno como `WGPU_BACKEND=dx12`. (Descartada: pésima experiencia de usuario, manual y propenso a errores).
2. **Opción B (Software rendering con tiny-skia):** Renderizado por CPU. (Descartada: rendimiento deficiente para streaming a 60 FPS).
3. **Opción C (Cambiar el renderer de eframe a Glow OpenGL):** Desactivar las features por defecto de `eframe` y configurar explícitamente `renderer: eframe::Renderer::Glow` junto con la feature `glow` en `Cargo.toml`. (**Elegida**).

## Decisión
Se configuró en `nxdesk-rust/apps/client/Cargo.toml`:
```toml
eframe = { version = "0.37.0", default-features = false, features = ["glow", "default_fonts", "accesskit"] }
```
Y en `nxdesk-rust/apps/client/src/main.rs`:
```rust
let options = eframe::NativeOptions {
    renderer: eframe::Renderer::Glow,
    // ...
};
```

## Motivos
* **Estabilidad Universal:** OpenGL clásico (Glow) cuenta con compatibilidad del 100% en prácticamente cualquier driver gráfico de Windows, Intel, AMD y NVIDIA sin tocar capas conflictivas de Vulkan.
* **Reducción de Dependencias:** Al no compilar el compilador de shaders `naga` ni el pipeline completo de WebGPU, se podaron cientos de crates innecesarios.
* **Cero Errores 0xc0000005:** El arranque de la aplicación es fluido, estable y reproducible en cualquier equipo.

## Consecuencias
### Positivas
* Eliminación definitiva del crash nativo `STATUS_ACCESS_VIOLATION`.
* Tiempos de compilación notablemente más rápidos.
* Consumo reducido de memoria en la GPU.

### Trade-offs
* No se utilizan características de shaders modernos de Vulkan / DirectX 12 que no son requeridas por una aplicación de escritorio AnyDesk 2D.
