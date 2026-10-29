# 🔍 Estándares de Linter y Análisis Estático (Clippy) — NxDesk

| Código | TST-CLP-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk QA & Governance Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Política de Cero Advertencias (`-D warnings`)

En el repositorio NxDesk se exige que el compilador y el linter Clippy no emitan **ninguna advertencia**. Cualquier advertencia se promueve automáticamente a error de compilación:

```powershell
cargo clippy --workspace -- -D warnings
```

---

## 2. Reglas Clippy Clave Implementadas

* **`clippy::unnecessary-map-or`:**  
  En lugar de patrones verbosos como `active.as_ref().map_or(false, |s| s.allow_input)`, se exige el uso idiomático de:
  `active.as_ref().is_some_and(|s| s.allow_input)`.
* **`clippy::needless_borrow`:**  
  Evitar préstamos redundantes (`&`) en referencias que ya son punteros compartidos.
* **`clippy::redundant_clone`:**  
  Eliminar clonaciones profundas de memoria en fotogramas o estructuras grandes cuando la propiedad (`move`) puede transferirse de forma limpia.
