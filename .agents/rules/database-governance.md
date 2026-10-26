# Database & Persistence Governance Rules

Aplica a persistencia relacional, NoSQL y vectorial (PostgreSQL, PgVector, SQLite, MySQL, Redis, MongoDB).

## Reglas Obligatorias (ALWAYS DO)
1. **Migraciones Idempotentes y Versionadas:** Todo cambio de esquema DEBE residir en un archivo de migración versionado y reproducible. Prohibido ejecutar DDL manual no versionado.
2. **Pooling de Conexiones Mandatorio:** En entornos de alta concurrencia, utilizar connection poolers dedicados (PgBouncer en Transaction Mode, HikariCP, pool interno del driver con límites estrictos).
3. **Indexación y Plan de Ejecución (EXPLAIN):** Toda columna utilizada en cláusulas `WHERE`, `JOIN` o `ORDER BY` de consultas frecuentes DEBE poseer un índice explícito (B-Tree, GiST, GIN, HNSW para vectores).
4. **Consultas Solo-Lectura Optimizadas:** Usar consultas sin seguimiento de cambios (`AsNoTracking()` en EF Core, queries raw o projections) en operaciones de lectura.
5. **Paginación Mandatoria:** Toda consulta que retorne listas o colecciones DEBE estar paginada por cursor (Keyset) o por límite (`LIMIT / OFFSET`). Prohibido hacer queries `SELECT *` abiertas.

## Reglas Prohibidas (NEVER DO)
- ❌ **NEVER** permitir queries con problema N+1: usar eager loading explícito (`Include`, `JOIN`) o batch loading.
- ❌ **NEVER** ejecutar queries dentro de bucles (`for`, `foreach`, `while`).
- ❌ **NEVER** almacenar vectores o embeddings de alta dimensionalidad sin índices HNSW o IVFFlat.
- ❌ **NEVER** ejecutar transacciones de larga duración que mantengan bloqueos de tablas o registros mientras se espera I/O externo o llamadas a APIs.
