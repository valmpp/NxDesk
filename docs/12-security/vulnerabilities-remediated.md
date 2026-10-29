# 🛡️ Seguridad: Registro de Vulnerabilidades Mitigadas — NxDesk

| Código | SEC-VUL-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Senior Security Auditor |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## Matriz de Remediación de Seguridad (OWASP & CWE)

| ID | Severidad | Vulnerabilidad / Hallazgo | Norma | Archivos Corregidos | Estado |
| :---: | :---: | :--- | :---: | :--- | :---: |
| **01** | ⛔ **CRÍTICO** | Inyección de input arbitrario sin validar sesión activa ni permiso `allow_input`. | CWE-20<br>CWE-285 | [main.rs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/client/src/main.rs#L276-L315) | 🟢 Mitigado |
| **02** | ⛔ **CRÍTICO** | Servidor de señalización vulnerable a secuestro de salas y saturación DoS. | CWE-306<br>CWE-400 | [SignalingHub.cs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/src/NxDesk.Server/Hubs/SignalingHub.cs)<br>[main.rs (server)](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/server/src/main.rs#L216-L245) | 🟢 Mitigado |
| **03** | 🔴 **ROJO** | Evasión de lista negra en terminal remota permitiendo ejecución de comandos arbitrarios. | CWE-78<br>OWASP A03 | [RemoteTerminalService.cs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/src/NxDesk.Infrastructure/Services/RemoteTerminalService.cs#L120-L168) | 🟢 Mitigado |
| **04** | 🔴 **ROJO** | Fuga de archivos arbitrarios del anfitrión mediante `InitiateFileTransferAsync` fuera del sandbox. | CWE-22<br>OWASP A01 | [FileTransferService.cs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/src/NxDesk.Infrastructure/Services/FileTransferService.cs#L14-L40) | 🟢 Mitigado |
| **05** | 🔴 **ROJO** | Dependencias NuGet vulnerables con avisos GHSA de alta severidad. | CWE-1395<br>OWASP A06 | [NxDesk.Infrastructure.csproj](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/src/NxDesk.Infrastructure/NxDesk.Infrastructure.csproj)<br>[NxDesk.Server.csproj](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/src/NxDesk.Server/NxDesk.Server.csproj) | 🟢 Mitigado |
| **06** | 🟡 **MEDIO** | Posibles panics por `.unwrap()` en generación de timestamps y parsing de sockets. | CWE-754 | [discovery/lib.rs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/crates/discovery/src/lib.rs#L59-L115)<br>[main.rs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/client/src/main.rs#L80-L86) | 🟢 Mitigado |
| **07** | 🟡 **MEDIO** | Clonaciones redundantes de memoria a 30-60 FPS en el bucle de streaming. | CWE-400 | [capture_pipeline.rs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/client/src/capture_pipeline.rs#L62-L78) | 🟢 Mitigado |
| **08** | 🟢 **BUENO** | Sección 8 obsoleta de endpoints web Axum en la documentación técnica. | Clean Arch | [DOCUMENTACION.md](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/DOCUMENTACION.md#L150-L170) | 🟢 Mitigado |
