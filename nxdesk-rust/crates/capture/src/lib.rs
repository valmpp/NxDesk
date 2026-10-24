/// Representa un fotograma crudo capturado de la pantalla
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Píxeles en formato BGRA de 32 bits por píxel
    pub data: Vec<u8>,
}

pub struct ScreenCapturer {
    width: u32,
    height: u32,
}

impl Default for ScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer {
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let (w, h) = unsafe {
                let w = GetSystemMetrics(SM_CXSCREEN) as u32;
                let h = GetSystemMetrics(SM_CYSCREEN) as u32;
                (if w > 0 { w } else { 1920 }, if h > 0 { h } else { 1080 })
            };
            Self { width: w, height: h }
        }
        #[cfg(not(windows))]
        {
            Self { width: 1920, height: 1080 }
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Captura un fotograma instantáneo de la pantalla
    pub fn capture_frame(&mut self) -> Result<CapturedFrame, String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Graphics::Gdi::*;

            unsafe {
                let hdc_screen = GetDC(0);
                if hdc_screen == 0 {
                    return Err("No se pudo obtener el contexto de dispositivo (HDC) de la pantalla".to_string());
                }

                let hdc_mem = CreateCompatibleDC(hdc_screen);
                let h_bitmap = CreateCompatibleBitmap(hdc_screen, self.width as i32, self.height as i32);
                let old_bitmap = SelectObject(hdc_mem, h_bitmap);

                // Copiar la pantalla a memoria de mapa de bits (BitBlt)
                let bitblt_ok = BitBlt(
                    hdc_mem,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                    hdc_screen,
                    0,
                    0,
                    SRCCOPY | CAPTUREBLT,
                );

                if bitblt_ok == 0 {
                    SelectObject(hdc_mem, old_bitmap);
                    DeleteObject(h_bitmap);
                    DeleteDC(hdc_mem);
                    ReleaseDC(0, hdc_screen);
                    return Err("BitBlt falló al copiar la pantalla".to_string());
                }

                let mut bi: BITMAPINFO = std::mem::zeroed();
                bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bi.bmiHeader.biWidth = self.width as i32;
                bi.bmiHeader.biHeight = -(self.height as i32); // Negativo para orden top-down
                bi.bmiHeader.biPlanes = 1;
                bi.bmiHeader.biBitCount = 32;
                bi.bmiHeader.biCompression = BI_RGB;

                let buffer_size = (self.width * self.height * 4) as usize;
                let mut data = vec![0u8; buffer_size];

                GetDIBits(
                    hdc_mem,
                    h_bitmap,
                    0,
                    self.height,
                    data.as_mut_ptr() as *mut _,
                    &mut bi,
                    DIB_RGB_COLORS,
                );

                // Limpieza de recursos GDI
                SelectObject(hdc_mem, old_bitmap);
                DeleteObject(h_bitmap);
                DeleteDC(hdc_mem);
                ReleaseDC(0, hdc_screen);

                Ok(CapturedFrame {
                    width: self.width,
                    height: self.height,
                    data,
                })
            }
        }

        #[cfg(not(windows))]
        {
            Err("Captura no implementada para esta plataforma aún".to_string())
        }
    }
}
