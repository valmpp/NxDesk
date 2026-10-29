# 📁 Módulo: Transferencia de Archivos — NxDesk

| Código | MOD-FLT-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Core Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Propósito y Modelo de Fragmentación
El módulo de **Transferencia de Archivos** (`FileTransferService.cs` en C# y `crates/file-transfer` en Rust) permite enviar y recibir ficheros y directorios completos entre el anfitrión y el operador remoto de manera resiliente ante micro-cortes de red.

Para evitar saturar la memoria RAM con archivos de gran tamaño (e.g. instaladores, imágenes ISO o videos), los archivos se dividen en fragmentos denominados **Chunks**:

```json
{
  "transfer_id": "a9d7c814-1234-4a56-87ef-abcdef012345",
  "file_name": "actualizacion.zip",
  "relative_path": "parches/v1.0",
  "offset": 131072,
  "data": "...base64_o_bytes...",
  "is_last": false,
  "checksum": "sha256_hash_opcional"
}
```

---

## 2. Ciclo de Vida de una Transferencia

```mermaid
sequenceDiagram
    autonumber
    participant Client as Operador Remoto
    participant Host as Anfitrión Local
    participant FS as Sistema de Archivos (Sandbox)

    Client->>Host: FileTransferRequest (Nombre, Tamaño, ID)
    Note over Host: Verifica permiso 'allow_file_transfer'
    Host->>FS: Crea archivo vacío en Sandbox y bloquea escritura
    Host-->>Client: RequestAccepted (transfer_id)
    loop Hasta el último fragmento
        Client->>Host: FileChunk (transfer_id, offset, bytes, is_last)
        Host->>FS: WriteAsync(bytes, offset)
    end
    Note over Host: is_last == true: Cierra FileStream y libera transfer_id
    Host-->>Client: TransferCompleted
```

---

## 3. Seguridad y Confinamiento
La lectura, subida o listado de archivos está estrictamente supeditada al cumplimiento de la política de Sandboxing. Para más información técnica, ver:  
👉 **[sandbox.md](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/docs/02-modules/file-transfer/sandbox.md)**
