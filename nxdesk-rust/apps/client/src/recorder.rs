//! Grabador de sesión remota: AVI MJPEG (formato estándar, reproducible en Windows
//! Media Player, VLC y la mayoría de reproductores).
//!
//! Diseño de baja fricción con el pipeline de video (NO altera FPS, latencia ni resolución):
//!   - El visor ya recibe frames JPEG; el grabador recibe esos bytes y los concatena en un
//!     archivo temporal (streaming a disco: RAM constante ~0).
//!   - El archivo final (.avi) se ensambla al DETENER la grabación: encabezados AVI +
//!     índice idx1 con los FPS reales medidos (frames transcurridos / tiempo), de modo que el
//!     video reproduce a la cadencia REAL de la sesión.
//!   - Todo el I/O es síncrono y aparte: el path del video (p. ej. a 30 FPS, 2K) no toca
//!     ningún watch/buffer del pipeline de señalización.

use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Instant;

const AVIIF_KEYFRAME: u32 = 0x00000010;

/// Grabador de sesión en AVI MJPEG con streaming a disco.
pub struct SessionRecorder {
    tmp_file: Option<std::fs::File>,
    tmp_path: PathBuf,
    out_path: PathBuf,
    /// Bytes ocupados por cada frame (payload JPEG) en orden de llegada.
    frame_sizes: Vec<u32>,
    /// Offset de cada frame relativo al fourcc "movi" (estándar idx1).
    frame_offsets: Vec<u32>,
    width: u32,
    height: u32,
    has_detected_dimensions: bool,
    total_chunk_bytes: u64,
    max_chunk: u32,
    started: Instant,
}

impl SessionRecorder {
    /// Inicia la grabación con dimensiones iniciales por defecto.
    pub fn start_with_dimensions(out_path: PathBuf, default_w: u32, default_h: u32) -> std::io::Result<Self> {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp_path = out_path.clone();
        let stem = out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("sesion")
            .to_string();
        tmp_path.set_file_name(format!("{}.avi.tmp~", stem));

        let tmp_file = std::fs::File::create(&tmp_path)?;
        Ok(Self {
            tmp_file: Some(tmp_file),
            tmp_path,
            out_path,
            frame_sizes: Vec::new(),
            frame_offsets: Vec::new(),
            width: default_w,
            height: default_h,
            has_detected_dimensions: false,
            total_chunk_bytes: 0,
            max_chunk: 0,
            started: Instant::now(),
        })
    }

    /// Inicia la grabación. El archivo final se crea al detener (`finish`).
    pub fn start(out_path: PathBuf) -> std::io::Result<Self> {
        Self::start_with_dimensions(out_path, 1280, 720)
    }

    /// Path final previsto (útil para mensajes al usuario).
    pub fn out_path(&self) -> &std::path::Path {
        &self.out_path
    }

    /// Añade un frame JPEG al stream. Barato: una escritura a disco + bookkeeping.
    pub fn write_frame(&mut self, jpeg: &[u8]) {
        let Some(file) = self.tmp_file.as_mut() else { return };
        if !self.has_detected_dimensions {
            if let Some((w, h)) = jpeg_dimensions(jpeg) {
                self.width = w as u32;
                self.height = h as u32;
                self.has_detected_dimensions = true;
            }
        }
        let mut buf = Vec::with_capacity(jpeg.len() + 8);
        buf.extend_from_slice(b"00dc");
        buf.extend_from_slice(&(jpeg.len() as u32).to_le_bytes());
        buf.extend_from_slice(jpeg);
        if jpeg.len() % 2 == 1 {
            buf.push(0); // alineación par obligatoria en RIFF
        }

        // Offset relativo al fourcc "movi": el primer chunk queda a +4 bytes.
        self.frame_offsets.push((4 + self.total_chunk_bytes) as u32);
        self.frame_sizes.push(jpeg.len() as u32);
        self.max_chunk = self.max_chunk.max(jpeg.len() as u32);
        self.total_chunk_bytes += buf.len() as u64;
        let _ = file.write_all(&buf);
    }

    /// Detiene y ensambla el AVI final. Devuelve el path del archivo generado.
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        // Cerrar el stream de chunks antes de leerlo.
        let _ = self.tmp_file.take();

        if self.frame_sizes.is_empty() {
            let _ = std::fs::remove_file(&self.tmp_path);
            return Err(std::io::Error::other(
                "La grabación no capturó ningún frame de video",
            ));
        }

