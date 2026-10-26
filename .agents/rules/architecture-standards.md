# Architecture & Layer Governance Rules

Aplica a cualquier proyecto (C# .NET, TypeScript/Node, Python, Go, Rust). Establece las fronteras arquitectónicas no negociables (Clean / Hexagonal / DDD).

## Reglas Obligatorias (ALWAYS DO)
1. **Dependency Rule Estricta:** Las dependencias de código fuente solo deben apuntar hacia adentro:
   `Presentation / UI / Controllers` ➔ `Application / UseCases` ➔ `Domain / Core`.
2. **Dominio Aislado y Agnóstico:** La capa de Dominio (Entidades, Value Objects, Domain Events) DEBE ser pura; prohibido referenciar frameworks de persistencia (EF Core, Prisma, TypeORM, SQLAlchemy) o librerías HTTP.
3. **Inversión de Dependencias (DIP):** Los adaptadores de infraestructura (DB, brokers, clientes HTTP externos) DEBEN implementar interfaces declaradas en Domain o Application.
4. **DTOs y Contratos Explícitos:** Toda comunicación entre capas o hacia clientes externos DEBE utilizar DTOs tipados. Prohibido exponer entidades de base de datos directamente en respuestas de API.
5. **Composición sobre Herencia:** Preferir interfaces segregadas y composición modular. Evitar jerarquías profundas de clases base.

## Reglas Prohibidas (NEVER DO)
- ❌ **NEVER** importar `Infrastructure` o `Data` desde la capa `Domain`.
- ❌ **NEVER** escribir lógica de negocio en controladores, vistas o manejadores de eventos UI.
- ❌ **NEVER** usar singletons mutables globales para almacenar estado de sesión o de usuario.
- ❌ **NEVER** omitir el principio de responsabilidad única (SRP): controladores y servicios no deben exceder 250 líneas.
