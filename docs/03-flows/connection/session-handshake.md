# 🔄 Flujo E2E: Handshake de Sesión AnyDesk — NxDesk

| Código | FLW-CON-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Protocol Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Diagrama de Secuencia E2E

```mermaid
sequenceDiagram
    autonumber
    participant Client as Operador Remoto
    participant Hub as Servidor de Señalización
    participant Host as Anfitrión Local
    participant UI as Modal ConnectionTypeDialog

    Client->>Hub: RelayMessage(RoomId, SessionRequestDto)
    Hub->>Host: ReceiveMessage(SessionRequestDto)
    Host->>UI: Despliega Diálogo Modal con datos del par
    Note over UI: El anfitrión revisa alias, ID, IP y selecciona Rol
    alt Anfitrión Acepta
        UI->>Host: Confirmar(Rol: FullControl, allow_input: true)
        Host->>Host: Crea ActiveSessionInfo y activa HostDisconnectWindow
        Host->>Hub: RelayMessage(RoomId, SessionDecisionDto { Aceptado: true })
        Hub->>Client: ReceiveMessage(SessionDecisionDto)
        Client->>Client: Conmuta a RemoteView y activa canvas de video
    else Anfitrión Rechaza
        UI->>Host: Cancelar(Motivo: "Rechazado por el usuario")
        Host->>Hub: RelayMessage(RoomId, SessionDecisionDto { Aceptado: false })
        Hub->>Client: ReceiveMessage(SessionDecisionDto)
        Client->>Client: Notifica desconexión y vuelve a HomeView
    end
```

---

## 2. Contratos de Mensajes del Handshake

### `SessionRequestDto`
```json
{
  "RequestId": "req-98765",
  "RequesterId": "222-333-444",
  "RequesterAlias": "Soporte TI - Laptop",
  "RequestedRole": "FullControl",
  "Timestamp": 1730105000
}
```

### `SessionDecisionDto`
```json
{
  "RequestId": "req-98765",
  "IsAccepted": true,
  "GrantedRole": "FullControl",
  "RejectionReason": null,
  "AllowInput": true,
  "AllowClipboard": true,
  "AllowFileTransfer": true,
  "SessionToken": "tok-abc123xyz"
}
```
