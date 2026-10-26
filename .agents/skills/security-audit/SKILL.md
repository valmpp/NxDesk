---
name: security-audit
description: Auditoría profunda de vulnerabilidades basada en el harness de seguridad de Cloudflare. Ejecuta un proceso en 6 fases (reconocimiento, caza dirigida por cobertura, validación adversarial de candidatos, salida estructurada y reportes neutrales) contra clases de ataque web, autenticación, inyección, cliente y memoria. Usar para auditorías exhaustivas de seguridad o pentesting de código.
---

# Security Audit — Cloudflare Vulnerability Discovery Harness Skill

Framework de auditoría de seguridad exhaustiva y descubrimiento riguroso de vulnerabilidades en código fuente.

## Cuándo invocar
- Se solicita una auditoría integral o pen-test sobre un repositorio o módulo completo.
- Se requiere descubrir fallas no evidentes mediante clases de ataque estructuradas.
- Se debe generar un reporte formal con esquema JSON (`findings.json`) y validación adversarial.

## Las 6 Fases del Harness de Auditoría

```text
1. Reconocimiento ──> 2. Caza por Cobertura ──> 3. Validación Adversarial ──> 4. findings.json ──> 5. Verificación ──> 6. REPORT.md
   (Mapeo Superficies)  (Clases de Ataque)       (Desmentir Candidatos)        (Validator CJS)       (Independiente)    (Neutral)
```

### Fase 1 — Reconocimiento y Límites de Confianza
- Mapear puntos de entrada, superficies no autenticadas y límites de confianza.
- Consultar guía de reconocimiento: [`references/RECONNAISSANCE.md`](./references/RECONNAISSANCE.md).
- Generar `coverage-ledger.json` y verificar con el validador:
  ```bash
  node ./scripts/validate-coverage-ledger.cjs coverage-ledger.json
  ```

### Fase 2 — Caza Dirigida por Cobertura
- Asignar unidades de caza aisladas e inspeccionar contra las guías de ataque especializadas:
  - Protocolos Web y Autenticación: [`references/WEB-PROTOCOL-AND-AUTH.md`](./references/WEB-PROTOCOL-AND-AUTH.md)
  - Clientes, DOM e Inyección: [`references/CLIENT-SIDE.md`](./references/CLIENT-SIDE.md)
  - Clases Generales de Ataque: [`references/ATTACK-CLASSES.md`](./references/ATTACK-CLASSES.md)
  - Aislamiento de Datos y Ciclo de Vida: [`references/DATA-ISOLATION-AND-LIFECYCLE.md`](./references/DATA-ISOLATION-AND-LIFECYCLE.md)
  - Modelos AI y LLMs: [`references/AI-AND-LLM.md`](./references/AI-AND-LLM.md)
  - Agotamiento de Recursos / DoS: [`references/RESOURCE-EXHAUSTION-AND-AVAILABILITY.md`](./references/RESOURCE-EXHAUSTION-AND-AVAILABILITY.md)
- Metodología de caza: [`references/HUNTING.md`](./references/HUNTING.md).

### Fase 3 — Validación Adversarial de Candidatos
- **Regla de Oro:** El agente que verifica una vulnerabilidad nunca es el que la descubrió.
- Intentar desmentir activamente la hipótesis de vulnerabilidad buscando defensas previas o sanitizaciones intermedias.
- Solo confirmar si existe una traza de código completa desde la entrada no confiable hasta el sumidero (sink).

### Fase 4 — Salida Estructurada (`findings.json`)
- Escribir hallazgos clasificados en:
  - `confirmed`: Traza probada y resultado verificable.
  - `needs_validation`: Hecho específico no resuelto sin severidad asignada.
  - `rejected`: Candidato refutado con evidencia.
- Validar contra el esquema oficial:
  ```bash
  node ./scripts/validate-findings.cjs findings.json ./resources/report-schema.json
  ```

### Fase 5 — Verificación Independiente de Registros
- Verificar que las afirmaciones en el código fuente coincidan exactamente con las líneas y archivos actuales del repositorio.

### Fase 6 — Generación de Reportes Neutrales
- Derivar `REPORT.md`, `FINDINGS-DETAIL.md` y `NEEDS-VALIDATION.md` basados en los registros aprobados.
- Especificación de reportes: [`references/VALIDATION-AND-REPORTING.md`](./references/VALIDATION-AND-REPORTING.md).

## Handover CAHP
```text
[CAMBIÓ]: <rutas de findings.json y REPORT.md>
[AFECTA]: <vulnerabilidades confirmadas y componentes afectados>
[RIESGO]: bajo | medio | alto | crítico
[SIGUIENTE]: @sec, @tech-lead
```
