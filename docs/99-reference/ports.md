# ⚡ Referencia Rápida: Puertos de Red y Conectividad — NxDesk

| Código | REF-PRT-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Network & Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## Matriz de Puertos de Red

| Puerto | Protocolo | Dirección | Componente | Función y Tráfico |
| :---: | :---: | :---: | :--- | :--- |
| **5000** | `TCP` | Inbound en Servidor | `nxdesk-server` / `NxDesk.Server` | Señalización WebSocket, Handshake SignalR y Streaming de video/input. |
| **5001** | `UDP` | Bidireccional LAN | `crates/discovery` / `DiscoveryService` | Radar de descubrimiento automático de equipos en red local (Broadcast). |
| **443** | `TCP` | Inbound (Reverse Proxy) | `nginx` / `caddy` | Terminación TLS segura (WSS / HTTPS) en entornos de producción WAN. |
| **80** | `TCP` | Inbound (Reverse Proxy) | `nginx` / `certbot` | Desafío ACME Let's Encrypt y redirección automática a HTTPS. |

---

## Verificación de Puertos en Windows (PowerShell)

```powershell
# Verificar si el puerto de señalización 5000 está en escucha
Get-NetTCPConnection -LocalPort 5000 -ErrorAction SilentlyContinue

# Probar conectividad hacia el servidor de señalización
Test-NetConnection -ComputerName 127.0.0.1 -Port 5000
```
