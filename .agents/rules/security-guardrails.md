# Security & Zero-Trust Guardrails

Aplica transversalmente a todas las capas del sistema y pipelines de integración.

## Reglas Obligatorias (ALWAYS DO)
1. **Defensa contra OWASP Top 10:**
   - **Inyecciones:** Uso estricto de parámetros parametrizados / ORMs seguros. Prohibida la concatenación de strings en SQL, shell o comandos del SO.
   - **Broken Auth:** Validación estricta de firma, expiración (`exp`), audiencia (`aud`) y emisor (`iss`) de tokens JWT. Rotación segura de refresh tokens.
   - **XSS & Injection UI:** Sanitización y escape automático de outputs dinámicos en templates.
2. **Principio de Menor Privilegio:** Servicios y conexiones de base de datos deben utilizar credenciales con el mínimo alcance requerido para su operación.
3. **Gestión de Secretos:** Claves API, tokens, strings de conexión y certificados DEBEN provenir de variables de entorno o gestores de secretos (Vault, Azure Key Vault, AWS Secrets Manager).
4. **Cabeceras de Seguridad HTTP:** Configurar obligatoriamente HSTS, Content-Security-Policy (CSP), X-Frame-Options (`DENY` o `SAMEORIGIN`), X-Content-Type-Options (`nosniff`).
5. **Rate Limiting & Throttling:** Todo endpoint público de login, registro, pago o generación intensiva por IA DEBE tener limitador de tasa de peticiones.

## Reglas Prohibidas (NEVER DO)
- ❌ **NEVER** incluir secretos, tokens, contraseñas o llaves privadas en código fuente o commits de Git.
- ❌ **NEVER** deshabilitar la validación de certificados SSL/TLS (`RejectUnauthorized = false` o `ServerCertificateCustomValidationCallback = AlwaysAccept`).
- ❌ **NEVER** configurar CORS con `AllowAnyOrigin (*)` combinado con `AllowCredentials (true)`.
- ❌ **NEVER** loggear datos personales (PII), números de tarjeta, passwords o tokens en texto plano.