        let frames = self.frame_sizes.len() as u64;
        let elapsed_us = self.started.elapsed().as_micros().max(1) as u64;

        // Frecuencia de fotogramas estándar (30/1, 60/1 o múltiplo exacto) para compatibilidad
        // universal con decodificadores DirectShow y Media Foundation de Windows.
        let fps = (frames as f64 / (elapsed_us as f64 / 1_000_000.0)).clamp(1.0, 60.0);
        let (dw_rate, dw_scale): (u32, u32) = if (fps - 30.0).abs() < 2.0 {
            (30, 1)
        } else if (fps - 60.0).abs() < 2.0 {
            (60, 1)
        } else if (fps - 25.0).abs() < 1.5 {
            (25, 1)
        } else {
            ((fps * 100.0).round() as u32, 100)
        };
        let microsec_per_frame = ((dw_scale as u64 * 1_000_000) / dw_rate as u64).clamp(16_666, 1_000_000) as u32;

        let max_bytes_per_sec = ((self.total_chunk_bytes * 1_000_000) / elapsed_us)
            .min(u32::MAX as u64) as u32;
        let (w, h) = (self.width.max(64), self.height.max(48));
        let suggested_buffer = (self.max_chunk + 4096).max(256 * 1024);

        // === LIST hdrl ===
        let mut hdrl = Vec::<u8>::with_capacity(192);
        hdrl.extend_from_slice(b"hdrl");

        // avih (56 bytes - MainAVIHeader según especificación estándar Microsoft AVI 1.0)
        let mut avih = [0u8; 56];
        avih[0..4].copy_from_slice(&microsec_per_frame.to_le_bytes()); // [0..4] dwMicroSecPerFrame
        avih[4..8].copy_from_slice(&max_bytes_per_sec.to_le_bytes());  // [4..8] dwMaxBytesPerSec
        avih[8..12].copy_from_slice(&0u32.to_le_bytes());              // [8..12] dwPaddingGranularity
        // dwFlags: AVIF_HASINDEX (0x10) | AVIF_TRUSTCKTYPE (0x800) = 0x00000810.
        // NUNCA incluir AVIF_ISINTERLEAVED (0x100) en archivos de stream único de video sin audio,
        // de lo contrario Windows Media Player aborta con error 0xc00d36c4 ("formato no admitido").
        avih[12..16].copy_from_slice(&0x00000810u32.to_le_bytes());
        avih[16..20].copy_from_slice(&(frames as u32).to_le_bytes());  // [16..20] dwTotalFrames
        avih[20..24].copy_from_slice(&0u32.to_le_bytes());             // [20..24] dwInitialFrames
        avih[24..28].copy_from_slice(&1u32.to_le_bytes());             // [24..28] dwStreams (exactamente 1 stream de video)
        avih[28..32].copy_from_slice(&suggested_buffer.to_le_bytes()); // [28..32] dwSuggestedBufferSize
        avih[32..36].copy_from_slice(&w.to_le_bytes());                // [32..36] dwWidth
        avih[36..40].copy_from_slice(&h.to_le_bytes());                // [36..40] dwHeight
        // avih[40..56] son 4 DWORDs reservados (0)
        hdrl.extend_from_slice(b"avih");
        hdrl.extend_from_slice(&(56u32).to_le_bytes());
        hdrl.extend_from_slice(&avih);

