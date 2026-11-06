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
pub fn validate_terminal_command(
    cmd: &str,
    is_admin_verified: bool,
) -> Result<String, &'static str> {
    if !is_admin_verified {
        return Err(
            "Acceso denegado: el usuario no cuenta con privilegios de administrador autenticados.",
        );
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

        // Tarea asíncrona para leer STDOUT (buffer 8 KiB: menos syscalls y menos mensajes
        // hacia el broadcast; la agrupación de red hace el resto).
        let tx_out = tx.clone();
        let running_out = Arc::clone(&self.is_running);
        tokio::spawn(async move {
            let mut reader = stdout;
            let mut buf = [0u8; 8192];
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

        // Tarea asíncrona para leer STDERR (buffer 8 KiB, igual que stdout)
        let tx_err = tx;
        let running_err = Arc::clone(&self.is_running);
        tokio::spawn(async move {
            let mut reader = stderr;
            let mut buf = [0u8; 8192];
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
            let formatted = if input.ends_with("\r\n") {
                input.to_string()
            } else if input.ends_with('\n') {
                format!("{}\r\n", &input[..input.len() - 1])
            } else {
                format!("{}\r\n", input)
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

    /// Interrumpe la ejecución actual enviando Ctrl+C y deteniendo el proceso si está colgado
    pub async fn interrupt(&self) {
        if let Some(writer) = self.stdin_writer.lock().await.as_mut() {
            let _ = writer.write_all(b"\x03\r\n").await;
            let _ = writer.flush().await;
        }
        self.terminate().await;
    }

    /// Finaliza el proceso del shell y libera recursos
    pub async fn terminate(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let mut child_lock = self.child.lock().await;
        if let Some(mut child) = child_lock.take() {
            #[cfg(windows)]
            if let Some(pid) = child.id() {
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new("taskkill")
                    .args(&["/F", "/T", "/PID", &pid.to_string()])
                    .creation_flags(0x08000000) // CREATE_NO_WINDOW
                    .output();
            }
            let _ = child.kill().await;
            info!("🛑 [TerminalSession] Shell terminado y recursos liberados.");
        }
        let mut writer_lock = self.stdin_writer.lock().await;
        *writer_lock = None;
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Ok(mut child_lock) = self.child.try_lock() {
            if let Some(mut child) = child_lock.take() {
                #[cfg(windows)]
                if let Some(pid) = child.id() {
                    use std::os::windows::process::CommandExt;
                    let _ = std::process::Command::new("taskkill")
                        .args(&["/F", "/T", "/PID", &pid.to_string()])
                        .creation_flags(0x08000000)
                        .output();
                }
                let _ = child.start_kill();
                tracing::info!("🛑 [TerminalSession] Proceso hijo terminado automáticamente en Drop.");
            }
        }
    }
}

// =============================================================================
// Pseudo-terminal interactiva real (ConPTY en Windows, PTY en Linux/macOS)
// =============================================================================

struct PtyInner {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// Shell interactiva sobre pseudo-terminal. Thread-safe; todos los métodos son no bloqueantes
/// salvo la lectura, que vive en un hilo dedicado para no tocar el runtime de Tokio ni el video.
pub struct PtySession {
    inner: std::sync::Mutex<Option<PtyInner>>,
    running: Arc<AtomicBool>,
}

impl Default for PtySession {
    fn default() -> Self {
        Self::new()
    }
}

fn io_err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

impl PtySession {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(None),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Lanza PowerShell (Windows) o `$SHELL`/bash (Unix) dentro de un PTY de `cols`x`rows`.
    /// Devuelve un canal con los bytes crudos de salida (incluye secuencias ANSI).
    pub fn start(&self, cols: u16, rows: u16) -> std::io::Result<mpsc::Receiver<Vec<u8>>> {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};

        self.terminate();

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: rows.clamp(5, 200),
                cols: cols.clamp(20, 400),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io_err)?;

        let mut cmd = if cfg!(windows) {
            let mut c = CommandBuilder::new("powershell.exe");
            c.arg("-NoLogo");
            c
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            CommandBuilder::new(shell)
        };
        cmd.env("TERM", "xterm-256color");
        if let Ok(home) = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd).map_err(io_err)?;
        drop(pair.slave); // el proceso hijo conserva su extremo

        let mut reader = pair.master.try_clone_reader().map_err(io_err)?;
        let writer = pair.master.take_writer().map_err(io_err)?;

        *self.inner.lock().map_err(io_err)? = Some(PtyInner {
            master: pair.master,
            writer,
            child,
        });
        self.running.store(true, Ordering::SeqCst);

        let (tx, rx) = mpsc::channel::<Vec<u8>>(256);
        let running = Arc::clone(&self.running);
        std::thread::Builder::new()
            .name("nxdesk-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match std::io::Read::read(&mut reader, &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
                running.store(false, Ordering::SeqCst);
                info!("💻 [PtySession] Lector de PTY finalizado.");
            })?;

        info!("💻 [PtySession] Shell interactiva iniciada en PTY {}x{}.", cols, rows);
        Ok(rx)
    }

    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut guard = self.inner.lock().map_err(io_err)?;
        let inner = guard
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "PTY no iniciado"))?;
        inner.writer.write_all(data)?;
        inner.writer.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> std::io::Result<()> {
        let guard = self.inner.lock().map_err(io_err)?;
        if let Some(inner) = guard.as_ref() {
            inner
                .master
                .resize(portable_pty::PtySize {
                    rows: rows.clamp(5, 200),
                    cols: cols.clamp(20, 400),
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(io_err)?;
        }
        Ok(())
    }

    /// Mata el shell y libera master/writer (esto también desbloquea el hilo lector).
    pub fn terminate(&self) {
        let taken = self.inner.lock().ok().and_then(|mut g| g.take());
        if let Some(mut inner) = taken {
            let _ = inner.child.kill();
            let _ = inner.child.wait();
            drop(inner);
            info!("🛑 [PtySession] Shell terminada y PTY liberado.");
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Guarda VUL-01 para entrada interactiva: reconstruye la línea tecleada y, al pulsar Enter,
/// la valida. Si es destructiva sustituye el Enter por Ctrl+C (cancela la línea en el shell).
#[derive(Default)]
pub struct InputLineGuard {
    line: String,
    in_escape: bool,
    in_csi: bool,
}

impl InputLineGuard {
    /// Devuelve (bytes a reenviar al PTY, motivo de bloqueo si lo hubo)
    pub fn filter(&mut self, input: &str) -> (String, Option<String>) {
        let mut out = String::with_capacity(input.len());
        let mut blocked = None;
        for ch in input.chars() {
            if self.in_escape {
                out.push(ch);
                if self.in_csi {
                    if ('@'..='~').contains(&ch) {
                        self.in_escape = false;
                        self.in_csi = false;
                    }
                } else if ch == '[' || ch == 'O' {
                    self.in_csi = true;
                } else {
                    self.in_escape = false;
                }
                continue;
            }
            match ch {
                '\x1b' => {
                    self.in_escape = true;
                    out.push(ch);
                }
                '\r' | '\n' => {
                    let line = std::mem::take(&mut self.line);
                    if !line.trim().is_empty() {
                        if let Err(reason) = validate_terminal_command(&line, true) {
                            blocked = Some(reason.to_string());
                            out.push('\x03');
                            continue;
                        }
                    }
                    out.push(ch);
                }
                '\x7f' | '\x08' => {
                    self.line.pop();
                    out.push(ch);
                }
                '\x03' | '\x15' => {
                    self.line.clear();
                    out.push(ch);
                }
                c if (c as u32) < 0x20 => out.push(c),
                c => {
                    self.line.push(c);
                    out.push(c);
                }
            }
        }
        (out, blocked)
    }
}

/// Convierte el flujo de bytes del PTY a texto sin partir caracteres UTF-8 multibyte.
#[derive(Default)]
pub struct Utf8Chunker {
    pending: Vec<u8>,
}

impl Utf8Chunker {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        match std::str::from_utf8(&self.pending) {
            Ok(s) => {
                let out = s.to_string();
                self.pending.clear();
                out
            }
            Err(e) if e.error_len().is_none() => {
                // Secuencia incompleta al final: emitir lo válido y conservar el resto
                let valid = e.valid_up_to();
                let out = String::from_utf8_lossy(&self.pending[..valid]).to_string();
                self.pending.drain(..valid);
                out
            }
            Err(_) => {
                let out = String::from_utf8_lossy(&self.pending).to_string();
                self.pending.clear();
                out
            }
        }
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
        session
            .write_input("echo test_output")
            .await
            .expect("Write failed");

        // Leer salida con timeout
        let read_result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(read_result.is_ok());

        session.terminate().await;
        assert!(!session.is_running());
    }

    #[test]
    fn test_line_guard_blocks_destructive_line_and_cancels_it() {
        let mut g = InputLineGuard::default();
        let (out, blocked) = g.filter("rm -rf /\r");
        assert!(blocked.is_some());
        assert!(out.ends_with('\x03'), "El Enter debe sustituirse por Ctrl+C");
        assert!(!out.contains('\r'));
    }

    #[test]
    fn test_line_guard_allows_safe_line_with_backspace_and_arrows() {
        let mut g = InputLineGuard::default();
        let (out, blocked) = g.filter("dirx\x7f\x1b[A\r");
        assert!(blocked.is_none());
        assert!(out.ends_with('\r'));
    }

    #[test]
    fn test_utf8_chunker_never_splits_multibyte() {
        let mut c = Utf8Chunker::default();
        let bytes = "ñá".as_bytes();
        let a = c.push(&bytes[..1]);
        let b = c.push(&bytes[1..]);
        assert_eq!(format!("{}{}", a, b), "ñá");
    }

    #[tokio::test]
    async fn test_pty_session_lifecycle() {
        let pty = PtySession::new();
        assert!(!pty.is_running());
        if let Ok(mut rx) = pty.start(80, 24) {
            assert!(pty.is_running());
            let _ = pty.write(b"echo NXPTY\r");
            let got = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
            assert!(got.is_ok(), "El PTY debe producir salida");
            pty.resize(100, 30).expect("resize");
            pty.terminate();
            assert!(!pty.is_running());
        }
    }
}
