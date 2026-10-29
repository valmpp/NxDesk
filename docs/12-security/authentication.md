# 🔒 Seguridad: Modelo de Autenticación — NxDesk

| Código | SEC-AUT-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Mecanismos de Autenticación Soportados

NxDesk implementa dos modalidades de autenticación según el escenario de operación:

### Modo 1: Asistencia Interactiva (Consentimiento en Pantalla)
* **Caso de Uso:** Soporte técnico a un usuario que se encuentra frente a su computadora.
* **Flujo:** No requiere contraseña previa. El operador remoto ingresa el ID AnyDesk del anfitrión; el software anfitrión despliega en caliente el diálogo modal `ConnectionTypeDialog` solicitando aprobación explícita del usuario local.
* **Seguridad:** Cero riesgo de acceso no supervisado. Si no hay nadie frente a la máquina para pulsar **Aceptar**, la petición expira a los 30 segundos.

### Modo 2: Acceso No Asistido (Contraseña de Anfitrión)
* **Caso de Uso:** Administración remota de servidores desatendidos o computadoras de oficina.
* **Flujo:** El anfitrión se inicia con una contraseña fija (`--password="MiClaveSegura"` o en `.env`).
* **Protección:**
  * Rate-limiting estricto por IP (`InMemoryRateLimiter`): Máximo 5 intentos fallidos en una ventana de 60 segundos antes de bloqueo temporal.
  * Hashing en memoria y comparación de tiempo constante (`subtle::ConstantTimeEq`) para prevenir ataques de temporización (Timing Attacks).
