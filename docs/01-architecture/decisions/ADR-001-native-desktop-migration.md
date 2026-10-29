# ADR-001 — Migración a Aplicación de Escritorio Nativa Pura (Descarte de Axum Web Server)

| Código | ADR-001 | Versión | v1.0 |
|--------|---------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Chief Architecture Officer |
| Estado | Aceptado | Clasificación | Decisión Arquitectónica Clave |

---

## Estado
**Aceptado y Ejecutado.**

## Contexto
Originalmente, el cliente de Rust incluía un servidor web embebido basado en `axum` (`web_server.rs`) que servía un panel de control HTTP en el puerto `8080` para ser visualizado en un navegador web (Chrome, Edge, etc.).

## Problema
1. El requerimiento estricto del producto es ofrecer una experiencia de software de escritorio idéntica a AnyDesk o TeamViewer, **NO una página web**.
2. Operar un servidor web local abriendo puertos HTTP en cada máquina cliente aumentaba innecesariamente la superficie de ataque (CSRF, CORS, exposición accidental a la red local).
3. La latencia y el consumo de recursos de renderizar un stream de pantalla dentro del motor de un navegador web degradaban la experiencia frente al cliente nativo C# Avalonia.
4. `web_server.rs` introducía más de 2,800 líneas de código acoplado y docenas de dependencias transitivas en `Cargo.lock`.

## Alternativas Consideradas
1. **Opción A (Mantener híbrido):** Correr simultáneamente la interfaz de escritorio egui y el servidor web Axum. (Descartada: código duplicado, puertos expuestos innecesarios y consumo de memoria elevado).
2. **Opción B (Electron / WebView2):** Envolver el dashboard web en un ejecutable de escritorio. (Descartada: consumo de cientos de megabytes de RAM e incompatibilidad con la meta de ultra-bajo consumo).
3. **Opción C (Escritorio Nativo Puro con egui/eframe):** Eliminar completamente Axum y el servidor web local, implementando toda la interfaz gráfica, controles de anfitrión y lienzo remoto en la ventana de escritorio nativa de Rust. (**Elegida**).

## Decisión
Se decidió eliminar permanentemente `nxdesk-rust/apps/client/src/web_server.rs`, retirar la dependencia `axum` de `Cargo.toml`, y consolidar el 100% de la lógica de usuario en la aplicación de escritorio nativa `eframe/egui` en `app.rs`.

## Motivos
* Paridad visual y funcional 1:1 con la interfaz de C# Avalonia (`MainWindow.axaml` y `HomeView.axaml`).
* Reducción de más de 1,400 líneas en `Cargo.lock`.
* Cero puertos HTTP abiertos en el cliente; toda la comunicación sale exclusivamente hacia el servidor de señalización.
* Arranque instantáneo del ejecutable (< 1.2 segundos).

## Consecuencias
### Positivas
* Eliminación total de la superficie de ataque web en el cliente.
* Menor consumo de memoria RAM (< 65 MB en reposo).
* Experiencia de usuario uniforme y profesional como aplicación nativa de Windows/Linux.

### Trade-offs
* Los usuarios no pueden conectarse desde un navegador web sin instalar la aplicación cliente nativa.
