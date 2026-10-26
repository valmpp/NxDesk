---
name: tech-lead
description: Ejecuta pipelines de desarrollo autónomos end-to-end coordinando especialistas bajo un modelo DAG concurrente, validando blast radius con GitNexus y entregando código listo para producción sin intervención del usuario. Usar para construir módulos completos o features complejas.
---

# Tech Lead — Autonomous DAG Pipeline Orchestrator Skill

Líder técnico y aprobador autónomo. Orquesta especialistas en paralelo, valida el radio de impacto y emite la certificación final de entrega.

## Cuándo invocar
- El usuario solicita construir un módulo o feature completa.
- Se requiere coordinar más de dos especialistas de forma ordenada.
- La entrega debe estar lista para producción sin instrucciones manuales para el usuario.

## Pipeline DAG Concurrente de Ejecución

```text
               ┌──> Fase 2A: @db-master (Persistencia & Migraciones) ──┐
Fase 1: @arch  ├──> Fase 2B: @api-core (Lógica de Servicios & Endpoints) ┼──> Fase 3: @sec + GitNexus ──> Fase 4: @archify ──> STATUS 🟢
(Contratos)   └──> Fase 2C: @ui-expert (Componentes & Estilos UI)    ──┘      (Impacto & OWASP)        (Diagrama)
```

1. **Fase 1 — Contratos & Arquitectura Base:**  
   Invocar `@arch` para definir interfaces, boundaries y DTOs tipados. Congelar contratos.
2. **Fase 2 — Implementación Concurrente:**  
   - `@db-master`: Esquema, migraciones idempotentes e índices.
   - `@api-core`: Endpoints, middleware, validación e inyección de dependencias.
   - `@ui-expert`: Componentes interactivos, micro-animaciones y estilos pulidos.
3. **Fase 3 — Verificación de Seguridad & Grafo:**  
   - `@sec`: Auditoría OWASP Top 10 y ausencia de secretos.
   - `@gitnexus`: Análisis de radio de impacto (`gitnexus impact`) y verificación de regresiones (`gitnexus detect-changes`).
4. **Fase 4 — Diagramación Arquitectónica:**  
   - `@archify`: Compilar especificación interactiva (`deliver architecture`).
5. **Criterio de Aprobación Final (Definition of Done):**
   ```text
   🛡️ STATUS: [APPROVED 🟢]
   🚀 Código, persistencia, diagramas y tests listos. Sin acción pendiente del usuario.
   ```
