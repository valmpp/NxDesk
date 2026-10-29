# 🛡️ Política y Mecanismo del Sandbox de Archivos — NxDesk

| Código | MOD-FLT-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. La Amenaza: Path Traversal (CWE-22)

En software de escritorio remoto sin confinamiento, un atacante o cliente no confiable puede solicitar la descarga de archivos críticos del sistema operativo enviando secuencias de escape como `../../../../Windows/System32/config/SAM` o `../../../../etc/shadow`. Asimismo, podría sobrescribir ejecutables del sistema en `C:\Program Files` mediante solicitudes de escritura manipuladas.

---

## 2. Implementación del Sandbox Hermético

NxDesk confina **todas** las operaciones del sistema de archivos a un directorio raíz inmutable:

* **Ruta Base en Windows:** `%USERPROFILE%\Downloads\NxDesk_Transfers`
* **Ruta Base en Linux:** `$HOME/Downloads/NxDesk_Transfers`

### Algoritmo Canónico de Validación (`ValidateAndResolveSafePath`)

Tanto en C# (`FileTransferService.cs`) como en Rust (`crates/file-transfer/src/lib.rs`), se aplica el siguiente algoritmo defensivo:

1. **Extracción Estricta del Nombre de Fichero:** Se descartan carpetas previas usando `Path.GetFileName(filename)` para evitar rutas relativas compuestas.
2. **Normalización Canónica Absoluta:**
   ```csharp
   string sandbox = Path.GetFullPath(SandboxBasePath);
   string combined = Path.IsPathRooted(requestedPath)
       ? requestedPath
       : Path.Combine(sandbox, requestedPath.TrimStart('/', '\\'));
   string fullPath = Path.GetFullPath(combined);
   ```
3. **Verificación de Prefijo Inmutable:**
   ```csharp
   if (!fullPath.Equals(sandbox, StringComparison.OrdinalIgnoreCase) &&
       !fullPath.StartsWith(sandbox + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
   {
       throw new UnauthorizedAccessException($"[Seguridad] Intento de acceso fuera del Sandbox bloqueado: '{requestedPath}'");
   }
   ```

---

## 3. Matriz de Comportamiento Defensivo

| Ruta Solicitada por el Par Remoto | Resultado | Excepción / Código |
| :--- | :---: | :--- |
| `archivo.txt` | 🟢 Permitido | Resuelve a `%USERPROFILE%\Downloads\NxDesk_Transfers\archivo.txt` |
| `subcarpeta/reporte.pdf` | 🟢 Permitido | Resuelve a `%USERPROFILE%\Downloads\NxDesk_Transfers\subcarpeta\reporte.pdf` |
| `../../Windows/System32/calc.exe` | ⛔ Bloqueado | `UnauthorizedAccessException` inmediata |
| `C:\Users\OtroUsuario\Desktop` | ⛔ Bloqueado | `UnauthorizedAccessException` inmediata |
| `/etc/passwd` | ⛔ Bloqueado | `UnauthorizedAccessException` inmediata |
