//! Capturador nativo de pantalla basado en DirectX 11 / DXGI Desktop Duplication API (Windows 8/10/11).
//! 
//! Ofrece captura de fotogramas directamente desde la VRAM de la GPU sin pasar por GDI ni bloquearse
//! por VSync (latencia de captura típica: < 1.5 ms frente a 31.6 ms de GDI BitBlt).
//! Compatible con GPUs dedicadas (NVIDIA/AMD) y gráficos integrados (Intel HD/UHD/Iris).

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;
use std::time::Instant;
use windows_sys::Win32::Foundation::{POINT, S_OK};

pub type HRESULT = i32;

pub const DXGI_ERROR_WAIT_TIMEOUT: HRESULT = 0x887A0027u32 as i32;
pub const DXGI_ERROR_ACCESS_LOST: HRESULT = 0x887A0026u32 as i32;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

pub const IID_IUNKNOWN: GUID = GUID {
    data1: 0x00000000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

pub const IID_IDXGIDEVICE: GUID = GUID {
    data1: 0x54ec77fa,
    data2: 0x1377,
    data3: 0x44e6,
    data4: [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c],
};

pub const IID_IDXGIRESOURCE: GUID = GUID {
    data1: 0x035f3ab4,
    data2: 0x482e,
    data3: 0x4e50,
    data4: [0xb4, 0x1f, 0x8a, 0x7f, 0x8b, 0xd8, 0x96, 0x0b],
};

pub const IID_IDXGIOUTPUT1: GUID = GUID {
    data1: 0x00cddea8,
    data2: 0x939b,
    data3: 0x4b83,
    data4: [0xa3, 0x40, 0xa6, 0x85, 0x22, 0x66, 0x66, 0xcc],
};

pub const IID_ID3D11TEXTURE2D: GUID = GUID {
    data1: 0x6f15aaf2,
    data2: 0xd208,
    data3: 0x4e89,
    data4: [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c],
};

#[repr(C)]
pub struct DXGI_SAMPLE_DESC {
    pub Count: u32,
    pub Quality: u32,
}

#[repr(C)]
pub struct D3D11_TEXTURE2D_DESC {
    pub Width: u32,
    pub Height: u32,
    pub MipLevels: u32,
    pub ArraySize: u32,
    pub Format: u32,
    pub SampleDesc: DXGI_SAMPLE_DESC,
    pub Usage: u32,
    pub BindFlags: u32,
    pub CPUAccessFlags: u32,
    pub MiscFlags: u32,
}

#[repr(C)]
pub struct D3D11_MAPPED_SUBRESOURCE {
    pub pData: *mut c_void,
    pub RowPitch: u32,
    pub DepthPitch: u32,
}

#[repr(C)]
pub struct DXGI_OUTDUPL_POINTER_POSITION {
    pub Position: POINT,
    pub Visible: i32,
}

#[repr(C)]
pub struct DXGI_OUTDUPL_FRAME_INFO {
    pub LastPresentTime: i64,
    pub LastMouseUpdateTime: i64,
    pub AccumulatedFrames: u32,
    pub RectsCoalesced: i32,
    pub ProtectedContentMaskedOut: i32,
    pub PointerPosition: DXGI_OUTDUPL_POINTER_POSITION,
    pub TotalMetadataBufferSize: u32,
    pub PointerShapeBufferSize: u32,
}

type PFN_D3D11_CREATE_DEVICE = unsafe extern "system" fn(
    *mut c_void,
    u32,
    *mut c_void,
    u32,
    *const u32,
    u32,
    u32,
    *mut *mut c_void,
    *mut u32,
    *mut *mut c_void,
) -> HRESULT;

#[inline]
unsafe fn com_call<T>(obj: *mut c_void, vtbl_index: usize) -> T {
    let vtable = *(obj as *mut *mut *mut c_void);
    let method = *vtable.add(vtbl_index);
    std::mem::transmute_copy(&method)
}

#[inline]
unsafe fn com_release(obj: *mut c_void) {
    if !obj.is_null() {
        let release: unsafe extern "system" fn(*mut c_void) -> u32 = com_call(obj, 2);
        release(obj);
    }
}

pub struct DxgiCapturer {
    device: *mut c_void,
    context: *mut c_void,
    duplication: *mut c_void,
    staging_tex: *mut c_void,
    pub width: u32,
    pub height: u32,
    output_index: u32,
    pub raw_bgra: Vec<u8>,
    x_map: Vec<usize>,
    last_mapped_dw: u32,
    last_mapped_src_w: u32,
    pub last_acquire_us: u32,
    pub last_copy_us: u32,
    pub last_scale_us: u32,
}

unsafe impl Send for DxgiCapturer {}

impl DxgiCapturer {
    pub fn new(output_index: u32) -> Result<Self, String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

            unsafe {
                let h_d3d11 = LoadLibraryA(b"d3d11.dll\0".as_ptr());
                if h_d3d11 == 0 {
                    return Err("No se pudo cargar d3d11.dll".to_string());
                }

                let proc = GetProcAddress(h_d3d11, b"D3D11CreateDevice\0".as_ptr());
                if proc.is_none() {
                    return Err("No se encontró D3D11CreateDevice en d3d11.dll".to_string());
                }
                let D3D11CreateDevice: PFN_D3D11_CREATE_DEVICE = std::mem::transmute(proc.unwrap());

                // Crear D3D11 Device (Hardware)
                let mut device: *mut c_void = std::ptr::null_mut();
                let mut context: *mut c_void = std::ptr::null_mut();
                let mut feature_level: u32 = 0;

                // Flags: D3D11_CREATE_DEVICE_BGRA_SUPPORT = 0x20
                let hr = D3D11CreateDevice(
                    std::ptr::null_mut(),
                    1, // D3D_DRIVER_TYPE_HARDWARE = 1
                    std::ptr::null_mut(),
                    0x20, // BGRA_SUPPORT
                    std::ptr::null(),
                    0,
                    7, // D3D11_SDK_VERSION
                    &mut device,
                    &mut feature_level,
                    &mut context,
                );

                if hr != S_OK || device.is_null() {
                    return Err(format!("D3D11CreateDevice falló con HRESULT 0x{:08X}", hr as u32));
                }

                // QueryInterface -> IDXGIDevice
                let mut dxgi_device: *mut c_void = std::ptr::null_mut();
                let qi_device: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT =
                    com_call(device, 0);
                let hr = qi_device(device, &IID_IDXGIDEVICE, &mut dxgi_device);
                if hr != S_OK || dxgi_device.is_null() {
                    com_release(context);
                    com_release(device);
                    return Err(format!("QueryInterface IDXGIDevice falló: 0x{:08X}", hr as u32));
                }

                // IDXGIDevice::GetAdapter (index 7)
                let mut adapter: *mut c_void = std::ptr::null_mut();
                let get_adapter: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT =
                    com_call(dxgi_device, 7);
                let hr = get_adapter(dxgi_device, &mut adapter);
                com_release(dxgi_device);

                if hr != S_OK || adapter.is_null() {
                    com_release(context);
                    com_release(device);
                    return Err(format!("GetAdapter falló: 0x{:08X}", hr as u32));
                }

                // IDXGIAdapter::EnumOutputs (index 7)
                let mut output: *mut c_void = std::ptr::null_mut();
                let enum_outputs: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT =
                    com_call(adapter, 7);
                let hr = enum_outputs(adapter, output_index, &mut output);
                com_release(adapter);

                if hr != S_OK || output.is_null() {
                    com_release(context);
                    com_release(device);
                    return Err(format!("EnumOutputs({}) falló: 0x{:08X}", output_index, hr as u32));
                }

                // QueryInterface -> IDXGIOutput1 (index 0)
                let mut output1: *mut c_void = std::ptr::null_mut();
                let qi_output: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT =
                    com_call(output, 0);
                let hr = qi_output(output, &IID_IDXGIOUTPUT1, &mut output1);
                com_release(output);

                if hr != S_OK || output1.is_null() {
                    com_release(context);
                    com_release(device);
                    return Err(format!("QueryInterface IDXGIOutput1 falló: 0x{:08X}", hr as u32));
                }

                // Asociar el hilo de captura al escritorio interactivo activo (Input Desktop)
                use windows_sys::Win32::System::StationsAndDesktops::*;
                let hdesk = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS);
                if hdesk != 0 {
                    let _ = SetThreadDesktop(hdesk);
                }

                // IDXGIOutput1::DuplicateOutput (index 22)
                let mut duplication: *mut c_void = std::ptr::null_mut();
                let duplicate_output: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT =
                    com_call(output1, 22);
                let hr = duplicate_output(output1, device, &mut duplication);
                com_release(output1);

                if hr != S_OK || duplication.is_null() {
                    com_release(context);
                    com_release(device);
                    return Err(format!("DuplicateOutput falló: 0x{:08X}", hr as u32));
                }

                // IDXGIOutputDuplication::GetDesc (index 7)
                #[repr(C)]
                struct DXGI_OUTDUPL_DESC {
                    ModeDesc: [u8; 28],
                    Rotation: u32,
                    DesktopImageInSystemMemory: i32,
                }
                let mut desc: DXGI_OUTDUPL_DESC = std::mem::zeroed();
                let get_desc: unsafe extern "system" fn(*mut c_void, *mut DXGI_OUTDUPL_DESC) =
                    com_call(duplication, 7);
                get_desc(duplication, &mut desc);

                let width = u32::from_ne_bytes(desc.ModeDesc[0..4].try_into().unwrap_or_default());
                let height = u32::from_ne_bytes(desc.ModeDesc[4..8].try_into().unwrap_or_default());

                if width == 0 || height == 0 {
                    com_release(duplication);
                    com_release(context);
                    com_release(device);
                    return Err("Dimensiones de pantalla inválidas desde DXGI".to_string());
                }

                // Crear Staging Texture para lectura por CPU (D3D11_USAGE_STAGING = 3, D3D11_CPU_ACCESS_READ = 0x20000)
                let tex_desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: 87, // DXGI_FORMAT_B8G8R8A8_UNORM = 87
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: 3, // D3D11_USAGE_STAGING
                    BindFlags: 0,
                    CPUAccessFlags: 0x20000, // D3D11_CPU_ACCESS_READ
                    MiscFlags: 0,
                };

                let mut staging_tex: *mut c_void = std::ptr::null_mut();
                let create_tex: unsafe extern "system" fn(*mut c_void, *const D3D11_TEXTURE2D_DESC, *const c_void, *mut *mut c_void) -> HRESULT =
                    com_call(device, 5);
                let hr = create_tex(device, &tex_desc, std::ptr::null(), &mut staging_tex);

                if hr != S_OK || staging_tex.is_null() {
                    com_release(duplication);
                    com_release(context);
                    com_release(device);
                    return Err(format!("CreateTexture2D staging falló: 0x{:08X}", hr as u32));
                }

                let pixel_count = (width * height * 4) as usize;
                Ok(Self {
                    device,
                    context,
                    duplication,
                    staging_tex,
                    width,
                    height,
                    output_index,
                    raw_bgra: vec![0u8; pixel_count],
                    x_map: Vec::new(),
                    last_mapped_dw: 0,
                    last_mapped_src_w: 0,
                    last_acquire_us: 0,
                    last_copy_us: 0,
                    last_scale_us: 0,
                })
            }
        }
        #[cfg(not(windows))]
        {
            let _ = output_index;
            Err("DirectX DXGI solo está disponible en Windows".to_string())
        }
    }

    /// Captura un fotograma directamente desde DirectX 11 VRAM.
    /// Si la pantalla no cambió, retorna Ok(false) en ~0.05 ms sin costo de copia.
    /// Si hay nuevo fotograma, lo copia y retorna Ok(true) en ~1.0 ms.
    pub fn capture_to_bgra(&mut self, timeout_ms: u32) -> Result<bool, String> {
        let t0 = Instant::now();
        unsafe {
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = std::mem::zeroed();
            let mut desktop_res: *mut c_void = std::ptr::null_mut();

            // IDXGIOutputDuplication::AcquireNextFrame (index 8)
            let acquire: unsafe extern "system" fn(*mut c_void, u32, *mut DXGI_OUTDUPL_FRAME_INFO, *mut *mut c_void) -> HRESULT =
                com_call(self.duplication, 8);
            let hr = acquire(self.duplication, timeout_ms, &mut frame_info, &mut desktop_res);

            if hr == DXGI_ERROR_WAIT_TIMEOUT {
                self.last_acquire_us = t0.elapsed().as_micros() as u32;
                return Ok(false); // Sin cambios en pantalla, mantener fotograma previo
            }

            if hr != S_OK || desktop_res.is_null() {
                return Err(format!("AcquireNextFrame falló con HRESULT 0x{:08X}", hr as u32));
            }
            self.last_acquire_us = t0.elapsed().as_micros() as u32;

            let t_copy = Instant::now();
            // QueryInterface -> ID3D11Texture2D (index 0)
            let mut desktop_tex: *mut c_void = std::ptr::null_mut();
            let qi: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT =
                com_call(desktop_res, 0);
            let hr = qi(desktop_res, &IID_ID3D11TEXTURE2D, &mut desktop_tex);
            com_release(desktop_res);

            if hr != S_OK || desktop_tex.is_null() {
                // Liberar fotograma antes de salir
                let release_frame: unsafe extern "system" fn(*mut c_void) -> HRESULT =
                    com_call(self.duplication, 14);
                let _ = release_frame(self.duplication);
                return Err(format!("QueryInterface ID3D11Texture2D falló: 0x{:08X}", hr as u32));
            }

            // ID3D11DeviceContext::CopyResource (index 47)
            let copy_res: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) =
                com_call(self.context, 47);
            copy_res(self.context, self.staging_tex, desktop_tex);
            com_release(desktop_tex);

            // IDXGIOutputDuplication::ReleaseFrame (index 14)
            let release_frame: unsafe extern "system" fn(*mut c_void) -> HRESULT =
                com_call(self.duplication, 14);
            let _ = release_frame(self.duplication);

            // ID3D11DeviceContext::Map (index 14)
            let mut mapped: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
            let map: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32, u32, *mut D3D11_MAPPED_SUBRESOURCE) -> HRESULT =
                com_call(self.context, 14);
            let hr = map(self.context, self.staging_tex, 0, 1 /* READ */, 0, &mut mapped);

            if hr != S_OK || mapped.pData.is_null() {
                return Err(format!("Map staging texture falló: 0x{:08X}", hr as u32));
            }

            // Copia de renglones respetando RowPitch
            let src_pitch = mapped.RowPitch as usize;
            let dst_pitch = (self.width * 4) as usize;
            let h = self.height as usize;
            let src_ptr = mapped.pData as *const u8;
            let dst_ptr = self.raw_bgra.as_mut_ptr();

            if src_pitch == dst_pitch {
                std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, dst_pitch * h);
            } else {
                for y in 0..h {
                    std::ptr::copy_nonoverlapping(
                        src_ptr.add(y * src_pitch),
                        dst_ptr.add(y * dst_pitch),
                        dst_pitch,
                    );
                }
            }

            // ID3D11DeviceContext::Unmap (index 15)
            let unmap: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) =
                com_call(self.context, 15);
            unmap(self.context, self.staging_tex, 0);

            self.last_copy_us = t_copy.elapsed().as_micros() as u32;
            Ok(true)
        }
    }

    /// Captura y escala directamente a RGB en una sola pasada usando punteros nativos (VSync desactivado)
    pub fn capture_rgb_scaled_into(
        &mut self,
        max_dim: u32,
        dst: &mut Vec<u8>,
    ) -> Result<(u32, u32), String> {
        let changed = self.capture_to_bgra(4)?; // 4 ms: captura responsiva sin bloqueo de VSync que permite atrapar nuevos fotogramas del compositor
        let _ = changed;

        let t_scale = Instant::now();
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

        let src = &self.raw_bgra;
        let src_len = src.len();

        if dw == src_w && dh == src_h {
            let pixel_count = dw * dh;
            let src_ptr = src.as_ptr();
            let dst_ptr = dst.as_mut_ptr();
            unsafe {
                for i in 0..pixel_count {
                    let src_idx = i * 4;
                    let dst_idx = i * 3;
                    if src_idx + 2 < src_len {
                        *dst_ptr.add(dst_idx) = *src_ptr.add(src_idx + 2); // R
                        *dst_ptr.add(dst_idx + 1) = *src_ptr.add(src_idx + 1); // G
                        *dst_ptr.add(dst_idx + 2) = *src_ptr.add(src_idx); // B
                    }
                }
            }
        } else {
            if self.last_mapped_dw != enc_w || self.last_mapped_src_w != self.width {
                self.x_map = (0..dw).map(|dx| (dx * src_w) / dw).collect();
                self.last_mapped_dw = enc_w;
                self.last_mapped_src_w = self.width;
            }

            let src_ptr = src.as_ptr();
            let dst_ptr = dst.as_mut_ptr();
            let x_map = &self.x_map;

            unsafe {
                for dy in 0..dh {
                    let sy = (dy * src_h) / dh;
                    let src_row = sy * src_w * 4;
                    let dst_row = dy * dw * 3;

                    for dx in 0..dw {
                        let sx = x_map[dx];
                        let s_idx = src_row + sx * 4;
                        let d_idx = dst_row + dx * 3;

                        if s_idx + 2 < src_len {
                            *dst_ptr.add(d_idx) = *src_ptr.add(s_idx + 2); // R
                            *dst_ptr.add(d_idx + 1) = *src_ptr.add(s_idx + 1); // G
                            *dst_ptr.add(d_idx + 2) = *src_ptr.add(s_idx); // B
                        }
                    }
                }
            }
        }

        self.last_scale_us = t_scale.elapsed().as_micros() as u32;
        Ok((enc_w, enc_h))
    }

    pub fn last_timings_us(&self) -> (u32, u32, u32) {
        (self.last_acquire_us, self.last_copy_us, self.last_scale_us)
    }
}

impl Drop for DxgiCapturer {
    fn drop(&mut self) {
        unsafe {
            com_release(self.staging_tex);
            com_release(self.duplication);
            com_release(self.context);
            com_release(self.device);
        }
    }
}
