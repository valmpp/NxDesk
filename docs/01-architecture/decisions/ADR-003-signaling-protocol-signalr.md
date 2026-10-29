# ADR-003 — Protocolo de Señalización Basado en SignalR JSON con Delimitador 0x1E

| Código | ADR-003 | Versión | v1.0 |
|--------|---------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Chief Architecture Officer |
| Estado | Aprobado | Clasificación | Decisión Arquitectónica Clave |

---

## Estado
**Aceptado.**

## Contexto
El ecosistema NxDesk requería un protocolo de señalización y coordinación de salas que permitiese interoperabilidad completa entre el cliente de C# (.NET 10 Avalonia) y el cliente de Rust (`nxdesk-client`), así como entre los servidores implementados en ambos lenguajes (`NxDesk.Server` y `nxdesk-server`).

## Problema
Diseñar un protocolo binario propietario desde cero aumentaba el tiempo de desarrollo y la complejidad de deserialización en múltiples plataformas. Por otra parte, usar HTTP REST estándar para señalización introducía latencia de sondeo (polling) inaceptable.

## Decisión
Adoptar el estándar de mensajería **SignalR Hub Protocol sobre WebSockets** con codificación JSON y enmarcado mediante el carácter ASCII Record Separator `0x1E` (`\u{001e}`).

## Motivos
* **Compatibilidad Nativa con .NET:** Permite a `NxDesk.Client` usar directamente el paquete oficial `Microsoft.AspNetCore.SignalR.Client` sin adaptadores adicionales.
* **Fácil Implementación en Rust:** En Rust, `tokio-tungstenite` y `serde_json` permiten serializar y delimitar tramas con `\u{1e}` en pocas líneas de código sin overhead.
* **Soporte Híbrido:** Admite indistintamente mensajes de texto JSON (metadatos, roles, peticiones) y frames binarios (streams de video JPEG sin recodificación Base64).

## Consecuencias
### Positivas
* Interoperabilidad cruzada completa: Un cliente en C# puede conectarse a un servidor de señalización en Rust, y viceversa.
* Facilidad de depuración: Los mensajes JSON delimitados por `0x1E` son legibles por inspectores de red como Wireshark o logs estructurados.

### Trade-offs
* Ligero overhead comparado con protocolos binarios puros como Protobuf o MessagePack en los metadatos de control (despreciable frente al peso de los fotogramas de video).
