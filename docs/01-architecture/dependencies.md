# 📦 Dependencias y Ecosistema de Librerías — NxDesk

| Código | ARC-DEP-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Chief Architecture Officer |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Dependencias del Ecosistema Rust (`Cargo.toml`)

Tras la optimización y descarte de crates pesados innecesarios (eliminación de `axum` y podado del compilador WebGPU `wgpu/naga`), el árbol de dependencias de `Cargo.lock` se redujo de más de 5,270 líneas a **3,778 líneas**.

| Crate | Versión | Propósito Arquitectónico | Justificación Técnica |
| :--- | :---: | :--- | :--- |
| **`eframe`** | `0.37.0` | Framework de ventana y contexto GUI para `egui`. | Configurado con `default-features = false, features = ["glow", "default_fonts", "accesskit"]` para usar OpenGL ligero. |
| **`egui`** | `0.37.0` | Motor de interfaz de modo inmediato. | Renderizado acelerado por GPU, layout declarativo y alta reactividad. |
| **`tokio`** | `1.38+` | Runtime asíncrono para operaciones I/O. | Features `full` para sockets, timers, mutex y canales de mensajería (`mpsc`, `broadcast`, `oneshot`). |
| **`tokio-tungstenite`** | `0.26+` | Cliente y servidor WebSocket nativo. | Manejo de tramas de texto y binarias de alta velocidad. |
| **`image`** | `0.25+` | Procesamiento y codificación de imagen. | Compresión multihilo a formato JPEG para streaming de pantalla. |
| **`windows`** | `0.58+` | Bindings oficiales de Win32 API. | Acceso de bajo nivel a `GDI`, `GetDC`, `CreateCompatibleBitmap` y `SendInput`. |
| **`socket2`** | `0.5+` | Configuración avanzada de sockets de red. | Habilita `SO_REUSEADDR` y `SO_BROADCAST` para el radar UDP 5001. |
| **`serde` / `serde_json`** | `1.0+` | Serialización y deserialización de datos. | Formateo de mensajes y eventos de red con tipado estricto. |
| **`tracing` / `tracing-subscriber`** | `0.1+` | Telemetría y logging estructurado. | Trazabilidad de eventos de red y seguridad sin bloqueo de hilos. |

---

## 2. Dependencias del Ecosistema C# (.NET 10.0)

Todas las dependencias de NuGet se encuentran auditadas contra avisos de seguridad (GHSA) con **0 alertas de vulnerabilidad conocidas**.

| Paquete NuGet | Versión | Proyecto | Propósito |
| :--- | :---: | :--- | :--- |
| **`Avalonia`** | `11.3.12` | `NxDesk.Client` | Framework de interfaz Fluent MVVM multiplataforma. |
| **`Avalonia.Desktop`** | `11.3.12` | `NxDesk.Client` | Backend de ventana de escritorio nativo para Windows/Linux/macOS. |
| **`CommunityToolkit.Mvvm`** | `8.4.0` | `NxDesk.Client` | Generadores de código fuente para ViewModels (`[ObservableProperty]`, `[RelayCommand]`). |
| **`Microsoft.AspNetCore.SignalR.Client`** | `10.0.14` | `NxDesk.Infrastructure` | Cliente oficial de SignalR alineado con .NET 10 (sin vulnerabilidades). |
| **`Microsoft.OpenApi`** | `2.13.0` | `NxDesk.Server` | Especificación y documentación OpenAPI (parche contra GHSA-v5pm-xwqc-g5wc). |
| **`Tmds.DBus.Protocol`** | `0.21.3` | `NxDesk.Client` | Protocolo D-Bus para integración nativa en Linux (parche contra GHSA-xrw6-gwf8-vvr9). |
| **`SharpDX.Direct3D11` / `DXGI`** | `4.2.0` | `NxDesk.Infrastructure` | Captura acelerada por GPU mediante Desktop Duplication API en Windows. |
| **`Newtonsoft.Json`** | `13.0.4` | `NxDesk.Infrastructure` | Serialización de mensajes compatible con SignalR JSON. |
