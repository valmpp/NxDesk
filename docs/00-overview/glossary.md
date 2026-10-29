# 📖 Glosario Técnico — NxDesk

| Código | OVR-GLO-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

| Término | Definición Técnica |
| :--- | :--- |
| **Anfitrión (Host)** | El equipo que comparte su pantalla y periféricos para ser administrado o visualizado remotamente. |
| **Operador (Client / Receiver)** | El equipo que se conecta a una sala remota para ver la pantalla del anfitrión y enviarle eventos de control. |
| **Señalización (Signaling)** | Canal de mensajería (WebSocket o SignalR) utilizado para negociar la conexión, transferir metadatos y coordinar la sesión antes y durante el streaming. |
| **0x1E Record Separator** | Carácter especial de control ASCII (`\u{1e}`) utilizado por el protocolo SignalR JSON para delimitar el final de cada trama de mensaje sobre sockets TCP/WebSocket continuos. |
| **eframe / egui** | Framework de interfaz gráfica inmediata en Rust, acelerado por GPU y con cero dependencias de navegadores web. |
| **Glow (OpenGL)** | Backend de renderizado ligero basado en OpenGL clásico. Reemplazó a `wgpu` para eliminar fallos nativos de drivers Vulkan en Windows. |
| **SendInput** | API nativa de Win32 de bajo nivel para sintetizar pulsaciones de teclado, movimientos del cursor y clics de ratón en el sistema operativo anfitrión. |
| **DXGI Desktop Duplication** | Interfaz de DirectX 11 en Windows que permite capturar los fotogramas del búfer de pantalla directamente en la memoria de la tarjeta gráfica (VRAM) sin pasar por la CPU. |
| **Room ID** | Cadena de identificación de sala (ej. `111-222-333` o `HOST-WIN11`) que agrupa al anfitrión y a los operadores autorizados en el servidor de señalización. |
| **File Chunk** | Bloque binario fragmentado (habitualmente de 64 KB a 256 KB) con desplazamiento (offset) e índice secuencial, utilizado para transferir archivos sin saturar la memoria RAM. |
| **Sandbox de Archivos** | Directorio restringido (`%USERPROFILE%\Downloads\NxDesk_Transfers`) del cual ninguna operación de subida, bajada o listado de archivos puede escapar. |
| **GDI (Graphics Device Interface)** | Subsistema gráfico heredado de Windows utilizado como motor de captura universal en Rust por su alta compatibilidad en sistemas donde DXGI o Vulkan fallan. |
