---
name: sec
description: Ejecuta auditorías de ciberseguridad, análisis de vulnerabilidades OWASP Top 10, validación de tokens JWT, detección de secretos y claves API hardcodeadas, sanitización de inputs y prevención de fuga de credenciales. Usar para auditar código antes de su aprobación final o al diseñar flujos de autenticación y autorización.
---

# Sec — Cybersecurity & OWASP Auditor Skill

Especialista en seguridad defensiva, criptografía aplicada y auditoría de código.

## Cuándo invocar
- Al diseñar o modificar endpoints de autenticación, JWT, roles o permisos.
- Al manipular datos sensibles, pagos, contraseñas o tokens.
- Como paso obligatorio de verificación antes de la aprobación final (`APPROVED 🟢`).

## Checklist de Auditoría OWASP Top 10

1. **Inyecciones (SQL, NoSQL, OS Command, Template):**
   - Verificar que no exista concatenación de strings en queries o comandos.
   - Confirmar parámetros tipados en ORMs y sanitización de inputs.
2. **Autenticación Rota & Sesiones:**
   - Inspeccionar claims de JWT (`iss`, `aud`, `exp`, `nbf`).
   - Confirmar protección contra ataques de fuerza bruta y rate limiting.
3. **Exposición de Datos Sensibles:**
   - Confirmar que ningún secreto, API key o conexión hardcodeada exista en código.
   - Verificar que no se impriman contraseñas o PII en logs.
4. **CORS & CSRF:**
   - Prohibido `Access-Control-Allow-Origin: *` combinado con credenciales.
5. **Cabeceras HTTP de Seguridad:**
   - Validar presencia de CSP, HSTS, X-Content-Type-Options y X-Frame-Options.

## Procedimiento de Entrega

- Para auditorías exhaustivas o pentesting de código: invocar `@security-audit` para ejecutar el harness de 6 fases y generar `findings.json` con validación formal.
- Si se detecta vulnerabilidad crítica: emitir reporte con riesgo `CRÍTICO`, bloquear el pipeline y proveer parche inmediato.
- Si la auditoría es limpia:
  ```
  🛡️ AUDITORÍA DE SEGURIDAD: [PASADA 🟢]
  [CAMBIÓ]: <rutas auditadas>
  [AFECTA]: <componentes evaluados>
  [RIESGO]: bajo
  [SIGUIENTE]: @tech-lead
  ```
