# 🔌 Integración: Protocolo SignalR Hub y Enmarcado 0x1E — NxDesk

| Código | INT-SIG-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Protocol Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. El Protocolo de Hubs de SignalR

NxDesk utiliza el estándar de SignalR de ASP.NET Core para permitir que clientes escritos en .NET y clientes escritos en Rust se comuniquen a través del mismo servidor de señalización de manera transparente.

### Handshake Inicial
1. El cliente inicia una conexión WebSocket a `/signalinghub`.
2. El cliente envía el mensaje inicial de protocolo especificando versión y formato JSON:
   ```json
   {"protocol":"json","version":1}\u001e
   ```
3. El servidor responde confirmando el protocolo:
   ```json
   {}\u001e
   ```

---

## 2. Tipos de Invocaciones Soportadas

* **Invocación a Método del Hub (`Type: 1`):**  
  Utilizado para invocar métodos remotos en el servidor como `JoinRoom` o `RelayMessage`:
  ```json
  {"type":1,"target":"JoinRoom","arguments":["111-222-333"]}\u001e
  ```
* **Recepción de Mensajes en el Cliente:**  
  Cuando el servidor retransmite un mensaje a los participantes de la sala:
  ```json
  {"type":1,"target":"ReceiveMessage","arguments":[{"Type":"Input","Payload":"..."}]}\u001e
  ```
* **Mensajes de Keep-Alive (Ping / Pong - `Type: 6`):**  
  Se transmiten cada 15 segundos para mantener abierta la conexión a través de proxies y cortafuegos:
  ```json
  {"type":6}\u001e
  ```
