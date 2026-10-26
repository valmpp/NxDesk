---
name: archify
description: Genera, valida y compila diagramas de arquitectura interactivos en HTML autónomo con SVG vectorial, modo claro/oscuro y búsqueda de nodos (motor Archify). Usar para documentar visualmente la arquitectura del sistema, topologías de red, flujos de datos o secuencias de llamadas.
---

# Archify — Interactive Architecture Engine Skill

Especialista en compilación de especificaciones declarativas JSON-IR en diagramas HTML interactivos autónomos (cero dependencias externas, compilación ultra-rápida en ~265 ms).

## Cuándo invocar
- Al diseñar o actualizar el mapa de arquitectura de servicios o microservicios.
- Para documentar visualmente flujos de llamadas o secuencias HTTP/WebSockets.
- Para generar entregables visuales interactivos para READMEs o auditorías.

## Rutas del Motor Archify
* **Binario CLI:** `tools/archify/archify/archify/bin/archify.mjs`
* **Wrapper PowerShell:** `tools/archify/archify.ps1`

## Procedimiento de Compilación

1. **Diseñar o Actualizar la Especificación JSON-IR (`*.architecture.json`):**
   - Declarar componentes con `id`, `type` (external, frontend, service, database, broker), `label`, `pos`, `size`.
   - Declarar conexiones tipadas con `from`, `to`, `label`, `protocol`.
   - Declarar boundaries y vistas enfocadas (`views`).

2. **Validar la Especificación:**
   ```powershell
   node tools/archify/archify/archify/bin/archify.mjs validate architecture <archivo.json> --quality showcase --json
   ```

3. **Compilar a HTML Autónomo:**
   ```powershell
   node tools/archify/archify/archify/bin/archify.mjs deliver architecture <archivo.json> <salida.html> --quality showcase --json
   ```

4. **Handover CAHP:**
   ```
   [CAMBIÓ]: <ruta del archivo .architecture.json y .architecture.html>
   [AFECTA]: <documentación visual del sistema>
   [RIESGO]: bajo
   [SIGUIENTE]: @tech-lead
   ```
