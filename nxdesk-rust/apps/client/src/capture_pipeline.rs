use crate::state::AppState;
use nxdesk_signaling::SignalingClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

pub struct CapturePipeline {
    is_running: Arc<AtomicBool>,
}

impl CapturePipeline {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }


    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    pub fn start(
        &self,
        state: AppState,
        signaling_opt: Option<Arc<SignalingClient>>,
    ) {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return;
        }

        let is_running_clone = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("🎥 [CapturePipeline] Bucle de captura y compresión JPEG iniciado.");

            let (fps, quality) = {
                let cfg = state.config.read().await;
                (cfg.max_fps.clamp(5, 60), (cfg.quality as u8).clamp(10, 100))
            };

            let frame_duration = Duration::from_millis((1000 / fps).max(16) as u64);
            let mut interval = tokio::time::interval(frame_duration);

            while is_running_clone.load(Ordering::Relaxed) {
                interval.tick().await;

                // 1. Captura de pantalla nativa
                let capture_res = {
                    let mut capturer = state.capturer.lock().await;
                    capturer.capture_frame()
                };

                match capture_res {
                    Ok(frame) => {
                        // 2. Compresión JPEG acelerada
                        match frame.to_jpeg(quality) {
                            Ok(jpeg_bytes) => {
                                // Almacenar último fotograma en memoria para clientes web
                                {
                                    let mut cache = state.latest_jpeg_frame.write().await;
                                    *cache = Some(jpeg_bytes.clone());
                                }

                                // 3. Difundir a viewers locales (Web UI)
                                let _ = state.frame_broadcast_tx.send(jpeg_bytes.clone());

                                // 4. Retransmitir al par remoto mediante señalización si está conectado
                                if let Some(ref sig) = signaling_opt {
                                    let _ = sig.send_video_frame(jpeg_bytes).await;
                                }
                            }
                            Err(e) => {
                                error!("Error comprimiendo fotograma a JPEG: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error capturando pantalla: {}", e);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }

            info!("🛑 [CapturePipeline] Bucle de captura detenido.");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_pipeline_initial_state() {
        let pipeline = CapturePipeline::new();
        assert!(!pipeline.is_running());
        pipeline.stop();
        assert!(!pipeline.is_running());
    }
}

