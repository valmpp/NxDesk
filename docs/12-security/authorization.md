# 🛡️ Seguridad: Modelo de Autorización y Roles Granulares — NxDesk

| Código | SEC-ATH-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Matriz de Control de Acceso por Roles (RBAC)

Cada sesión activa (`ActiveSessionInfo`) tiene asignado de forma inmutable un rol específico negociado durante el handshake:

| Capacidad / Privilegio | Rol `ViewOnly` | Rol `FileOnly` | Rol `FullControl` |
| :--- | :---: | :---: | :---: |
| **Recepción de Pantalla Remota (Video JPEG)** | ✅ Sí | ❌ No | ✅ Sí |
| **Inyección de Ratón y Teclado (`SendInput`)** | ❌ No | ❌ No | ✅ Sí |
| **Subida y Descarga de Archivos (Sandbox)** | ❌ No | ✅ Sí | ✅ Sí |
| **Portapapeles Compartido** | ❌ No | ❌ No | ✅ Sí (configurable) |
| **Terminal de Diagnóstico en Línea Blanca** | ❌ No | ❌ No | ✅ Sí |

---

## 2. Puntos de Ejecución de Políticas (PEPs)

1. **`main.rs:276` (Filtro de Entrada):**  
   Verifica atómicamente `active.is_some_and(|s| s.allow_input)` antes de invocar `injector.inject(&input_evt)`.
2. **`main.rs:290` (Filtro de Terminal):**  
   Verifica que `active.is_some_and(|s| s.granted_role == SessionRole::FullControl)`.
3. **`FileTransferService.cs:75` (Filtro de Archivos):**  
   Verifica que la sesión tenga `allow_file_transfer == true` y que la ruta pertenezca estrictamente al sandbox.
