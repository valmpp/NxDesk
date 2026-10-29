# 📘 Documentación Técnica Oficial — NxDesk

## 1. Descripción General
NxDesk es una plataforma de escritorio remoto, asistencia técnica y streaming de ultra-baja latencia que opera de forma autónoma sin depender de servidores centralizados en la nube. Resuelve la latencia elevada, la dependencia de servicios comerciales propietarios y la falta de control sobre la privacidad en redes locales y empresariales.

---

## 2. Tecnologías y Requisitos

* **Rust (2021 Edition / 100% Nativo):** Cliente de escritorio nativo con eframe / egui (aceleración Glow OpenGL), Servidor de señalización asíncrono Tokio-Tungstenite, motor de captura multimonitor Win32, inyector de entrada `SendInput`, radar UDP LAN y sandbox hermético de archivos.
* **Arquitectura:** Monorepo Rust modularizado en 7 crates (`core`, `capture`, `input`, `discovery`, `signaling`, `file-transfer`, `terminal`) y 2 aplicaciones ejecutables (`client`, `server`).
* **Protocolos:** WebSockets con delimitador SignalR `0x1E`, UDP Broadcast en puerto `5001`.

### Requisitos del Sistema
* **Windows 10 / 11 (64-bit)** o **Linux** (con OpenGL / X11 / Wayland).
* **Rust Toolchain:** `rustc` y `cargo` 1.75+ (`rustup default stable`).
* Red local Wi-Fi, Ethernet o WAN para streaming y control remoto.

---

## 3. Instalación y Configuración

1. **Clonar el repositorio:**
   ```powershell
   git clone https://github.com/usuario/NxDesk.git
   cd NxDesk
   ```

2. **Compilar y Ejecutar el Servidor de Señalización (Rust):**
   ```powershell
   cd nxdesk-rust
   cargo run -p nxdesk-server
   ```

3. **Compilar y Ejecutar el Cliente de Escritorio Nativo (Rust):**
   ```powershell
   cargo run -p nxdesk-client
   ```

4. **Ejecutar Suite de Pruebas Automatizadas y E2E:**
   ```powershell
   cargo test --workspace
   ```

4. **Configurar regla de Firewall en Windows (PowerShell como Administrador):**
   ```powershell
   New-NetFirewallRule -DisplayName "NxDesk Ports" -Direction Inbound -LocalPort 5000,5001,5050 -Protocol TCP -Action Allow
   ```

---

## 4. Uso de la Aplicación de Escritorio

### Caso 1: Ejecutar la Aplicación de Escritorio Nativa (NxDesk Desktop App)
Abre la interfaz gráfica nativa de escritorio (Avalonia UI con soporte para bandeja del sistema, radar LAN y conexión AnyDesk):
```powershell
dotnet run --project src/NxDesk.Client/NxDesk.Client.csproj
```
> **Características de la aplicación de escritorio:**
> * Abre una ventana nativa de Windows (sin necesidad de abrir navegadores web).
> * Muestra tu ID de puesto de trabajo de 9 dígitos.
> * Radar de equipos descubiertos en red local en tiempo real con tarjetas interactivas.
> * Cuadro de diálogo de petición y autorización de conexión entrante (`ConnectionTypeDialog`).
> * Ventana flotante de sesión activa con temporizador y botón de desconexión (`HostDisconnectWindow`).

### Caso 2: Ejecutar el Motor de Señalización o Servidor Central
```powershell
# En Rust (alto rendimiento Tokio):
cargo run -p nxdesk-server

# O en C# ASP.NET Core:
dotnet run --project src/NxDesk.Server/NxDesk.Server.csproj
```
Escucha en `http://0.0.0.0:5000/signalinghub` para coordinar salas concurrentes y relay P2P.

---

## 5. Estructura del Proyecto

