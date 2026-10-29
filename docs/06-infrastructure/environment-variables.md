# 🔐 Infraestructura: Variables de Entorno y Configuración Segura — NxDesk

| Código | INF-ENV-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk DevOps & Security Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

> **Regla de Seguridad Fundamental (Regla #07):** Este documento lista exclusivamente los nombres, tipos y propósitos de las variables de entorno. **NUNCA** deben guardarse contraseñas, tokens o secretos reales en archivos rastreados por Git.

---

## 1. Variables del Cliente (`nxdesk-client`)

| Variable | Tipo | Valor de Ejemplo | Descripción |
| :--- | :---: | :--- | :--- |
| `DEVICE_ID` | `string` | `111-222-333` | Identificador único de este equipo en la red. |
| `DEVICE_ALIAS` | `string` | `PC-Soporte-01` | Nombre amigable presentado a otros pares. |
| `SIGNALING_URL` | `string` | `wss://nxdesk.empresa.com/signalinghub` | URL del servidor de señalización WebSocket. |
| `ACCESS_PASSWORD` | `string` | `[SECRETO]` | Contraseña para autorizar conexiones desatendidas. |
| `ALLOW_TERMINAL` | `bool` | `false` | Activa (`true`) o bloquea (`false`) la terminal remota. |
| `MAX_FPS` | `integer` | `30` | Límite de tasa de fotogramas (5 a 60). |
| `JPEG_QUALITY` | `integer` | `75` | Nivel de compresión de imagen (10 a 100). |
| `SANDBOX_PATH` | `string` | `C:\NxDesk_Transfers` | Directorio autorizado para transferencias. |

---

## 2. Variables del Servidor (`nxdesk-server`)

| Variable | Tipo | Valor de Ejemplo | Descripción |
| :--- | :---: | :--- | :--- |
| `BIND_ADDR` | `string` | `0.0.0.0:5000` | Interfaz y puerto TCP de escucha del servidor. |
| `RUST_LOG` | `string` | `info` o `nxdesk_server=debug` | Filtro de nivel de trazas de logging. |
