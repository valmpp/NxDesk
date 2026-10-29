# 📜 Reglas de Negocio del Cliente — NxDesk

| Código | MOD-CLI-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Desktop Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Reglas de Sesión y Conexión

* **BR-CLI-001 (Unicidad de Sesión Anfitrión):** Un anfitrión local solo puede mantener activa **una única sesión de control remoto** a la vez. Si ingresa una nueva solicitud mientras ya existe una sesión en curso, se rechaza automáticamente o se le presenta al usuario la opción de transferir la sesión.
* **BR-CLI-002 (Consentimiento Explícito por Defecto):** Salvo que se configure explícitamente una contraseña de acceso no asistido (`--password`), toda solicitud entrante debe mostrar el diálogo modal `ConnectionTypeDialog` requiriendo que el usuario anfitrión haga clic en **Aceptar**.
* **BR-CLI-003 (Mínimo Privilegio de Roles):** El anfitrión puede asignar al par entrante cualquiera de los siguientes roles:
  * `FullControl`: Transmisión de pantalla + Inyección de ratón/teclado + Transferencia de archivos + Terminal.
  * `ViewOnly`: Transmisión de pantalla únicamente (control remoto y transferencias bloqueados).
  * `FileOnly`: Transferencia de archivos únicamente (pantalla y periféricos bloqueados).
* **BR-CLI-004 (Interrupción de Emergencia del Anfitrión):** El usuario anfitrión siempre mantiene el control prioritario absoluto a través del widget `HostDisconnectWindow`. Al presionar **Desconectar**, la sesión remota se destruye de inmediato y se revocan todos los permisos de periféricos.

---

## 2. Reglas de Inyección y Captura

* **BR-CLI-005 (Filtro Previo de Inyección):** Ningún evento `SignalingEvent::Input` es transmitido al driver del sistema operativo a menos que `active_session.is_some_and(|s| s.allow_input)` sea verdadero.
* **BR-CLI-006 (Transmisión Bajo Demanda):** El bucle de captura de pantalla solo envía fotogramas JPEG a la red si hay 2 o más usuarios presentes en la sala de señalización (`room_occupancy > 1`).
