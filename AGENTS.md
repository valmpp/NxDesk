<!-- gitnexus:start -->
# GitNexus & Governance Rules — Antigravity IDE (v2)

Este proyecto está indexado y protegido por **GitNexus (Code Intelligence)** y **Archify (Interactive Architecture)**.

## Reglas Obligatorias (ALWAYS DO)

1. **MUST run impact before editing:**
   Antes de editar cualquier función, clase, entidad o contrato central, DEBES verificar su radio de impacto:
   `pnpm dlx gitnexus@latest impact "<NombreDelSimbolo>" --direction upstream --repo .`
   Reportar llamadores detectados y nivel de riesgo. Prohibido reemplazar el análisis de grafo por un simple grep.

2. **MUST analyze graph changes before committing:**
   Antes de dar por concluida una tarea o realizar commit:
   `pnpm dlx gitnexus@latest detect-changes --scope all --repo .`

3. **MUST warn on HIGH/CRITICAL risk:**
   Si el análisis de impacto o el diff arroja riesgo ALTO o CRÍTICO, advertir inmediatamente al usuario con la estrategia de mitigación.

4. **MUST use CAHP protocol on handovers:**
   Todo reporte de traspaso entre especialistas DEBE incluir el bloque canónico:
   ```text
   [CAMBIÓ]: <archivos modificados>
   [AFECTA]: <componentes impactados>
   [RIESGO]: bajo | medio | alto | crítico
   [SIGUIENTE]: <@agente>
   ```

5. **MUST ensure zero-error builds:**
   Prohibido declarar `STATUS: APPROVED 🟢` si existen errores de compilación, de types o linters pendientes.

## Reglas Prohibidas (NEVER DO)

- ❌ **NEVER** editar un contrato de API o entidad de dominio sin previo análisis de impacto en GitNexus.
- ❌ **NEVER** usar buscar-y-reemplazar masivo en código fuente sin comprender el grafo de llamadas.
- ❌ **NEVER** incluir secretos, credenciales o claves API en archivos rastreados por Git.
- ❌ **NEVER** dar por terminada una tarea dejando instrucciones manuales no ejecutadas para el usuario.
<!-- gitnexus:end -->
