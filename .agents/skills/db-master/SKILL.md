---
name: db-master
description: Diseña y valida esquemas de base de datos relacionales, NoSQL y vectoriales (PostgreSQL, PgVector, MySQL, SQLite, Redis), migraciones idempotentes, pooling de conexiones y optimización de planes de ejecución (EXPLAIN). Usar para modelar persistencia o resolver cuellos de botella de datos.
---

# DB Master — Database & Persistence Architect Skill

Especialista en persistencia de datos, consultas de alto rendimiento y escalabilidad de almacenamiento.

## Cuándo invocar
- Al crear o modificar entidades de base de datos, tablas, índices o relaciones.
- Para diseñar migraciones idempotentes y reproducibles.
- Para optimizar consultas lentas o resolver problemas N+1.
- Al configurar búsqueda vectorial con PgVector o pooling con PgBouncer.

## Procedimiento de Ejecución

1. **Modelado Relacional / NoSQL:**
   - Diseñar DDL con tipos de datos óptimos (ej. `UUIDv7 / BIGINT`, `TIMESTAMPTZ`, `JSONB`).
   - Declarar claves foráneas, restricciones de unicidad y reglas de borrado (`CASCADE / RESTRICT`).
2. **Estrategia de Indexación:**
   - Crear índices B-Tree para claves foráneas y columnas de filtro frecuente.
   - Para vectores de embeddings: configurar índice `HNSW` con función de distancia adecuada (`vector_cosine_ops` o `vector_l2_ops`).
3. **Validación de Consultas & Pooling:**
   - Confirmar que las queries de lectura usen proyecciones específicas (evitar `SELECT *`).
   - Verificar límites de conexión y compatibilidad con PgBouncer en Transaction Mode (evitar `LISTEN/NOTIFY` no desacoplado).
4. **Generación de Migración:**
   - Entregar script SQL idempotente con sección UP y DOWN (rollback).
5. **Handover CAHP:**
   ```
   [CAMBIÓ]: <ruta de migraciones / esquemas generados>
   [AFECTA]: <repositorios y entidades de dominio>
   [RIESGO]: bajo | medio | alto
   [SIGUIENTE]: @tech-lead, @api-core
   ```
