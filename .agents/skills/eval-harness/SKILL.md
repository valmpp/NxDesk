---
name: eval-harness
description: Ejecuta baterías de pruebas de precisión y activación (Should-Trigger / Should-Not-Trigger) para validar agentes, skills y reglas en Antigravity IDE. Calcula tasas de precisión, falsos positivos y falsos negativos. Usar para validar cambios en descripciones o antes de promover una versión.
---

# Eval Harness — Agent Activation & Precision Testing Skill

Herramienta de benchmarking automatizado para medir la precisión en el enrutamiento de agentes y descartar alucinaciones o sobre-activación.

## Cuándo invocar
- Al modificar el `description` de una skill o las reglas del workspace.
- Antes de subir de versión en el versionamiento semántico (`/versionar`).
- Cuando se sospeche de conflictos de activación o solapamientos entre especialistas.

## Métricas Evaluadas
* **Should-Trigger:** Consultas que DEBEN activar al especialista.
* **Should-Not-Trigger:** Consultas adyacentes o de otros dominios que NO deben activar al especialista.
* **Falso Positivo:** El agente se activa para un dominio que no le corresponde.
* **Falso Negativo:** El agente no se activa ante una consulta de su competencia.

## Ejecución del Runner Automatizado
```powershell
python v2/.agents/skills/eval-harness/scripts/run_eval.py --agent <nombre-agente>
```

## Criterio de Promoción
Un agente se considera calificado para producción únicamente con **100% de precisión** (0 falsos positivos, 0 falsos negativos).
