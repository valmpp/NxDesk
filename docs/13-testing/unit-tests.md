# 🧪 Catálogo de Pruebas Unitarias Automatizadas — NxDesk

| Código | TST-UNT-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk QA & Testing Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Suite de Pruebas en Rust (`cargo test --workspace`)

Total de pruebas unitarias automatizadas: **13 aprobadas (0 falladas, 0 ignoradas)**.

| Módulo / Crate | Test | Descripción y Aserción |
| :--- | :--- | :--- |
| **`nxdesk_capture`** | `test_screen_capturer_creation` | Verifica la inicialización del contexto GDI y dimensiones de pantalla. |
| **`nxdesk_capture`** | `test_get_screens_detection` | Detecta los monitores físicos conectados al equipo. |
| **`nxdesk_capture`** | `test_to_rgb_into_buffer_conversion` | Valida la transformación de búfer BGRA a RGB sin desbordamientos. |
| **`nxdesk_client`** | `test_app_state_initialization` | Comprueba que el estado inicial no tenga sesiones activas huérfanas. |
| **`nxdesk_client`** | `test_anydesk_session_handshake_flow` | Simula la petición remota, aprobación del host y verificación de token. |
| **`nxdesk_client`** | `test_auth_manager_with_password` | Valida el rechazo de contraseñas incorrectas y rate-limiting. |
| **`nxdesk_file_transfer`** | `test_path_traversal_blocked` | **Seguridad:** Confirma que intentos de escape `../../` arrojen error. |
| **`nxdesk_file_transfer`** | `test_list_directory_returns_items` | Lista únicamente los ficheros autorizados en el sandbox. |
| **`nxdesk_file_transfer`** | `test_transfer_manager_write_and_complete`| Verifica el ensamblado secuencial de chunks en disco. |
| **`nxdesk_terminal`** | `test_vul01_command_validation_allows_safe_authorized` | Valida que comandos en lista blanca se ejecuten con rol `FullControl`. |
| **`nxdesk_terminal`** | `test_vul01_command_validation_blocks_destructive` | **Seguridad:** Bloquea comandos destructivos (`format`, `del /f`). |
| **`nxdesk_terminal`** | `test_vul01_command_validation_blocks_unauthorized` | **Seguridad:** Bloquea comandos de terminal si el rol es `ViewOnly`. |
| **`nxdesk_terminal`** | `test_terminal_session_lifecycle` | Valida el inicio y cierre limpio del subproceso de shell. |

---

## 2. Ejecución Local de la Suite

```powershell
cd nxdesk-rust
cargo test --workspace -- --nocapture
```
