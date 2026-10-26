---
name: api-core
description: Diseña e implementa APIs RESTful, Minimal APIs, endpoints gRPC, GraphQL y canales en tiempo real (WebSockets / SignalR) en cualquier stack (.NET, Node/TS, Python, Go, Rust). Especialista en middleware, validación tipada y manejo global de errores. Usar para lógica de servicios backend.
---

# API Core — Backend Services & Gateway Skill

Especialista en desarrollo de servicios de backend robustos, contratos de comunicación y pasarelas de integración.

## Cuándo invocar
- Al implementar controladores, endpoints HTTP o routers de API.
- Para configurar middleware de autenticación JWT, rate limiting o compresión.
- Para implementar comunicación en tiempo real con WebSockets o SignalR.
- Para configurar reverse proxies o gateways (YARP, NGINX, Traefik, Envoy).

## Procedimiento de Ejecución

1. **Implementación de Endpoints:**
   - Agrupar rutas por recurso semántico (`/api/v1/recurso`).
   - Aplicar validación de esquemas tipados en el payload entrante antes de invocar la lógica de negocio.
   - Retornar códigos de estado HTTP semánticos y estructurados (`ProblemDetails` en caso de error).
2. **Inyección de Dependencias y Cancelación:**
   - Inyectar interfaces de casos de uso (Application Layer) de forma limpia.
   - Pasar tokens o contextos de cancelación en cada llamada I/O.
3. **Resiliencia & Telemetría:**
   - Configurar políticas de reintento con backoff exponencial y circuit breaker en clientes HTTP salientes.
   - Agregar logging estructurado y correlación de peticiones (`Correlation-ID`).
4. **Handover CAHP:**
   ```
   [CAMBIÓ]: <rutas de endpoints y middleware>
   [AFECTA]: <clientes frontend y contratos de API>
   [RIESGO]: bajo | medio
   [SIGUIENTE]: @ui-expert, @sec, @tech-lead
   ```
