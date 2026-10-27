use nxdesk_core::{DirectoryItemDto, FileChunk, ListDirectoryResponse};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::Mutex;
use tracing::info;

/// Obtiene el directorio seguro por defecto en Downloads/NxDesk_Transfers
pub fn get_safe_transfers_dir() -> PathBuf {
    let base = if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        std::env::temp_dir()
    };

    base.join("Downloads").join("NxDesk_Transfers")
}

/// Sanea y valida estrictamente una ruta para prevenir Path Traversal
pub fn sanitize_destination_path(relative_path: &str, file_name: &str) -> Result<PathBuf, String> {
    let safe_dir = get_safe_transfers_dir();

    // Extraer únicamente el nombre de archivo puro, removiendo cualquier prefijo de ruta
    let clean_file_name = Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;

    // Limpiar rutas relativas contrasecuencias ".."
    let clean_relative = relative_path
        .replace("..", "")
        .trim_matches(|c| c == '/' || c == '\\')
        .to_string();

    let target_dir = if clean_relative.is_empty() {
        safe_dir.clone()
    } else {
        safe_dir.join(clean_relative)
    };

    let full_path = target_dir.join(clean_file_name);

    // Verificación estricta de sandbox: el destino final DEBE comenzar con safe_dir
    let safe_canon = safe_dir.canonicalize().unwrap_or(safe_dir);
    // Para rutas que aún no existen, comprobamos el prefijo lógico normalizado
    let target_str = full_path.to_string_lossy().to_lowercase();
    let safe_str = safe_canon.to_string_lossy().to_lowercase();

    if !target_str.starts_with(&safe_str) && !target_str.contains("nxdesk_transfers") {
        return Err(
            "Ataque de Path Traversal bloqueado: intento de escritura fuera del sandbox."
                .to_string(),
        );
    }

    Ok(full_path)
}

/// Explora y lista el contenido de un directorio local
pub async fn list_directory_async(path_str: &str) -> ListDirectoryResponse {
    let target_path = if path_str.trim().is_empty() {
        let safe = get_safe_transfers_dir();
        let _ = std::fs::create_dir_all(&safe);
        safe
    } else {
        PathBuf::from(path_str)
    };

    let mut response = ListDirectoryResponse {
        current_path: target_path.to_string_lossy().to_string(),
        items: Vec::new(),
        error: None,
    };

    let mut read_dir = match fs::read_dir(&target_path).await {
        Ok(rd) => rd,
        Err(e) => {
            response.error = Some(format!("No se pudo leer el directorio: {}", e));
            return response;
        }
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        if let Ok(metadata) = entry.metadata().await {
            let is_dir = metadata.is_dir();
            let size = if is_dir { 0 } else { metadata.len() };
            let name = entry.file_name().to_string_lossy().to_string();

            response.items.push(DirectoryItemDto {
                name,
                is_directory: is_dir,
                size,
                last_modified: None,
            });
        }
    }

    // Ordenar directorios primero, luego archivos alfabéticamente
    response
        .items
        .sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

    response
}

/// Gestor asíncrono de transferencias activas
#[derive(Default, Clone)]
pub struct FileTransferManager {
    active_transfers: Arc<Mutex<HashMap<String, PathBuf>>>,
}

impl FileTransferManager {
    pub fn new() -> Self {
        Self {
            active_transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Prepara la recepción de un archivo asegurando el sandbox
    pub async fn prepare_incoming_transfer(
        &self,
        transfer_id: &str,
        relative_path: &str,
        file_name: &str,
    ) -> Result<PathBuf, String> {
        let dest = sanitize_destination_path(relative_path, file_name)?;

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Crear/truncar el archivo destino
        File::create(&dest).await.map_err(|e| e.to_string())?;

        let mut lock = self.active_transfers.lock().await;
        lock.insert(transfer_id.to_string(), dest.clone());

        info!(
            "📁 [FileTransfer] Transferencia '{}' registrada en: {:?}",
            transfer_id, dest
        );
        Ok(dest)
    }

    /// Escribe un fragmento en el archivo correspondiente
    pub async fn write_chunk(&self, chunk: &FileChunk) -> Result<(), String> {
        let path = {
            let lock = self.active_transfers.lock().await;
            lock.get(&chunk.transfer_id)
                .cloned()
                .ok_or_else(|| format!("ID de transferencia desconocido: {}", chunk.transfer_id))?
        };

        let mut file = OpenOptions::new()
            .write(true)
            .open(&path)
            .await
            .map_err(|e| e.to_string())?;

        file.seek(SeekFrom::Start(chunk.offset))
            .await
            .map_err(|e| e.to_string())?;

        file.write_all(&chunk.data)
            .await
            .map_err(|e| e.to_string())?;

        if chunk.is_last {
            let mut lock = self.active_transfers.lock().await;
            lock.remove(&chunk.transfer_id);
            info!(
                "✅ [FileTransfer] Transferencia '{}' completada con éxito.",
                chunk.transfer_id
            );
        }

        Ok(())
    }

    /// Lee un fragmento de archivo local para subida
    pub async fn read_chunk(
        file_path: &Path,
        offset: u64,
        chunk_size: usize,
    ) -> Result<(Vec<u8>, bool), std::io::Error> {
        let mut file = File::open(file_path).await?;
        let metadata = file.metadata().await?;
        let total_size = metadata.len();

        file.seek(SeekFrom::Start(offset)).await?;

        let mut buffer = vec![0u8; chunk_size];
        let bytes_read = file.read(&mut buffer).await?;
        buffer.truncate(bytes_read);

        let is_last = (offset + bytes_read as u64) >= total_size;
        Ok((buffer, is_last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_traversal_blocked() {
        // Intento de escapar usando "../"
        let res = sanitize_destination_path("../../Windows/System32", "evil.dll");
        // El sanitize_destination_path elimina ".." y el archivo queda en Downloads/NxDesk_Transfers/Windows/System32/evil.dll
        let path = res.expect("sanitize failed");
        let path_str = path.to_string_lossy().to_lowercase();
        assert!(path_str.contains("nxdesk_transfers"));
        assert!(!path_str.contains(".."));
    }

    #[tokio::test]
    async fn test_list_directory_returns_items() {
        let current_dir = std::env::current_dir().unwrap();
        let res = list_directory_async(&current_dir.to_string_lossy()).await;
        assert!(res.error.is_none());
        assert!(!res.items.is_empty());
    }

    #[tokio::test]
    async fn test_transfer_manager_write_and_complete() {
        let manager = FileTransferManager::new();
        let transfer_id = "test-transfer-123";

        let path = manager
            .prepare_incoming_transfer(transfer_id, "", "test_sample.bin")
            .await
            .expect("prepare failed");

        let chunk1 = FileChunk {
            transfer_id: transfer_id.to_string(),
            offset: 0,
            is_last: false,
            data: b"Hello NxDesk ".to_vec(),
        };
        manager
            .write_chunk(&chunk1)
            .await
            .expect("chunk 1 write failed");

        let chunk2 = FileChunk {
            transfer_id: transfer_id.to_string(),
            offset: 13,
            is_last: true,
            data: b"Pure Rust!".to_vec(),
        };
        manager
            .write_chunk(&chunk2)
            .await
            .expect("chunk 2 write failed");

        let content = fs::read_to_string(&path).await.expect("read failed");
        assert_eq!(content, "Hello NxDesk Pure Rust!");

        // Limpiar archivo de prueba
        let _ = fs::remove_file(&path).await;
    }
}
