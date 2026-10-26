# Build Module Workflow (DAG Model)

Construye módulos o funcionalidades completas end-to-end de manera autónoma, coordinando especialistas en un Grafo Acíclico Dirigido (DAG) concurrente sin interacción manual requerida del usuario.

## Pasos de Ejecución

### Fase 1 — Contratos y Arquitectura
1. Invocar `@arch` para diseñar la separación de capas y declarar interfaces públicas y DTOs tipados.
2. Congelar contratos de comunicación entre Backend, Persistencia y Frontend.

### Fase 2 — Implementación Concurrente (Paralelizable)
- **Persistencia:** Invocar `@db-master` para generar el modelo DDL, índices y script de migración versionado.
- **Backend:** Invocar `@api-core` para implementar endpoints, controladores, validación de entrada e inyección de dependencias.
- **Frontend:** Invocar `@ui-expert` para implementar pantallas, componentes reactivos, temas y accesibilidad WCAG AA.

### Fase 3 — Verificación de Integridad, Seguridad y Grafo
1. Invocar `@sec` para auditar el checklist OWASP Top 10 sobre todo el código generado.
2. Invocar `@gitnexus` para ejecutar:
   - `pnpm dlx gitnexus@latest impact "<SímboloPrincipal>" --direction upstream`
   - `pnpm dlx gitnexus@latest detect-changes --scope all`
3. Si el riesgo es ALTO o CRÍTICO, resolver dependencias antes de continuar.

### Fase 4 — Documentación Visual Interactiva
1. Invocar `@archify` para actualizar la especificación JSON-IR (`docs/architecture/*.architecture.json`).
2. Compilar el diagrama interactivo a HTML con `archify deliver`.

### Fase 5 — Certificación y Aprobación
Emitir bloque final de entrega:
```text
🛡️ STATUS: [APPROVED 🟢]
🚀 Módulo completado, validado por GitNexus y certificado por OWASP. Sin acción requerida del usuario.
```
