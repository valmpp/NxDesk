# Eval Workflow

Valida la precisión en el enrutamiento de especialistas mediante baterías automatizadas de activación (Should-Trigger vs Should-Not-Trigger). Invocar con `/eval` o `@eval-harness`.

## Pasos

### Paso 1 — Selección del Objetivo
Identificar el agente a evaluar (ej. `arch`, `sec`, `db-master`, `ui-expert`, `tech-lead`) o seleccionar evaluación global.

### Paso 2 — Ejecutar el Runner Automatizado
Ejecutar el script evaluador:
```powershell
python v2/.agents/skills/eval-harness/scripts/run_eval.py --agent <nombre-agente>
```

### Paso 3 — Revisión de Métricas
Registrar el reporte JSON emitido:
- Casos Should-Trigger superados (Tasa de acierto objetivo: 100%).
- Casos Should-Not-Trigger superados (Tasa de aislamiento objetivo: 100%).
- Falsos Positivos / Falsos Negativos detectados.

### Paso 4 — Criterio de Aprobación
- Si precisión >= 90%: Estado `APPROVED 🟢`.
- Si precisión < 90%: Refinar el campo `description` en `SKILL.md` para ajustar las palabras clave sin modificar el cuerpo de la skill.
