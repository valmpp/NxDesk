# 🔄 Flujo E2E: Ejecución Segura de Terminal Remota — NxDesk

| Código | FLW-TRM-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security & Systems Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Diagrama de Control Defensivo

```mermaid
sequenceDiagram
    autonumber
    participant Client as Operador Remoto
    participant HostPipe as Receiver en main.rs
    participant TermService as RemoteTerminalService
    participant OS as Subproceso del Sistema (cmd.exe / bash)

    Client->>HostPipe: SignalingMessage("terminal_cmd", "dir C:\\")
    Note over HostPipe: Paso 1: ¿Sesión activa tiene rol FullControl?
    alt Sin permiso
        HostPipe-->>Client: ⛔ [Seguridad] Comando rechazado: sesión no autorizada
    else Autorizado
        HostPipe->>TermService: WriteInputAsync("dir C:\\")
        Note over TermService: Paso 2: ¿Contiene comandos peligrosos o pipes?
        alt Contiene caracteres '&', '|', ';' o comandos no autorizados
            TermService-->>Client: ⛔ [Seguridad] Comando bloqueado por política
        else Comando en Lista Blanca (dir, ipconfig, ping, etc.)
            TermService->>OS: StandardInput.Write("dir C:\\\r\n")
            OS-->>TermService: StandardOutput Relay
            TermService-->>Client: SignalingMessage("terminal_out", "El volumen es...")
        end
    end
```

---

## 2. Comandos Diagnósticos Habilitados

* **Inspección de Archivos y Rutas:** `dir`, `ls`, `cd`, `pwd` (dentro del sandbox).
* **Diagnóstico de Red:** `ping`, `ipconfig`, `ifconfig`, `hostname`.
* **Identidad y Entorno:** `whoami`, `ver`, `systeminfo`, `date`, `time`.
* **Control de Consola:** `help`, `cls`, `clear`, `echo`.
