use crate::state::AppState;
use nxdesk_signaling::SignalingClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

pub struct CapturePipeline {
    is_running: Arc<AtomicBool>,
}

impl Default for CapturePipeline {
    fn default() -> Self {
        Self::new()
    }
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
                        match frame_to_jpeg(&frame, quality) {
                            Ok(jpeg_bytes) => {
                                // 3. Almacenar último fotograma y difundir localmente a la UI
                                {
                                    let mut cache = state.latest_jpeg_frame.write().await;
                                    *cache = Some(jpeg_bytes.clone());
                                }
                                state.increment_frame_sequence();
                                let _ = state.frame_broadcast_tx.send(jpeg_bytes.clone());

                                // 4. Retransmitir al par remoto mediante señalización solo si hay sala o sesión activa
                                if let Some(ref sig) = signaling_opt {
                                    if state.room_occupancy.load(Ordering::Relaxed) > 1 {
                                        let _ = sig.send_video_frame(jpeg_bytes).await;
                                    }
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

/// Comprime un fotograma crudo BGRA a formato JPEG usando la biblioteca image
pub fn frame_to_jpeg(frame: &nxdesk_capture::CapturedFrame, quality: u8) -> Result<Vec<u8>, String> {
    let mut rgb = Vec::new();
    frame.to_rgb_into(&mut rgb);
    let mut jpeg_bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, quality);
    encoder
        .encode(&rgb, frame.width, frame.height, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(jpeg_bytes)
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

