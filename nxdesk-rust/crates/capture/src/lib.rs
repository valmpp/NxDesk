use nxdesk_core::ScreenInfoDto;

/// Representa un fotograma crudo capturado de la pantalla
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Píxeles en formato BGRA de 32 bits por píxel
    pub data: Vec<u8>,
}

impl CapturedFrame {
    /// Convierte los píxeles BGRA a RGB reutilizando un buffer existente para eliminar asignaciones en memoria
    pub fn to_rgb_into(&self, dst: &mut Vec<u8>) {
        let pixel_count = (self.width * self.height) as usize;
        let required_len = pixel_count * 3;
        dst.resize(required_len, 0);

        let src = &self.data;
        let src_len = src.len();

        for i in 0..pixel_count {
            let src_idx = i * 4;
            let dst_idx = i * 3;
            if src_idx + 2 < src_len {
                dst[dst_idx] = src[src_idx + 2];     // R
                dst[dst_idx + 1] = src[src_idx + 1]; // G
                dst[dst_idx + 2] = src[src_idx];     // B
            }
        }
    }
}

pub struct ScreenCapturer {
    pub width: u32,
    pub height: u32,
    pub active_index: i32,
    pub origin_x: i32,
    pub origin_y: i32,
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
            let mut capturer = Self {
                width: 1920,
                height: 1080,
                active_index: 0,
                origin_x: 0,
                origin_y: 0,
            };
            let screens = capturer.get_screens();
            if let Some(primary) = screens.iter().find(|s| s.is_primary).or_else(|| screens.first()) {
                capturer.width = primary.width;
                capturer.height = primary.height;
                capturer.origin_x = primary.x;
                capturer.origin_y = primary.y;
                capturer.active_index = primary.index;
            }
            capturer
        }
        #[cfg(not(windows))]
        {
            Self {
                width: 1920,
                height: 1080,
                active_index: 0,
                origin_x: 0,
                origin_y: 0,
            }
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Retorna la lista de monitores detectados con sus coordenadas y resoluciones reales
    pub fn get_screens(&self) -> Vec<ScreenInfoDto> {
        let mut screens = Vec::new();
        #[cfg(windows)]
        {
            use windows_sys::Win32::Graphics::Gdi::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            use windows_sys::Win32::Foundation::*;

            unsafe extern "system" fn enum_proc(
                hmonitor: HMONITOR,
                _hdc: HDC,
                _rect: *mut RECT,
                lparam: LPARAM,
            ) -> BOOL {
                let list = &mut *(lparam as *mut Vec<ScreenInfoDto>);
                let mut mi: MONITORINFO = std::mem::zeroed();
                mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(hmonitor, &mut mi) != 0 {
                    let w = (mi.rcMonitor.right - mi.rcMonitor.left) as u32;
                    let h = (mi.rcMonitor.bottom - mi.rcMonitor.top) as u32;
                    let is_primary = (mi.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1
                    let idx = list.len() as i32;
                    let name = if is_primary {
                        format!("Pantalla {} (Principal)", idx + 1)
                    } else {
                        format!("Pantalla {}", idx + 1)
                    };
                    list.push(ScreenInfoDto {
                        index: idx,
                        name,
                        width: w,
                        height: h,
                        x: mi.rcMonitor.left,
                        y: mi.rcMonitor.top,
                        is_primary,
                    });
                }
                1
            }

            unsafe {
                EnumDisplayMonitors(
                    0,
                    std::ptr::null(),
                    Some(enum_proc),
                    &mut screens as *mut _ as isize,
                );

                // Si hay más de un monitor, agregar la opción de Mosaico (Todas las Pantallas)
                if screens.len() > 1 {
                    let virt_w = GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32;
                    let virt_h = GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32;
                    let virt_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                    let virt_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

                    screens.push(ScreenInfoDto {
                        index: -1,
                        name: "Mosaico (Todas las Pantallas)".to_string(),
                        width: if virt_w > 0 { virt_w } else { 1920 },
                        height: if virt_h > 0 { virt_h } else { 1080 },
                        x: virt_x,
                        y: virt_y,
                        is_primary: false,
                    });
                }
            }
        }
        if screens.is_empty() {
            screens.push(ScreenInfoDto {
                index: 0,
                name: "Pantalla 1".to_string(),
                width: self.width,
                height: self.height,
                x: 0,
                y: 0,
                is_primary: true,
            });
        }
        screens
    }

    /// Conmuta la captura activa a una pantalla específica o mosaico virtual
    pub fn select_screen(&mut self, index: i32) -> Result<ScreenInfoDto, String> {
        let screens = self.get_screens();
        if let Some(target) = screens.into_iter().find(|s| s.index == index) {
            self.width = target.width;
            self.height = target.height;
            self.origin_x = target.x;
            self.origin_y = target.y;
            self.active_index = target.index;
            Ok(target)
        } else {
            Err(format!("Monitor con índice {} no encontrado", index))
        }
    }

    /// Captura un fotograma instantáneo de la pantalla
    pub fn capture_frame(&mut self) -> Result<CapturedFrame, String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Graphics::Gdi::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

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
                    self.origin_x,
                    self.origin_y,
                    SRCCOPY,
                );

                if bitblt_ok == 0 {
                    SelectObject(hdc_mem, old_bitmap);
                    DeleteObject(h_bitmap);
                    DeleteDC(hdc_mem);
                    ReleaseDC(0, hdc_screen);
                    return Err("Falló la llamada a BitBlt al capturar la pantalla".to_string());
                }

                // Dibujar el cursor de Windows sobre el bitmap para que se vea en el streaming
                let mut cursor_info: CURSORINFO = std::mem::zeroed();
                cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
                if GetCursorInfo(&mut cursor_info) != 0 && (cursor_info.flags & 1) != 0 {
                    let rel_x = cursor_info.ptScreenPos.x - self.origin_x;
                    let rel_y = cursor_info.ptScreenPos.y - self.origin_y;
                    if rel_x >= 0 && rel_x < self.width as i32 && rel_y >= 0 && rel_y < self.height as i32 {
                        let mut icon_info = ICONINFO {
                            fIcon: 0,
                            xHotspot: 0,
                            yHotspot: 0,
                            hbmMask: 0,
                            hbmColor: 0,
                        };
                        if GetIconInfo(cursor_info.hCursor, &mut icon_info) != 0 {
                            DrawIconEx(
                                hdc_mem,
                                rel_x - icon_info.xHotspot as i32,
                                rel_y - icon_info.yHotspot as i32,
                                cursor_info.hCursor,
                                0,
                                0,
                                0,
                                0,
                                3, // DI_NORMAL
                            );
                            if icon_info.hbmMask != 0 { DeleteObject(icon_info.hbmMask); }
                            if icon_info.hbmColor != 0 { DeleteObject(icon_info.hbmColor); }
                        }
                    }
                }

                let mut bi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: self.width as i32,
                        biHeight: -(self.height as i32), // Top-down DIB
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB,
                        biSizeImage: self.width * self.height * 4,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
                };

                let mut buffer = vec![0u8; (self.width * self.height * 4) as usize];
                let lines_copied = GetDIBits(
                    hdc_mem,
                    h_bitmap,
                    0,
                    self.height,
                    buffer.as_mut_ptr() as *mut _,
                    &mut bi,
                    DIB_RGB_COLORS,
                );

                // Limpieza de handles GDI
                SelectObject(hdc_mem, old_bitmap);
                DeleteObject(h_bitmap);
                DeleteDC(hdc_mem);
                ReleaseDC(0, hdc_screen);

                if lines_copied == 0 {
                    return Err("Error al volcar mapa de bits con GetDIBits".to_string());
                }

                Ok(CapturedFrame {
                    width: self.width,
                    height: self.height,
                    data: buffer,
                })
            }
        }
        #[cfg(not(windows))]
        {
            Ok(CapturedFrame {
                width: self.width,
                height: self.height,
                data: vec![0u8; (self.width * self.height * 4) as usize],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capturer_creation() {
        let capturer = ScreenCapturer::new();
        let (w, h) = capturer.dimensions();
        assert!(w > 0);
        assert!(h > 0);
    }

    #[test]
    fn test_to_rgb_into_buffer_conversion() {
        let bgra = vec![255, 0, 0, 255]; // Azul en BGRA
        let frame = CapturedFrame {
            width: 1,
            height: 1,
            data: bgra,
        };
        let mut rgb = Vec::new();
        frame.to_rgb_into(&mut rgb);
        assert_eq!(rgb.len(), 3);
        assert_eq!(rgb, vec![0, 0, 255]); // Azul en RGB
    }

    #[test]
    fn test_get_screens_detection() {
        let capturer = ScreenCapturer::new();
        let screens = capturer.get_screens();
        assert!(!screens.is_empty());
        for s in &screens {
            println!("Detectada: {} -> {}x{} en ({}, {}) Primary={}", s.name, s.width, s.height, s.x, s.y, s.is_primary);
        }
    }
}
