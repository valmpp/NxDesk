# API & Backend Services Governance Rules

Aplica a servicios de backend, endpoints RESTful, GraphQL, gRPC y WebSockets/SignalR en cualquier stack (.NET, Node/TS, Python, Go, Rust).

## Reglas Obligatorias (ALWAYS DO)
1. **Preferir Minimal & Grouped Endpoints:** Agrupar endpoints por recurso con validación tipada en la entrada (FluentValidation, Zod, Pydantic).
2. **Manejo Centralizado de Excepciones:** Utilizar middleware global o Problem Details (RFC 7807) para capturar y normalizar errores HTTP. Nunca devolver stack traces al cliente.
3. **Estandarización de Códigos de Estado:**
   - Creación: `201 Created` con cabecera `Location`.
   - Consulta vacía por ID: `404 Not Found`.
   - Error de validación: `400 Bad Request` o `422 Unprocessable Entity` con diccionario de campos.
   - Acceso no autorizado: `401 Unauthorized` / `403 Forbidden`.
4. **Timeouts y Resiliencia en I/O:** Todo endpoint o llamada a servicios externos DEBE admitir cancelación (`CancellationToken` en C#, `AbortSignal` en JS/TS, `context.Context` en Go) y tener timeout explícito.
5. **Real-Time & WebSockets:** Autenticar en el handshake inicial; canalizar mensajes mediante grupos acotados (Hubs / Rooms), nunca por broadcast global indiscriminado.

## Reglas Prohibidas (NEVER DO)
- ❌ **NEVER** retornar `200 OK` en respuestas que contienen un payload con errores de fallo.
- ❌ **NEVER** procesar requests sin validación de esquema en el cuerpo (Body) y parámetros de consulta (Query).
- ❌ **NEVER** bloquear hilos con llamadas síncronas sobre I/O (`.Result`, `.Wait()`, `readFileSync` en rutas calientes).
