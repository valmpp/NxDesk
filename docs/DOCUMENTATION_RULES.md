# 📐 Reglas del Sistema Documental — NxDesk

| Código | DOC-RULE-001 | Versión | v1.0 |
|--------|--------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Governance Team |
| Estado | Aprobado | Clasificación | Estándar de Equipo |

> **Filosofía Fundamental:** `docs/` no es un almacén de archivos — es la memoria técnica, operativa y funcional del proyecto. Si algo ocurrió y no está en `docs/`, para el equipo siguiente no ocurrió.

---

## Las 10 Reglas de Oro

### 1. Documentar el porqué, no solo el qué
Si una decisión técnica puede generar dudas a futuro, debe registrarse como un **ADR (Architecture Decision Record)** en `01-architecture/decisions/`. El código fuente dice *qué* hace el sistema; la documentación explica *por qué* se decidió de esa manera y qué alternativas fueron descartadas.

### 2. Todo problema importante termina en la Base de Conocimiento (KB)
El ciclo inmutable ante cualquier falla es:  
`BUG/INCidente` ➔ `Investigación y Mitigación` ➔ `Solución de Raíz Validada` ➔ `Artículo en 10-knowledge-base/`.  
Sin un artículo de KB asociado, el aprendizaje se diluye en la siguiente rotación de desarrolladores.

### 3. Diferenciar mitigación temporal de solución definitiva
Un parche o workaround nunca debe disfrazarse de solución terminada. Debe documentarse con la etiqueta de severidad correspondiente y mantenerse abierto hasta que se compruebe la causa raíz y se valide la prueba de regresión.

### 4. La Base de Conocimiento (KB) debe ser 100% accionable
Toda guía debe incluir: **Cuándo usarla**, **Diagnóstico exacto**, **Pasos de resolución**, **Resultado esperado**, **Qué NO hacer** y **Procedimiento de Rollback**. Una guía que no especifica el resultado esperado o cómo deshacer los cambios es inservible.

### 5. Markdown para conocimiento — HTML para visualización interactiva
* Arquitectura, contratos de API, bases de datos, flujos de seguridad e incidentes se escriben en **Markdown**.
* Mockups visuales, mapas topológicos interactivos y diagramas de red con búsqueda y filtros se alojan en formato **HTML autónomo** dentro de `09-visual-docs/`.

### 6. Los documentos se enlazan entre sí de forma bidireccional
La documentación es una red de conocimiento viviente, no un listado plano de ficheros:
* Un reporte de Bug (`BUG-XXX`) enlaza a la guía de KB que originó.
* Un módulo en `02-modules/` enlaza a sus flujos en `03-flows/` y a sus ADRs correspondientes.
* Un flujo enlaza a las integraciones de bajo nivel en `05-integrations/`.

### 7. Cero secretos o credenciales reales en `docs/`
Está terminantemente prohibido incluir tokens, claves privadas, hashes de contraseñas de producción o credenciales en los documentos. Solo se documenta el nombre de la variable (`ACCESS_PASSWORD`, `BIND_ADDR`), su formato esperado y el mecanismo de configuración seguro.

### 8. Encabezado de Control de Versión en todo documento
Todo documento debe iniciar obligatoriamente con el bloque de metadatos institucional:
```markdown
| Código | [CODIGO-XXX] | Versión | v1.0 |
|--------|--------------|---------|------|
| Actualizado | YYYY-MM-DD | Autor | [Nombre/Rol] |
| Estado | Aprobado / Borrador | Clasificación | [Pública / Interna] |
```

### 9. Actualización obligatoria junto al cambio de código
Si un Pull Request o commit modifica un contrato, flujo o interfaz gráfica, la documentación asociada debe actualizarse en el **mismo commit**. Código actualizado con documentación desactualizada es considerado deuda técnica crítica.

### 10. La documentación es un proceso continuo
La documentación no es una tarea de fin de ciclo ni un trámite burocrático: crece, se depura y se refactoriza orgánicamente junto con cada refactorización del código.
