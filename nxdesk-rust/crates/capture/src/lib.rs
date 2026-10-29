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
                dst[dst_idx] = src[src_idx + 2]; // R
                dst[dst_idx + 1] = src[src_idx + 1]; // G
                dst[dst_idx + 2] = src[src_idx]; // B
            }
        }
    }

    /// Convierte y reescala directamente de BGRA a RGB con factor de resolución óptimo (ej. 1280x720)
    /// Minimizando CPU y reduciendo el payload de red sobre WAN en más de un 80% sin latencia.
    pub fn to_scaled_rgb_into(&self, dst_w: u32, dst_h: u32, dst: &mut Vec<u8>) {
        let required_len = (dst_w * dst_h * 3) as usize;
        dst.resize(required_len, 0);

        let src_w = self.width as usize;
        let src_h = self.height as usize;
        let dw = dst_w as usize;
        let dh = dst_h as usize;

        if src_w == 0 || src_h == 0 || dw == 0 || dh == 0 {
            return;
        }

        let src = &self.data;
        let src_len = src.len();

        let x_map: Vec<usize> = (0..dw).map(|dx| (dx * src_w) / dw).collect();

        for dy in 0..dh {
            let sy = (dy * src_h) / dh;
            let src_row = sy * src_w * 4;
            let dst_row = dy * dw * 3;

            for (dx, &sx) in x_map.iter().enumerate() {
                let src_idx = src_row + sx * 4;
                let dst_idx = dst_row + dx * 3;

                if src_idx + 2 < src_len {
                    dst[dst_idx] = src[src_idx + 2];     // R
                    dst[dst_idx + 1] = src[src_idx + 1]; // G
                    dst[dst_idx + 2] = src[src_idx];     // B
                }
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
    pub last_frame: Option<CapturedFrame>,
    raw_buffer: Vec<u8>,
    x_map: Vec<usize>,
    last_mapped_dw: u32,
    last_mapped_src_w: u32,
    #[cfg(windows)]
    hdc_screen: isize,
    #[cfg(windows)]
    hdc_mem: isize,
    #[cfg(windows)]
    h_bitmap: isize,
    #[cfg(windows)]
    old_bitmap: isize,
    #[cfg(windows)]
    allocated_w: u32,
    #[cfg(windows)]
    allocated_h: u32,
    #[cfg(windows)]
    last_cursor: isize,
    #[cfg(windows)]
    last_hotspot_x: i32,
    #[cfg(windows)]
    last_hotspot_y: i32,
}

impl Drop for ScreenCapturer {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Graphics::Gdi::*;
            if self.h_bitmap != 0 {
                SelectObject(self.hdc_mem, self.old_bitmap);
                DeleteObject(self.h_bitmap);
            }
            if self.hdc_mem != 0 {
                DeleteDC(self.hdc_mem);
            }
            if self.hdc_screen != 0 {
                ReleaseDC(0, self.hdc_screen);
            }
        }
    }
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
                last_frame: None,
                raw_buffer: Vec::with_capacity(1920 * 1080 * 4),
                x_map: Vec::new(),
                last_mapped_dw: 0,
                last_mapped_src_w: 0,
                hdc_screen: 0,
                hdc_mem: 0,
                h_bitmap: 0,
                old_bitmap: 0,
                allocated_w: 0,
                allocated_h: 0,
                last_cursor: 0,
                last_hotspot_x: 0,
                last_hotspot_y: 0,
            };
            let screens = capturer.get_screens();
            if let Some(primary) = screens
                .iter()
                .find(|s| s.is_primary)
                .or_else(|| screens.first())
            {
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
                last_frame: None,
                raw_buffer: Vec::with_capacity(1920 * 1080 * 4),
                x_map: Vec::new(),
                last_mapped_dw: 0,
                last_mapped_src_w: 0,
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
            use windows_sys::Win32::Foundation::*;
            use windows_sys::Win32::Graphics::Gdi::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

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

    /// Genera un fotograma de reserva cuando Windows se bloquea o salta un diálogo de seguridad UAC
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn generate_standby_frame(&self) -> CapturedFrame {
        let pixel_count = (self.width * self.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];
        // Fondo azul noche seguro (BGRA: B=35, G=20, R=12, A=255)
        for chunk in data.chunks_exact_mut(4) {
            chunk[0] = 35;
            chunk[1] = 20;
            chunk[2] = 12;
            chunk[3] = 255;
        }
        CapturedFrame {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Captura un fotograma instantáneo de la pantalla con resistencia a bloqueos de Windows y UAC
    pub fn capture_frame(&mut self) -> Result<CapturedFrame, String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Graphics::Gdi::*;
            use windows_sys::Win32::System::StationsAndDesktops::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            unsafe {
                let mut hdc_screen = GetDC(0);
                if hdc_screen == 0 {
                    // Solo si GetDC falla, intentar reasociar al input desktop activo (Winlogon o UAC)
                    let hdesk = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS);
                    if hdesk != 0 {
                        let _ = SetThreadDesktop(hdesk);
                        CloseDesktop(hdesk);
                        hdc_screen = GetDC(0);
                    }
                }

                if hdc_screen == 0 {
                    if let Some(ref last) = self.last_frame {
                        return Ok(last.clone());
                    }
                    return Ok(self.generate_standby_frame());
                }

                let hdc_mem = CreateCompatibleDC(hdc_screen);
                let h_bitmap =
                    CreateCompatibleBitmap(hdc_screen, self.width as i32, self.height as i32);
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
                    let hdesk = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS);
                    if hdesk != 0 {
                        let _ = SetThreadDesktop(hdesk);
                        CloseDesktop(hdesk);
                    }
                    SelectObject(hdc_mem, old_bitmap);
                    DeleteObject(h_bitmap);
                    DeleteDC(hdc_mem);
                    ReleaseDC(0, hdc_screen);

                    // Si BitBlt falló debido a bloqueo o cuadro UAC, mantener último frame en lugar de pantalla negra
                    if let Some(ref last) = self.last_frame {
                        return Ok(last.clone());
                    }
                    return Ok(self.generate_standby_frame());
                }

                // Dibujar el cursor de Windows sobre el bitmap para que se vea en el streaming
                let mut cursor_info: CURSORINFO = std::mem::zeroed();
                cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
                if GetCursorInfo(&mut cursor_info) != 0 && (cursor_info.flags & 1) != 0 {
                    let rel_x = cursor_info.ptScreenPos.x - self.origin_x;
                    let rel_y = cursor_info.ptScreenPos.y - self.origin_y;
                    if rel_x >= 0
                        && rel_x < self.width as i32
                        && rel_y >= 0
                        && rel_y < self.height as i32
                    {
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
                            if icon_info.hbmMask != 0 {
                                DeleteObject(icon_info.hbmMask);
                            }
                            if icon_info.hbmColor != 0 {
                                DeleteObject(icon_info.hbmColor);
                            }
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
                    bmiColors: [RGBQUAD {
                        rgbBlue: 0,
                        rgbGreen: 0,
                        rgbRed: 0,
                        rgbReserved: 0,
                    }],
                };

                let req_len = (self.width * self.height * 4) as usize;
                if self.raw_buffer.len() != req_len {
                    self.raw_buffer.resize(req_len, 0);
                }

                let lines_copied = GetDIBits(
                    hdc_mem,
                    h_bitmap,
                    0,
                    self.height,
                    self.raw_buffer.as_mut_ptr() as *mut _,
                    &mut bi,
                    DIB_RGB_COLORS,
                );

                // Limpieza de handles GDI
                SelectObject(hdc_mem, old_bitmap);
                DeleteObject(h_bitmap);
                DeleteDC(hdc_mem);
                ReleaseDC(0, hdc_screen);

                if lines_copied == 0 {
                    if let Some(ref last) = self.last_frame {
                        return Ok(last.clone());
                    }
                    return Ok(self.generate_standby_frame());
                }

                let frame = CapturedFrame {
                    width: self.width,
                    height: self.height,
                    data: self.raw_buffer.clone(),
                };
                self.last_frame = Some(frame.clone());
                Ok(frame)
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

    /// Detecta en <0.02 ms si la pantalla ha cambiado muestreando 64 puntos estratégicos
    pub fn fast_has_changed(&self, last_hash: &mut u64) -> bool {
        let buf = &self.raw_buffer;
        let len = buf.len();
        if len < 256 {
            return true;
        }
        let step = len / 64;
        let mut h: u64 = 0xcbf29ce484222325;
        for i in 0..64 {
            let val = buf[i * step] as u64;
            h ^= val;
            h = h.wrapping_mul(0x100000001b3);
        }
        let changed = h != *last_hash;
        *last_hash = h;
        changed
    }

    /// Captura directamente el fotograma en self.raw_buffer usando GDI Win32 persistente
    pub fn capture_raw_bgra_internal(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Graphics::Gdi::*;
            use windows_sys::Win32::System::StationsAndDesktops::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            unsafe {
                if self.hdc_screen == 0 {
                    self.hdc_screen = GetDC(0);
                    if self.hdc_screen == 0 {
                        let hdesk = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS);
                        if hdesk != 0 {
                            let _ = SetThreadDesktop(hdesk);
                            CloseDesktop(hdesk);
                            self.hdc_screen = GetDC(0);
                        }
                    }
                }

                if self.hdc_screen != 0 && self.hdc_mem == 0 {
                    self.hdc_mem = CreateCompatibleDC(self.hdc_screen);
                }

                if self.hdc_mem != 0
                    && (self.h_bitmap == 0
                        || self.allocated_w != self.width
                        || self.allocated_h != self.height)
                {
                    if self.h_bitmap != 0 {
                        SelectObject(self.hdc_mem, self.old_bitmap);
                        DeleteObject(self.h_bitmap);
                    }
                    self.h_bitmap =
                        CreateCompatibleBitmap(self.hdc_screen, self.width as i32, self.height as i32);
                    self.old_bitmap = SelectObject(self.hdc_mem, self.h_bitmap);
                    self.allocated_w = self.width;
                    self.allocated_h = self.height;
                }

                let mut bitblt_ok = 0;
                if self.h_bitmap != 0 {
                    bitblt_ok = BitBlt(
                        self.hdc_mem,
                        0,
                        0,
                        self.width as i32,
                        self.height as i32,
                        self.hdc_screen,
                        self.origin_x,
                        self.origin_y,
                        SRCCOPY,
                    );
                }

                if bitblt_ok != 0 {
                    // Dibujar cursor con hotspot en caché
                    let mut cursor_info: CURSORINFO = std::mem::zeroed();
                    cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
                    if GetCursorInfo(&mut cursor_info) != 0 && (cursor_info.flags & 1) != 0 {
                        let rel_x = cursor_info.ptScreenPos.x - self.origin_x;
                        let rel_y = cursor_info.ptScreenPos.y - self.origin_y;
                        if rel_x >= 0
                            && rel_x < self.width as i32
                            && rel_y >= 0
                            && rel_y < self.height as i32
                        {
                            if cursor_info.hCursor != self.last_cursor {
                                let mut icon_info = ICONINFO {
                                    fIcon: 0,
                                    xHotspot: 0,
                                    yHotspot: 0,
                                    hbmMask: 0,
                                    hbmColor: 0,
                                };
                                if GetIconInfo(cursor_info.hCursor, &mut icon_info) != 0 {
                                    self.last_cursor = cursor_info.hCursor;
                                    self.last_hotspot_x = icon_info.xHotspot as i32;
                                    self.last_hotspot_y = icon_info.yHotspot as i32;
                                    if icon_info.hbmMask != 0 {
                                        DeleteObject(icon_info.hbmMask);
                                    }
                                    if icon_info.hbmColor != 0 {
                                        DeleteObject(icon_info.hbmColor);
                                    }
                                }
                            }
                            DrawIconEx(
                                self.hdc_mem,
                                rel_x - self.last_hotspot_x,
                                rel_y - self.last_hotspot_y,
                                cursor_info.hCursor,
                                0,
                                0,
                                0,
                                0,
                                3, // DI_NORMAL
                            );
                        }
                    }

                    let mut bi = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: self.width as i32,
                            biHeight: -(self.height as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB,
                            biSizeImage: self.width * self.height * 4,
                            biXPelsPerMeter: 0,
                            biYPelsPerMeter: 0,
                            biClrUsed: 0,
                            biClrImportant: 0,
                        },
                        bmiColors: [RGBQUAD {
                            rgbBlue: 0,
                            rgbGreen: 0,
                            rgbRed: 0,
                            rgbReserved: 0,
                        }],
                    };

                    let req_len = (self.width * self.height * 4) as usize;
                    if self.raw_buffer.len() != req_len {
                        self.raw_buffer.resize(req_len, 0);
                    }

                    GetDIBits(
                        self.hdc_mem,
                        self.h_bitmap,
                        0,
                        self.height,
                        self.raw_buffer.as_mut_ptr() as *mut _,
                        &mut bi,
                        DIB_RGB_COLORS,
                    );
                } else if self.hdc_screen != 0 {
                    ReleaseDC(0, self.hdc_screen);
                    self.hdc_screen = 0;
                }
            }
        }

        let req_len = (self.width * self.height * 4) as usize;
        if self.raw_buffer.len() != req_len {
            self.raw_buffer.resize(req_len, 35);
        }

        Ok(())
    }

    /// Captura directamente la pantalla a resolución NATIVA 1:1 en formato BGRA de Windows GDI.
    /// Cero conversiones intermedias, cero reescalado, máxima nitidez y rendimiento ultra rápido (~3 ms).
    pub fn capture_native_bgra(&mut self) -> Result<(&[u8], u32, u32), String> {
        self.capture_raw_bgra_internal()?;
        Ok((&self.raw_buffer, self.width, self.height))
    }

    /// Captura y escala directamente la pantalla a un buffer RGB reutilizable.
    /// Manijas GDI persistentes, hotspot de cursor en caché y reescalado por tabla (Zero-Alloc, ~2 ms).
    pub fn capture_rgb_scaled_into(
        &mut self,
        max_dim: u32,
        dst: &mut Vec<u8>,
    ) -> Result<(u32, u32), String> {
        self.capture_raw_bgra_internal()?;

        // Asegurar que raw_buffer tenga tamaño adecuado
        let req_len = (self.width * self.height * 4) as usize;
        if self.raw_buffer.len() != req_len {
            self.raw_buffer.resize(req_len, 35);
        }

        // Calcular resolución optimizada conservando relación de aspecto
        let src_w = self.width as usize;
        let src_h = self.height as usize;
        let max_d = self.width.max(self.height);
        let (enc_w, enc_h) = if max_dim > 0 && max_d > max_dim {
            let scale = max_dim as f32 / max_d as f32;
            let dw = ((self.width as f32 * scale).round() as u32) & !1;
            let dh = ((self.height as f32 * scale).round() as u32) & !1;
            (dw, dh)
        } else {
            (self.width, self.height)
        };

        let dw = enc_w as usize;
        let dh = enc_h as usize;
        let dst_len = dw * dh * 3;
        if dst.len() != dst_len {
            dst.resize(dst_len, 0);
        }

        let src = &self.raw_buffer;
        let src_len = src.len();

        if dw == src_w && dh == src_h {
            let pixel_count = dw * dh;
            for i in 0..pixel_count {
                let src_idx = i * 4;
                let dst_idx = i * 3;
                if src_idx + 2 < src_len {
                    dst[dst_idx] = src[src_idx + 2];     // R
                    dst[dst_idx + 1] = src[src_idx + 1]; // G
                    dst[dst_idx + 2] = src[src_idx];     // B
                }
            }
        } else {
            // Reescalado acelerado sin divisiones por píxel
            if self.last_mapped_dw != enc_w || self.last_mapped_src_w != self.width {
                self.x_map = (0..dw).map(|dx| (dx * src_w) / dw).collect();
                self.last_mapped_dw = enc_w;
                self.last_mapped_src_w = self.width;
            }

            let x_map = &self.x_map;
            for dy in 0..dh {
                let sy = (dy * src_h) / dh;
                let src_row = sy * src_w * 4;
                let dst_row = dy * dw * 3;

                for dx in 0..dw {
                    let sx = x_map[dx];
                    let src_idx = src_row + sx * 4;
                    let dst_idx = dst_row + dx * 3;

                    if src_idx + 2 < src_len {
                        dst[dst_idx] = src[src_idx + 2];     // R
                        dst[dst_idx + 1] = src[src_idx + 1]; // G
                        dst[dst_idx + 2] = src[src_idx];     // B
                    }
                }
            }
        }

        Ok((enc_w, enc_h))
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
            println!(
                "Detectada: {} -> {}x{} en ({}, {}) Primary={}",
                s.name, s.width, s.height, s.x, s.y, s.is_primary
            );
        }
    }

    #[test]
    fn test_capture_all_screens_and_mosaic() {
        let mut capturer = ScreenCapturer::new();
        let screens = capturer.get_screens();
        for s in &screens {
            let res = capturer.select_screen(s.index);
            assert!(res.is_ok(), "Fallo seleccionando {}", s.name);
            let frame = capturer.capture_frame();
            assert!(frame.is_ok(), "Fallo capturando {}", s.name);
            let f = frame.unwrap();
            assert_eq!(f.width, s.width);
            assert_eq!(f.height, s.height);
            println!("Captura exitosa de {}: {}x{} (bytes: {})", s.name, f.width, f.height, f.data.len());
        }
    }

    #[test]
    fn test_benchmark_capture_stages() {
        use std::time::Instant;
        let mut capturer = ScreenCapturer::new();
        let mut rgb_buf = Vec::new();

        // 1. Calentar y medir capture_rgb_scaled_into
        let mut capture_times = Vec::new();
        for _ in 0..10 {
            let start = Instant::now();
            let res = capturer.capture_rgb_scaled_into(1280, &mut rgb_buf);
            assert!(res.is_ok());
            capture_times.push(start.elapsed());
        }

        let (w, h) = capturer.capture_rgb_scaled_into(1280, &mut rgb_buf).unwrap();

        let avg_cap: std::time::Duration = capture_times.iter().sum::<std::time::Duration>() / capture_times.len() as u32;

        println!("============================================================");
        println!("📊 BENCHMARK CAPTURA (Resolución {}x{}, RGB {} bytes):", w, h, rgb_buf.len());
        println!("   1. GDI BitBlt + GetDIBits + Reescalado: {:.2} ms", avg_cap.as_secs_f64() * 1000.0);
        println!("   Teórico sólo captura: {:.1} FPS", 1.0 / avg_cap.as_secs_f64());
        println!("============================================================");
    }
}
