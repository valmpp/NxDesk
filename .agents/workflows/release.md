# Release & Versioning Workflow

Gestiona el versionamiento semántico (SemVer), changelogs y planes de rollback ante modificaciones en la estructura de agentes o actualizaciones de Antigravity IDE. Invocar con `/release` o `/versionar`.

## Pasos

### Paso 1 — Clasificación del Cambio
- `feat(scope):` Nueva capacidad, skill o integración incorporada al ecosistema.
- `fix(scope):` Corrección de triggers, desambiguación o ajustes de reglas.
- `chore(ide-sync):` Adaptación de rutas o plugins por actualización del IDE.

### Paso 2 — Mensaje de Commit Estructurado
```text
<tipo>(<alcance>): <descripción imperativa breve>

[OPCIONAL] Justificación del cambio y análisis de impacto.
[OPCIONAL] BREAKING CHANGE: detalle de incompatibilidades si aplica.
```

### Paso 3 — Actualización de CHANGELOG
Registrar el bloque en el historial del repositorio:
```markdown
## [vX.Y.Z] - YYYY-MM-DD
### Added
- Nuevas skills o workflows disponibles.
### Changed
- Refactorizaciones o ajustes en reglas.
### Fixed
- Corrección de triggers o rutas.
```

### Paso 4 — Punto de Rollback y Validación
1. Confirmar commit hash de restauración previo.
2. Ejecutar `/eval` para garantizar 0 regresiones antes del tag o merge.
