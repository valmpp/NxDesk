---
name: gitnexus
description: Consulta el Grafo de Conocimiento (Knowledge Graph) AST del código fuente generado por GitNexus. Permite evaluar el radio de impacto (Blast Radius) antes de modificar símbolos, detectar llamadas upstream/downstream y mapear diffs de Git a flujos de ejecución. Usar antes de refactorizaciones o commits.
---

# GitNexus — Code Intelligence & Knowledge Graph Skill

Especialista en análisis de dependencias de código a nivel de AST y prevención de regresiones arquitectónicas.

## Cuándo invocar
- **ANTES** de editar una clase, función, interfaz o controlador central para conocer qué componentes se romperán.
- **ANTES** de commitear o finalizar una tarea para mapear los cambios realizados contra el grafo de ejecución.
- Para realizar consultas semánticas o Cypher sobre la estructura del código.

## Comandos Operativos

1. **Análisis de Impacto (Blast Radius):**
   ```bash
   pnpm dlx gitnexus@latest impact "<NombreDelSimbolo>" --direction upstream --repo .
   ```
   *Dirección:*
   - `upstream`: Qué llamadores / consumidores se ven afectados si cambio este símbolo.
   - `downstream`: De qué dependencias o servicios depende este símbolo.

2. **Detección de Cambios en Grafo (Pre-Commit):**
   ```bash
   pnpm dlx gitnexus@latest detect-changes --scope all --repo .
   ```

3. **Contexto 360° de un Símbolo:**
   ```bash
   pnpm dlx gitnexus@latest context "<NombreDelSimbolo>" --repo .
   ```

4. **Re-Indexación tras Modificaciones:**
   ```bash
   pnpm dlx gitnexus@latest analyze --index-only
   ```

## Handover CAHP
```
[CAMBIÓ]: <símbolos modificados>
[AFECTA]: <llamadores detectados por GitNexus>
[RIESGO]: bajo | medio | alto | crítico
[SIGUIENTE]: @tech-lead
```
