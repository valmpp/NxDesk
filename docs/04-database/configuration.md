# ⚙️ Configuración y Parámetros del Sistema — NxDesk

| Código | DAT-CFG-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Core Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Fuentes de Configuración

La configuración del nodo se resuelve en tiempo de inicio evaluando en orden jerárquico:
1. **Parámetros de Línea de Comandos (`CLI Flags`):** Tienen prioridad máxima sobre cualquier valor persistido.
2. **Variables de Entorno (`Environment Variables` / `.env`):** Utilizadas principalmente en despliegues desatendidos o servicios en segundo plano.
3. **Valores Predeterminados Seguros (`Defaults`):** Generados dinámicamente si no se suministra ninguna opción previa.

---

## 2. Parámetros del Cliente (`AppConfig`)

| Parámetro | Flag CLI | Variable de Entorno | Valor Predeterminado | Descripción |
| :--- | :--- | :--- | :---: | :--- |
| **ID de Dispositivo** | `--device-id=<ID>` | `DEVICE_ID` | `111-222-333` | Identificador único de este puesto de trabajo. |
| **Alias del Nodo** | `--alias=<NAME>` | `DEVICE_ALIAS` | `Hostname del OS` | Nombre legible anunciado en el radar LAN. |
| **URL de Señalización** | `--signaling-server=<URL>`| `SIGNALING_URL` | `ws://127.0.0.1:5000/signalinghub` | Dirección del hub central de señalización. |
| **Contraseña Anfitrión** | `--password=<PW>` | `ACCESS_PASSWORD` | Generada aleatoriamente (`NX-XXXXXX`) | Clave para autorizar control remoto desatendido. |
| **Terminal Remota** | `--allow-remote-terminal` | `ALLOW_TERMINAL` | `false` | Habilita o deshabilita la terminal de diagnóstico. |
| **Límite de FPS** | `--fps=<FPS>` | `MAX_FPS` | `30` | Límite máximo de cuadros por segundo (5 a 60). |
| **Calidad JPEG** | `--quality=<Q>` | `JPEG_QUALITY` | `75` | Factor de compresión JPEG (10 a 100). |
| **Ruta Sandbox** | `--shared-folder=<DIR>` | `SANDBOX_PATH` | `%USERPROFILE%\Downloads\NxDesk_Transfers` | Directorio autorizado para transferencias. |

---

## 3. Parámetros del Servidor de Señalización

| Parámetro | Variable de Entorno | Valor Predeterminado | Descripción |
| :--- | :--- | :---: | :--- |
| **Dirección de Escucha** | `BIND_ADDR` | `0.0.0.0:5000` | Interfaz y puerto TCP del servidor WebSocket. |
| **Nivel de Logs** | `RUST_LOG` | `info` | Filtro de trazas de ejecución (`error`, `warn`, `info`, `debug`). |