        // LIST strl: strh + strf
        let mut strh = [0u8; 56]; // AVIStreamHeader estándar
        strh[0..4].copy_from_slice(b"vids");                          // [0..4] fccType
        strh[4..8].copy_from_slice(b"MJPG");                          // [4..8] fccHandler
        strh[8..12].copy_from_slice(&0u32.to_le_bytes());             // [8..12] dwFlags
        strh[12..14].copy_from_slice(&0u16.to_le_bytes());            // [12..14] wPriority
        strh[14..16].copy_from_slice(&0u16.to_le_bytes());            // [14..16] wLanguage
        strh[16..20].copy_from_slice(&0u32.to_le_bytes());            // [16..20] dwInitialFrames
        strh[20..24].copy_from_slice(&dw_scale.to_le_bytes());        // [20..24] dwScale
        strh[24..28].copy_from_slice(&dw_rate.to_le_bytes());         // [24..28] dwRate (dwRate / dwScale = FPS real medido)
        strh[28..32].copy_from_slice(&0u32.to_le_bytes());            // [28..32] dwStart
        strh[32..36].copy_from_slice(&(frames as u32).to_le_bytes()); // [32..36] dwLength (total de fotogramas del stream)
        strh[36..40].copy_from_slice(&suggested_buffer.to_le_bytes()); // [36..40] dwSuggestedBufferSize
        strh[40..44].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());  // [40..44] dwQuality (default)
        strh[44..48].copy_from_slice(&0u32.to_le_bytes());            // [44..48] dwSampleSize (0 para compresión variable MJPG)
        strh[48..50].copy_from_slice(&0i16.to_le_bytes());            // [48..50] rcFrame.left
        strh[50..52].copy_from_slice(&0i16.to_le_bytes());            // [50..52] rcFrame.top
        strh[52..54].copy_from_slice(&(w as i16).to_le_bytes());      // [52..54] rcFrame.right
        strh[54..56].copy_from_slice(&(h as i16).to_le_bytes());      // [54..56] rcFrame.bottom

        let mut strf = [0u8; 40]; // BITMAPINFOHEADER
        strf[0..4].copy_from_slice(&40u32.to_le_bytes()); // biSize
        strf[4..8].copy_from_slice(&(w as i32).to_le_bytes()); // biWidth
        strf[8..12].copy_from_slice(&((h as i32)).to_le_bytes()); // biHeight (top-down=+)
        strf[12..14].copy_from_slice(&1u16.to_le_bytes()); // biPlanes
        strf[14..16].copy_from_slice(&24u16.to_le_bytes()); // biBitCount
        strf[16..20].copy_from_slice(b"MJPG"); // biCompression
        // Para compresión MJPG, biSizeImage debe especificar el tamaño del buffer comprimido
        // (self.max_chunk) y NO el tamaño de raster RGB descomprimido (w * h * 3).
        strf[20..24].copy_from_slice(&self.max_chunk.to_le_bytes()); // biSizeImage

        let mut strl = Vec::with_capacity(116);
        strl.extend_from_slice(b"strl");
        strl.extend_from_slice(b"strh");
        strl.extend_from_slice(&(56u32).to_le_bytes());
        strl.extend_from_slice(&strh);
        strl.extend_from_slice(b"strf");
        strl.extend_from_slice(&(40u32).to_le_bytes());
        strl.extend_from_slice(&strf);

        hdrl.extend_from_slice(b"LIST");
        hdrl.extend_from_slice(&(strl.len() as u32).to_le_bytes());
        hdrl.extend_from_slice(&strl);

        let hdrl_size = hdrl.len() as u32; // incluye "hdrl"

        // === idx1 ===
        let mut idx = Vec::<u8>::with_capacity(16 * self.frame_sizes.len() + 8);
        for (i, &size) in self.frame_sizes.iter().enumerate() {
            idx.extend_from_slice(b"00dc");
            idx.extend_from_slice(&AVIIF_KEYFRAME.to_le_bytes());
            idx.extend_from_slice(&self.frame_offsets[i].to_le_bytes());
            idx.extend_from_slice(&size.to_le_bytes());
        }
        let idx_size = idx.len() as u32;

        let movi_size = 4 + self.total_chunk_bytes as u32; // "movi" + chunks
        let riff_size = 4 + (8 + hdrl_size) + (8 + movi_size) + (8 + idx_size);

        let mut out = std::fs::File::create(&self.out_path)?;
        out.write_all(b"RIFF")?;
        out.write_all(&riff_size.to_le_bytes())?;
        out.write_all(b"AVI ")?;
        out.write_all(b"LIST")?;
        out.write_all(&hdrl_size.to_le_bytes())?;
        out.write_all(&hdrl)?;

        out.write_all(b"LIST")?;
        out.write_all(&movi_size.to_le_bytes())?;
        out.write_all(b"movi")?;
        {
            let mut tmp = std::fs::File::open(&self.tmp_path)?;
            tmp.seek(SeekFrom::Start(0))?;
            std::io::copy(&mut tmp, &mut out)?;
        }

        out.write_all(b"idx1")?;
        out.write_all(&idx_size.to_le_bytes())?;
        out.write_all(&idx)?;
        out.flush()?;
        drop(out);

        std::fs::remove_file(&self.tmp_path)?;
        tracing::info!(
            "[Grabación] AVI ensamblado: {} frames @ {:.1} FPS → {}",
            frames,
            fps,
            self.out_path.display()
        );
        Ok(self.out_path)
    }
}

