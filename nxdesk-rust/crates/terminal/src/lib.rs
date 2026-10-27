use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};
use tracing::info;

/// Lista de patrones destructivos bloqueados por política VUL-01
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",
    "format ",
    "del /f",
    "del /s",
    "mkfs",
    "powershell -enc",
    ":(){ :|:& };:",
];

/// Valida un comando contra la política de seguridad VUL-01
pub fn validate_terminal_command(cmd: &str, is_admin_verified: bool) -> Result<String, &'static str> {
    if !is_admin_verified {
        return Err("Acceso denegado: el usuario no cuenta con privilegios de administrador autenticados.");
    }

    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return Err("Comando vacío.");
    }

    let lower = trimmed.to_lowercase();
    for pattern in DANGEROUS_PATTERNS {
        if lower.contains(pattern) {
            return Err("Comando bloqueado por contener patrones potencialmente destructivos para el sistema.");
        }
    }

    Ok(trimmed.to_string())
}

/// Gestor de sesión de terminal remota nativa
pub struct TerminalSession {
    child: Arc<Mutex<Option<Child>>>,
    stdin_writer: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    is_running: Arc<AtomicBool>,
}

impl Default for TerminalSession {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalSession {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            stdin_writer: Arc::new(Mutex::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Inicia el proceso de shell local (cmd.exe en Windows, bash/sh en Linux)
    /// Retorna un canal `Receiver<String>` por donde se transmiten la salida estándar y de error.
    pub async fn start(&self) -> Result<mpsc::Receiver<String>, std::io::Error> {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd.exe");
            c.env("TERM", "xterm-256color");
            c
        } else {
            let mut c = Command::new("/bin/bash");
            c.env("TERM", "xterm-256color");
            c
        };

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW = 0x08000000
            cmd.creation_flags(0x08000000);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("Failed to open stdout");
        let stderr = child.stderr.take().expect("Failed to open stderr");
        let stdin = child.stdin.take().expect("Failed to open stdin");

        *self.stdin_writer.lock().await = Some(stdin);
        *self.child.lock().await = Some(child);
        self.is_running.store(true, Ordering::SeqCst);

        let (tx, rx) = mpsc::channel::<String>(256);

        // Tarea asíncrona para leer STDOUT
        let tx_out = tx.clone();
        let running_out = Arc::clone(&self.is_running);
        tokio::spawn(async move {
            let mut reader = stdout;
            let mut buf = [0u8; 1024];
            while running_out.load(Ordering::Relaxed) {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        if tx_out.send(text).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            running_out.store(false, Ordering::SeqCst);
        });

        // Tarea asíncrona para leer STDERR
        let tx_err = tx;
        let running_err = Arc::clone(&self.is_running);
        tokio::spawn(async move {
            let mut reader = stderr;
            let mut buf = [0u8; 1024];
            while running_err.load(Ordering::Relaxed) {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        if tx_err.send(text).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        info!("💻 [TerminalSession] Shell local iniciado exitosamente.");
        Ok(rx)
    }

    /// Escribe comandos en el stdin del shell activo
    pub async fn write_input(&self, input: &str) -> Result<(), std::io::Error> {
        let mut writer_lock = self.stdin_writer.lock().await;
        if let Some(writer) = writer_lock.as_mut() {
            let formatted = if input.ends_with('\n') {
                input.to_string()
            } else {
                format!("{}\n", input)
            };
            writer.write_all(formatted.as_bytes()).await?;
            writer.flush().await?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "El terminal no está iniciado o stdin no está disponible.",
            ))
        }
    }

    /// Finaliza el proceso del shell y libera recursos
    pub async fn terminate(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let mut child_lock = self.child.lock().await;
        if let Some(mut child) = child_lock.take() {
            let _ = child.kill().await;
            info!("🛑 [TerminalSession] Shell terminado y recursos liberados.");
        }
        let mut writer_lock = self.stdin_writer.lock().await;
        *writer_lock = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vul01_command_validation_blocks_unauthorized() {
        let res = validate_terminal_command("whoami", false);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Acceso denegado"));
    }

    #[test]
    fn test_vul01_command_validation_blocks_destructive() {
        assert!(validate_terminal_command("rm -rf /", true).is_err());
        assert!(validate_terminal_command("format C: /q", true).is_err());
        assert!(validate_terminal_command("del /s /f C:\\*", true).is_err());
        assert!(validate_terminal_command("powershell -enc aW52b2tl...", true).is_err());
    }

    #[test]
    fn test_vul01_command_validation_allows_safe_authorized() {
        let res = validate_terminal_command("echo NxDesk Terminal", true);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "echo NxDesk Terminal");
    }

    #[tokio::test]
    async fn test_terminal_session_lifecycle() {
        let session = TerminalSession::new();
        assert!(!session.is_running());

        let mut rx = session.start().await.expect("Failed to start session");
        assert!(session.is_running());

        // Enviar un comando de prueba simple
        session.write_input("echo test_output").await.expect("Write failed");

        // Leer salida con timeout
        let read_result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(read_result.is_ok());

        session.terminate().await;
        assert!(!session.is_running());
    }
}
