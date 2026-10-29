# 💥 Registro de Cambios Rompientes (Breaking Changes) — NxDesk

| Código | CHG-BRK-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Release Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Eliminación Permanente de la API Web Axum (`web_server.rs`)

* **Naturaleza del Cambio:** Rompiente de arquitectura.
* **Qué se Eliminó:** Los endpoints HTTP REST servidos previamente en el puerto local `8080` (`/api/status`, `/api/screens`, `/api/connect`, `/api/disconnect`, `/api/terminal/auth`, `/api/files`, `/api/input`, `/ws/stream`).
* **Motivo:** Cumplir con la directriz del proyecto de operar exclusivamente como una **aplicación de escritorio nativa**, eliminando navegadores web y reduciendo la superficie de ataque.
* **Migración:** Todas las interacciones de conexión, cambio de pantalla, transferencia de archivos y visualización remota se realizan directamente dentro de la ventana de escritorio de Rust (`nxdesk-client`) o de C# Avalonia (`NxDesk.Client`).

---

## 2. Parámetro de Línea de Comandos Retirado

* El flag `--web-port=<PORT>` ya no es reconocido por el binario `nxdesk-client`. Pasarlo arrojará un error de argumento desconocido.
