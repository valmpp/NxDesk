# 🔄 Flujo E2E: Conexión y Unión a Salas de Señalización — NxDesk

| Código | FLW-CON-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Protocol Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Ciclo de Negociación de Sala

```mermaid
sequenceDiagram
    autonumber
    participant Node as Nodo NxDesk (Host o Client)
    participant Hub as Servidor de Señalización (Port 5000)

    Node->>Hub: WebSocket Upgrade (ws://host:5000/signalinghub)
    Hub-->>Node: 101 Switching Protocols (Connection Established)
    Node->>Hub: {"Type":"join","Payload":"111-222-333"}\u001e
    Note over Hub: Valida Regex ^[a-zA-Z0-9\-_]{1,64}$ y aforo <= 5
    Hub->>Hub: Registra ConnectionId en room "111-222-333"
    Hub-->>Node: {"Type":"RoomOccupancyChanged","Payload":"1"}\u001e
    Note over Node: Actualiza contador de usuarios en AppState
```

---

## 2. Salida y Desconexión Abrupta

Cuando un nodo cierra la aplicación, pierde la red o invoca explícitamente `leave`:
1. El WebSocket emite una trama `Close` o el socket TCP se interrumpe (`EOF`).
2. El servidor ejecuta inmediatamente `cleanup_connection(conn_id)`:
   * Remueve la conexión de `s.clients` y del mapa `s.rooms[room_id]`.
   * Calcula la nueva ocupación restante.
   * Si la sala aún tiene miembros, difunde el evento `RoomOccupancyChanged` con el nuevo total.
   * Si la sala quedó vacía, destruye la entrada del mapa para liberar memoria.
