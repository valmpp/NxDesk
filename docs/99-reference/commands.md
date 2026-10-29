# ⚡ Referencia Rápida: Comandos Útiles — NxDesk

| Código | REF-CMD-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Ecosistema Rust (`nxdesk-rust/`)

```powershell
# Ejecutar el servidor de señalización local
cargo run -p nxdesk-server

# Ejecutar el cliente de escritorio con alias personalizado
cargo run -p nxdesk-client -- --alias="Mi-Equipo" --device-id="111-222-333"

# Compilar binarios de producción optimizados (Release)
cargo build --release

# Ejecutar toda la suite de pruebas unitarias automatizadas
cargo test --workspace

# Ejecutar el análisis estático estricto (Clippy sin advertencias)
cargo clippy --workspace -- -D warnings

# Podar dependencias y auditar árbol de crates
cargo tree -d
```

---

## 2. Ecosistema C# (.NET 10.0)

```powershell
# Compilar la solución completa (.NET 10)
dotnet build NxDesk.slnx

# Ejecutar el cliente de escritorio en Avalonia
dotnet run --project src/NxDesk.Client/NxDesk.Client.csproj

# Ejecutar el servidor de señalización SignalR
dotnet run --project src/NxDesk.Server/NxDesk.Server.csproj

# Auditar vulnerabilidades en paquetes NuGet
dotnet list package --vulnerable
```
