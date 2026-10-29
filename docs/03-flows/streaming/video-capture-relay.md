# 🔄 Flujo E2E: Streaming de Video y Retransmisión de Frames — NxDesk

| Código | FLW-STR-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Video & Systems Engineering |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Ciclo de Vida del Frame de Video

```mermaid
sequenceDiagram
    autonumber
    participant HostCapturer as Motor de Captura (Win32 GDI)
    participant HostPipe as CapturePipeline (Tokio Task)
    participant Hub as Servidor de Señalización
    participant ClientView as RemoteView Canvas (Client egui)

    loop Cada intervalo (ej. 33ms para 30 FPS)
        HostPipe->>HostCapturer: capture_frame()
        HostCapturer-->>HostPipe: Raw BGRA Buffer (Width x Height x 4)
        HostPipe->>HostPipe: to_jpeg(quality = 75)
        HostPipe->>Hub: Binary WebSocket Message (JPEG Bytes)
        Hub->>ClientView: Binary WebSocket Message (JPEG Bytes)
        ClientView->>ClientView: Carga textura GPU en egui::ColorImage
        ClientView->>ClientView: Pinta en pantalla con escalado adaptativo
    end
```

---

## 2. Parámetros Técnicos de Transmisión

* **Formato de Color Nativo:** BGRA 32-bit (8 bits por canal).
* **Compresión de Salida:** JPEG estándar con codificación acelerada por software en `crates/capture`.
* **Tamaño Promedio por Frame:** Entre 45 KB y 180 KB dependiendo de la complejidad visual de la pantalla (texto estático vs. video en movimiento).
* **Ancho de Banda Típico:** 1.5 Mbps a 6.0 Mbps para 1080p a 30 FPS.