```text
NxDesk/
├── nxdesk-rust/                 # Ecosistema nativo en Rust puro de alto rendimiento
│   ├── apps/client/             # Nodo cliente con Axum Web Dashboard, captura y control
│   ├── apps/server/             # Servidor de señalización Tokio-Tungstenite (puerto 5000)
│   ├── crates/capture/          # Captura Win32 GDI BitBlt y enumeración multimonitor
│   ├── crates/core/             # DTOs y modelos de datos compartidos con serialización PascalCase
│   ├── crates/discovery/        # Radar LAN UDP broadcast en puerto 5001 con Socket2
│   ├── crates/file-transfer/    # Gestor de archivos por bloques con sandbox estricto
│   ├── crates/input/            # Inyección de ratón y teclado mediante Win32 SendInput
│   ├── crates/signaling/        # Cliente WebSocket compatible con delimitador SignalR 0x1E
│   └── crates/terminal/         # Sesión interactiva ConPTY con blindaje de seguridad VUL-01
├── src/                         # Ecosistema C# .NET 10 bajo Clean Architecture
│   ├── NxDesk.Core/             # Entidades de dominio (PeerIdentity, SignalingMessage)
│   ├── NxDesk.Application/      # Interfaces de servicios y DTOs tipados
│   ├── NxDesk.Infrastructure/   # Adaptadores DirectX 11, GStreamer, SendInput y Kestrel
│   ├── NxDesk.Client/           # Aplicación de escritorio Avalonia UI 11 Fluent Theme
│   └── NxDesk.Server/           # Web API SignalR dedicada con documentación Swagger
├── packaging/linux/             # Archivos de empaquetado y accesos de distribución
├── tests/                       # Pruebas de integración y recursos de verificación
└── dist/releases/               # Distribuciones binarias empaquetadas
```

---

## 6. Arquitectura y Flujo Principal

```mermaid
graph LR
    subgraph Host["Equipo Anfitrión"]
        Cap[Win32 BitBlt] --> Enc[JPEG Encoder]
        Enc --> Hub[Axum :5050 / WS Stream]
        Rad1[UDP Radar :5001]
    end

    subgraph Net["Transporte de Red"]
        Sig[nxdesk-server :5000]
        Direct[Conexión Directa LAN]
    end

    subgraph Viewer["Equipo Visor"]
        UI[Navegador Web / Client CLI]
        Rad2[UDP Radar :5001]
    end

    Rad1 <-->|Broadcast| Rad2
    Hub <-->|Señalización| Sig
    Sig <-->|Salas / Relay| UI
    Hub <-->|Direct Stream| Direct <--> UI
    UI -->|Input Events 30Hz| Hub --> Inp[Win32 SendInput]
```

1. **Descubrimiento:** Los nodos emiten pulsos periódicos en `UDP :5001` anunciando su ID, IP y estado.
2. **Negociación:** Los clientes se unen a una sala compartida en `TCP :5000` o establecen conexión directa vía HTTP/WS en `TCP :5050`.
3. **Streaming:** El anfitrión captura la pantalla (individual o mosaico virtual multimonitor), comprime los fotogramas en JPEG y los transmite por WebSocket.
4. **Control:** El visor normaliza coordenadas `(0.0 a 1.0)` y envía eventos de ratón/teclado filtrados a 30 Hz hacia el inyector `SendInput`.

---

## 7. Variables de Entorno y Configuración

| Parámetro CLI | Variable / Config | Descripción | Valor por Defecto |
| :--- | :--- | :--- | :--- |
| `--id=<ID>` | `DEVICE_ID` | Identificador único del equipo en formato de 9 dígitos | `111-222-333` (auto-generado al conectar) |
| `--alias=<NAME>` | `COMPUTERNAME` | Nombre amigable visible en el radar local | Hostname del sistema operativo |
| `--port=<PORT>` | `WEB_PORT` | Puerto HTTP para el Web Dashboard interactivo | `5050` (auto-incrementa si está ocupado) |
| `--server=<URL>` | `SIGNALING_URL` | URL WebSocket del servidor de señalización | `ws://127.0.0.1:5000/signalinghub` |
| `--password=<P>` | `ACCESS_PASSWORD` | Contraseña requerida para autorizar control remoto | Ninguna (acceso libre) |
| `--allow-remote-terminal` | `ALLOW_TERMINAL` | Bandera de seguridad para habilitar la terminal remota | `false` (bloqueada por defecto - VUL-01) |
| `--fps=<FPS>` | `MAX_FPS` | Límite máximo de fotogramas por segundo capturados | `30` |
| `--quality=<Q>` | `JPEG_QUALITY` | Calidad de compresión JPEG (1 a 100) | `75` |
| `--shared-folder=<DIR>` | `SANDBOX_PATH` | Carpeta segura autorizada para transferencia de archivos | `~/Downloads/NxDeskShared` |

---

## 8. Protocolo de Señalización y Eventos de Escritorio Nativo

La comunicación en tiempo real entre pares se gestiona mediante eventos bidireccionales sobre WebSocket con enmarcado delimitado por `0x1E` (SignalR Compatible):

