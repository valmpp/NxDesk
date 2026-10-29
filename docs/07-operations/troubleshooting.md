# 🛠️ Operaciones: Guía General de Solución de Problemas — NxDesk

| Código | OPS-TRB-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Problemas Comunes y Soluciones Rápidas

### Síntoma 1: "No aparecen otros equipos en la sección Descubierto"
* **Causa:** El Firewall de Windows está bloqueando las tramas UDP entrantes en el puerto `5001`, o los equipos se encuentran en subredes/VLANs diferentes sin reenvío de broadcast.
* **Solución:**
  1. Ejecutar el script `permitir-firewall.bat` como Administrador en ambas máquinas.
  2. Verificar con `Get-NetFirewallRule -DisplayName "*NxDesk*"` que la regla UDP 5001 esté habilitada.
  3. Si están en redes separadas, conectarse directamente ingresando el ID de sala o la IP en la barra de búsqueda superior.

### Síntoma 2: "Pantalla negra en RemoteView pero el ratón responde"
* **Causa:** El monitor del anfitrión entró en suspensión por ahorro de energía de Windows, o la sesión de escritorio está bloqueada (`Win+L`).
* **Solución:** Mover el ratón desde el cliente remoto para despertar la pantalla o desbloquear la sesión del anfitrión.

### Síntoma 3: "Rechazado: La sala ha alcanzado el límite máximo de conexiones"
* **Causa:** La sala tiene 5 conexiones activas (límite anti-DoS).
* **Solución:** Desconectar espectadores inactivos o reiniciar la sala.
