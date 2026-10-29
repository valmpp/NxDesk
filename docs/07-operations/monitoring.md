# 📊 Operaciones: Monitoreo y Métricas de Rendimiento — NxDesk

| Código | OPS-MON-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Métricas Clave de Operación (SLIs)

| Métrica | Umbral Normal | Umbral Crítico | Acción de Mitigación |
| :--- | :---: | :---: | :--- |
| **Latencia de Frame (RTT)** | `< 35 ms` | `> 120 ms` | Reducir calidad JPEG (`quality`) o conmutar resolución. |
| **Ocupación de Memoria (Cliente)** | `40 - 90 MB` | `> 250 MB` | Verificar descarte de texturas en egui y limpieza de chunks. |
| **Ocupación de Memoria (Servidor)** | `< 25 MB` | `> 100 MB` | Purgar salas inactivas y cerrar sockets huérfanos. |
| **Tasa de Caída de Frames (Drop Rate)**| `< 2%` | `> 15%` | Activar backpressure adaptativo en el pipeline de captura. |
| **Tiempo de Respuesta Health Check** | `< 5 ms` | `> 200 ms` | Analizar saturación del Tokio event loop en el servidor. |

---

## 2. Puntos de Telemetría y Logs

* **Trazabilidad de Seguridad:**  
  Cualquier rechazo de comando en terminal, intento de evasión de sandbox o inyección de input no autorizada emite un log con el prefijo:
  `⛔ [Seguridad] ...` con nivel `WARN` o `ERROR`.
* **Logging Estructurado:**  
  Configurable mediante `RUST_LOG=info,nxdesk_client=debug,nxdesk_server=trace`.