/// Extrae (ancho, alto) del marcador SOF de un JPEG. Coste O(size del encabezado) — mucho más
/// económico que decodificar; permite anotar la resolución sin rasterizar.
fn jpeg_dimensions(jpeg: &[u8]) -> Option<(u16, u16)> {
    let len = jpeg.len();
    // SOI obligatorio: FFD8
    if jpeg.len() < 2 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= len {
        if jpeg[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = jpeg[i + 1];
        match marker {
            // SOF0..SOF15 excepto DHT(0xC4), JPG(0xC8), DAC(0xCC)
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                if i + 9 <= len {
                    let height = u16::from_be_bytes([jpeg[i + 5], jpeg[i + 6]]);
                    let width = u16::from_be_bytes([jpeg[i + 7], jpeg[i + 8]]);
                    return Some((width, height));
                }
                return None;
            }
            0xD8 | 0x01 => i += 2,                    // SOI / TEM sin payload
            0xD0..=0xD7 => i += 2,                    // RST sin payload
            _ => {
                if i + 4 <= len {
                    let seg_len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
                    if seg_len < 2 {
                        return None;
                    }
                    i += 2 + seg_len;
                } else {
                    return None;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

    fn make_jpeg(quality: u8) -> Vec<u8> {
        let (w, h) = (64u16, 48u16);
        let bg = vec![128u8; (w as usize) * (h as usize) * 4];
        let mut out = Vec::new();
        let mut enc = Encoder::new(&mut out, quality);
        enc.set_sampling_factor(SamplingFactor::R_4_2_0);
        enc.encode(&bg, w, h, ColorType::Bgra).unwrap();
        out
    }

    #[test]
    fn extrae_dimensiones_del_sof() {
        let jpeg = make_jpeg(90);
        assert_eq!(jpeg_dimensions(&jpeg), Some((64, 48)));
        assert_eq!(jpeg_dimensions(&[0x12, 0x34]), None);
        assert_eq!(jpeg_dimensions(&[]), None);
    }

    #[test]
    fn ensambla_avi_riff_con_frames_e_indice() {
        let dir = std::env::temp_dir().join(format!("nxdesk-rec-{}", std::process::id()));
        let out = dir.clone().join("prueba.avi");

        let mut rec = SessionRecorder::start(out.clone()).unwrap();
        assert_eq!(rec.out_path(), out);
        for i in 0..5u8 {
            rec.write_frame(&make_jpeg(60 + i));
        }
        let path = rec.finish().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"AVI ");
        assert!(bytes.windows(4).any(|w| w == b"hdrl"));
        assert!(bytes.windows(4).any(|w| w == b"movi"));
        assert!(bytes.windows(4).any(|w| w == b"MJPG"));
        assert!(bytes.windows(4).any(|w| w == b"idx1"));
        // 5 chunks 00dc + 5 entradas idx1
        assert_eq!(
            bytes
                .windows(4)
                .filter(|w| w == b"00dc")
                .count(),
            10
        );
        // Encabezados (192 B) + chunks reales de los frames guardados
        assert!(bytes.len() > 192 + 5 * 400);

        // Validar que strl esté correctamente envuelto en chunk LIST
        let strl_pos = bytes.windows(4).position(|w| w == b"strl").expect("strl presente");
        assert_eq!(&bytes[strl_pos - 8..strl_pos - 4], b"LIST", "strl debe estar precedido por el fourcc LIST");

        // Validar avih: dwStreams == 1, dwWidth == 64, dwHeight == 48 (dimensiones reales del video)
        let avih_pos = bytes.windows(4).position(|w| w == b"avih").expect("avih presente");
        let avih_data = &bytes[avih_pos + 8..avih_pos + 8 + 56];
        let dw_streams = u32::from_le_bytes(avih_data[24..28].try_into().unwrap());
        let dw_width = u32::from_le_bytes(avih_data[32..36].try_into().unwrap());
        let dw_height = u32::from_le_bytes(avih_data[36..40].try_into().unwrap());
        assert_eq!(dw_streams, 1, "dwStreams en MainAVIHeader debe ser 1");
        assert_eq!(dw_width, 64, "dwWidth debe ser 64");
        assert_eq!(dw_height, 48, "dwHeight debe ser 48");

        // El temporal fue eliminado
        assert!(!dir.join("prueba.avi.tmp~").exists());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sin_frames_reporta_error_y_limpia() {
        let dir = std::env::temp_dir().join(format!("nxdesk-rec-empty-{}", std::process::id()));
        let out = dir.clone().join("vacio.avi");
        let rec = SessionRecorder::start(out.clone()).unwrap();
        assert!(rec.finish().is_err());
        assert!(!out.exists());
        assert!(!dir.join("vacio.avi.tmp~").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
