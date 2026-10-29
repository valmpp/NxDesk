# DEV-RUN-001 — Fallo de Inicio STATUS_ACCESS_VIOLATION (0xc0000005) por Drivers Gráficos

| Código | DEV-RUN-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Low-Level Engineering |
| Estado | Aprobado | Clasificación | Runbook Operativo de Emergencia |

## Historial
| Versión | Fecha | Cambio | Autor |
|---------|-------|--------|-------|
| v1.0 | 2026-10-28 | Creación del runbook tras resolución de bug de GPU | NxDesk Tech Lead |

---

## 1. Cómo Detectar
* El ejecutable `nxdesk-client.exe` no abre ninguna ventana y termina inmediatamente.
* En la consola de PowerShell o CMD se muestra:
  `error: process didn't exit successfully: (Exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)`.
* En el Visor de Eventos de Windows (`eventvwr.msc`), Registro de Aplicación:
  `Nombre del módulo con errores: vulkan-1.dll` o `igvk64.dll`.

---

## 2. Qué Revisar Primero
1. Verificar si el binario fue compilado con `wgpu` o con el renderer `Glow`.
2. Inspeccionar `nxdesk-rust/apps/client/Cargo.toml`: la línea de `eframe` DEBE tener `default-features = false` y `features = ["glow"]`.
3. Comprobar si `app.rs` / `main.rs` especifica `renderer: eframe::Renderer::Glow`.

---

## 3. Pasos para Resolver
1. Abrir [nxdesk-rust/apps/client/Cargo.toml](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/client/Cargo.toml) y verificar la configuración:
   ```toml
   eframe = { version = "0.37.0", default-features = false, features = ["glow", "default_fonts", "accesskit"] }
   ```
2. Abrir [nxdesk-rust/apps/client/src/main.rs](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/nxdesk-rust/apps/client/src/main.rs) y asegurar que las opciones nativas usen Glow:
   ```rust
   let options = eframe::NativeOptions {
       renderer: eframe::Renderer::Glow,
       // ...
   };
   ```
3. Limpiar y recompilar el binario:
   ```powershell
   cargo clean -p nxdesk-client
   cargo build --release -p nxdesk-client
   ```

---

## 4. A quién Escalar
* **Rol:** Lead Graphics Engineer / Tech Lead.
* **Condición:** Si el fallo persiste en una tarjeta gráfica sin soporte OpenGL 2.1+.

---

## 5. Cómo Verificar
Ejecutar `.\target\release\nxdesk-client.exe`: la ventana principal en tema oscuro `#1E1E1E` debe abrirse en < 1.2 segundos mostrando el ID de AnyDesk sin errores en consola.
