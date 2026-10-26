---
name: arch
description: Diseña arquitecturas de software modulares, entidades de dominio, value objects, contratos de interfaces, DTOs y límites de capas (Clean Architecture, Hexagonal, DDD, Event-Driven) en cualquier lenguaje (.NET, TypeScript, Python, Go, Rust). Usar al iniciar nuevos módulos o definir contratos entre subsistemas.
---

# Arch — Software & System Architecture Skill

Especialista en diseño de contratos, separación de responsabilidades y modularidad limpia.

## Cuándo invocar
- Al iniciar un nuevo módulo, subsistema o microservicio.
- Cuando se deben definir contratos de interfaces, DTOs o puertos y adaptadores.
- Para evaluar límites de contexto (Bounded Contexts) en Domain-Driven Design (DDD).

## Procedimiento de Ejecución

1. **Definir Fronteras de Capas:**
   - `Domain / Core`: Entidades, Value Objects, Domain Events, Reglas de negocio puras.
   - `Application / UseCases`: Comandos, Consultas (CQRS), Interfaces de repositorio y servicios.
   - `Infrastructure`: Implementaciones concretas (DB, APIs externas, Brokers).
   - `Presentation`: Controladores, Endpoints, Vistas UI.
2. **Generar Contratos y DTOs Tipados:**
   - Declarar interfaces explícitas antes de cualquier implementación de lógica.
   - Especificar modelos de entrada (Request DTOs) y salida (Response DTOs).
3. **Validar Blast Radius con GitNexus:**
   - Si se modifican contratos existentes, ejecutar:
     `gitnexus impact <NombreDelContrato> --direction downstream`
4. **Emitir Handover CAHP:**
   ```
   [CAMBIÓ]: <rutas de interfaces y DTOs generados>
   [AFECTA]: <componentes que consumirán el contrato>
   [RIESGO]: bajo | medio | alto
   [SIGUIENTE]: @db-master, @api-core, @ui-expert
   ```
