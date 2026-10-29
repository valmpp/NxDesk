# ⚡ Referencia Rápida: Variables de Entorno — NxDesk

| Código | REF-ENV-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

Consulte la especificación completa, alcances y directrices de seguridad de las variables de entorno en el documento principal de infraestructura:

👉 **[docs/06-infrastructure/environment-variables.md](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/docs/06-infrastructure/environment-variables.md)**

---

## Tabla Resumen de Variables

| Variable | Alcance | Valor Típico | Propósito |
| :--- | :---: | :--- | :--- |
| `DEVICE_ID` | Cliente | `111-222-333` | Identificador de puesto de trabajo. |
| `DEVICE_ALIAS` | Cliente | `Hostname` | Nombre visible en el radar LAN. |
| `SIGNALING_URL` | Cliente | `ws://127.0.0.1:5000/signalinghub` | URL del servidor central. |
| `ACCESS_PASSWORD` | Cliente | `[SECRETO]` | Clave para acceso desatendido. |
| `ALLOW_TERMINAL` | Cliente | `false` | Habilita o bloquea la terminal remota. |
| `MAX_FPS` | Cliente | `30` | Límite de tasa de captura (5 a 60). |
| `JPEG_QUALITY` | Cliente | `75` | Calidad de compresión JPEG (10 a 100). |
| `SANDBOX_PATH` | Cliente | `%USERPROFILE%\Downloads\NxDesk_Transfers` | Directorio seguro de ficheros. |
| `BIND_ADDR` | Servidor | `0.0.0.0:5000` | Interfaz y puerto de escucha. |
| `RUST_LOG` | General | `info` | Nivel de detalle de telemetría y logs. |