| Evento / Mensaje | Carga Útil (Payload) | Dirección | Descripción y Políticas de Seguridad |
| :--- | :--- | :---: | :--- |
| `join` | `{ "Payload": "<RoomID>" }` | Cliente ➔ Servidor | Solicita ingreso a la sala de señalización (validación alfanumérica, máx 5 clientes). |
| `leave` | `{ "Payload": "<RoomID>" }` | Cliente ➔ Servidor | Abandona la sala y notifica decremento de ocupación a los pares restantes. |
| `Input` | `InputEvent` (JSON) | Remoto ➔ Anfitrión | Inyección de ratón/teclado. **Seguridad:** Requiere sesión activa aprobada con `allow_input == true`. |
| `frame` | Binario (JPEG) | Anfitrión ➔ Remoto | Fotogramas de video comprimidos en JPEG acelerado (calidad y FPS dinámicos). |
| `select_screen` | Índice entero (`"0"`, `"1"`) | Remoto ➔ Anfitrión | Conmuta dinámicamente el monitor capturado sin interrumpir la sesión activa. |
| `terminal_cmd` | Texto del comando | Remoto ➔ Anfitrión | Ejecución diagnóstica. **Seguridad:** Requiere rol `FullControl` y validación por lista blanca estricta. |
| `file_chunk` | `FileChunk` (JSON) | Bidireccional | Chunks de transferencia. **Seguridad:** Sandbox obligatorio en `%USERPROFILE%\Downloads\NxDesk_Transfers`. |
| `SessionRequest` | `SessionRequestDto` | Remoto ➔ Anfitrión | Petición AnyDesk en caliente con modal de consentimiento previo en pantalla del anfitrión. |
| `SessionDecision`| `SessionDecisionDto`| Anfitrión ➔ Remoto | Aprobación o rechazo con roles granulares (`FullControl`, `ViewOnly`, `FileOnly`). |

---

## 9. Pruebas y Contribución

### Ejecución de Pruebas Automatizadas
Para verificar la integridad de todos los módulos, serialización y validaciones de seguridad:
```powershell
# Batería completa de pruebas unitarias en Rust (23 tests aprobados)
cd nxdesk-rust
cargo test --all

# Verificación de compilación limpia de la solución C#
dotnet build --no-incremental
```

### Guía de Contribución
1. Haz un Fork del repositorio y crea una rama: `git checkout -b feature/nueva-mejora`.
2. Mantén la separación de capas de **Clean Architecture** (Dominio libre de dependencias).
3. Asegura **0 advertencias** y **0 errores** de compilación en `cargo check` y `dotnet build`.
4. Ejecuta `cargo test --all` antes de enviar cualquier Pull Request.

---

## 10. Despliegue en Producción y Operaciones SRE

### 10.1 Reverse Proxy TLS (Caddy y Nginx)
Para exponer el Web Dashboard y el Hub de Señalización en Internet pública (WAN) con cifrado SSL/TLS estricto (`https://` y `wss://`):

* **Caddy (Recomendado):** Utiliza [deploy/caddy/Caddyfile](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/deploy/caddy/Caddyfile). Caddy aprovisiona y renueva automáticamente certificados Let's Encrypt / ZeroSSL sin requerir configuración manual.
  ```bash
  caddy run --config deploy/caddy/Caddyfile
  ```
* **Nginx:** Emplea [deploy/nginx/nxdesk.conf](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/deploy/nginx/nxdesk.conf) con soporte para WebSocket Upgrade (`Upgrade $http_upgrade`, `Connection $connection_upgrade`) y buffers deshabilitados para garantizar 60 FPS sin buffering.

### 10.2 Gestión de Procesos Daemon (Alta Disponibilidad 24/7)
* **Linux (Systemd):**
  Unidad de servicio en [deploy/systemd/nxdesk-server.service](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/deploy/systemd/nxdesk-server.service).
  ```bash
  sudo cp deploy/systemd/nxdesk-server.service /etc/systemd/system/
  sudo systemctl daemon-reload
  sudo systemctl enable --now nxdesk-server.service
  ```
  Configurado con `Restart=always`, `RestartSec=5s`, descriptores `LimitNOFILE=65536` y políticas de aislamiento de kernel (`ProtectSystem=full`, `NoNewPrivileges=true`).

* **Windows (Servicio de Windows con NSSM):**
  Script automatizado en [deploy/windows/install-service-nssm.ps1](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/deploy/windows/install-service-nssm.ps1).
  ```powershell
  # Ejecutar en PowerShell como Administrador:
  .\deploy\windows\install-service-nssm.ps1 -Port 5000
  ```
  Registra el servicio con auto-recuperación ante caídas en menos de 5 segundos y rotación automática de logs en `logs/service-stdout.log` y `logs/service-stderr.log`.

### 10.3 Despliegue Contenerizado con Docker Compose
```bash
cd deploy/docker
docker compose up -d
```
Levanta simultáneamente el contenedor nativo de `nxdesk-server` junto con el proxy inverso Caddy configurado para HTTP/3 QUIC y TLS automático.

