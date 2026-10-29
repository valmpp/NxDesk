# 🔌 Mensajes y Protocolo del Servidor de Señalización — NxDesk

| Código | MOD-SRV-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Backend Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Handshake y Conexión WebSocket

* **Ruta de Conexión:** `ws://<HOST>:<PUERTO>/signalinghub` (o `http://<HOST>:<PUERTO>/signalinghub` con upgrade a WebSocket).
* **Endpoint de Salud (Health Check):** `GET /health` ➔ Responde `200 OK` con `{"status":"healthy","service":"nxdesk-server"}`.

---

## 2. Tipos de Mensajes Soportados

Todos los mensajes de texto deben enviarse formateados en JSON y finalizados con el carácter delimitador ASCII `\u{1e}` (Record Separator):

### A. Solicitud de Unión a Sala (`join`)
```json
{"Type":"join","Payload":"111-222-333"}
```
* **Validación:** El ID de sala debe coincidir con la expresión regular `^[a-zA-Z0-9\-_]{1,64}$`.
* **Capacidad:** Si la sala ya cuenta con 5 miembros, el servidor rechaza la conexión con advertencia de seguridad.

### B. Salida de Sala (`leave`)
```json
{"Type":"leave","Payload":"111-222-333"}
```

### C. Retransmisión de Mensaje de Control (`SignalingMessage`)
```json
{
  "Type": "Input",
  "SenderId": "conn_12345",
  "SenderAlias": "Operador-Soporte",
  "Payload": "{\"type\":\"MouseMove\",\"x\":0.45,\"y\":0.62}",
  "Timestamp": 1730100000
}
```
* **Validación:** El servidor comprueba que `conn_12345` esté efectivamente registrado en la sala correspondiente antes de retransmitir el mensaje a los demás miembros.

### D. Notificación de Ocupación de Sala (`RoomOccupancyChanged`)
Emitido automáticamente por el servidor a todos los participantes de la sala cuando un cliente se une o se desconecta:
```json
{"Type":"RoomOccupancyChanged","Payload":"2"}
```

### E. Frames Binarios de Video (Binary Message)
Paquetes binarios puros transmitidos directamente por el socket. El servidor los enruta en memoria al resto de clientes de la sala sin deserialización, garantizando latencia cero.
