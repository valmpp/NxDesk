# DEV-RUN-002 — Caída o Desconexión del Servidor de Señalización

| Código | DEV-RUN-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Runbook Operativo de Emergencia |

## Historial
| Versión | Fecha | Cambio | Autor |
|---------|-------|--------|-------|
| v1.0 | 2026-10-28 | Creación del runbook para contingencias de señalización | NxDesk Operations Team |

---

## 1. Cómo Detectar
* En el encabezado del cliente NxDesk, el estado de conexión pasa a desconectado (`🔴 Desconectado`).
* Los clientes no pueden unirse a salas ni recibir solicitudes de sesión remota.
* La solicitud `curl -I http://servidor-senales:5000/health` devuelve `Connection refused` o timeout.

---

## 2. Qué Revisar Primero
1. Comprobar si el proceso del servidor `nxdesk-server` está corriendo:
   ```bash
   systemctl status nxdesk-server
   # o en Docker:
   docker compose ps
   ```
2. Verificar si el puerto TCP 5000 está ocupado por otro proceso:
   ```bash
   sudo netstat -tlpn | grep 5000
   ```

---

## 3. Pasos para Resolver
1. Si el servicio falló, reiniciar el demonio:
   ```bash
   sudo systemctl restart nxdesk-server
   ```
2. Si el log indica saturación de descriptores de socket:
   ```bash
   # Incrementar límite de descriptores abiertos en Linux
   ulimit -n 65535
   sudo systemctl restart nxdesk-server
   ```
3. Verificar que los clientes se reconectan automáticamente en < 5 segundos gracias a la política de reintento exponencial de `SignalingClient`.

---

## 4. A quién Escalar
* **Rol:** DevOps / Administrador de Infraestructura de Red.
* **Condición:** Si la desconexión se debe a degradación del enlace WAN o fallos en el proxy Nginx.

---

## 5. Cómo Verificar
Ejecutar `curl http://localhost:5000/health` y validar respuesta `{"status":"healthy","service":"nxdesk-server"}`.
