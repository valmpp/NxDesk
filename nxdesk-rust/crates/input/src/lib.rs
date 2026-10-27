use nxdesk_core::{InputEvent, MouseButton};

/// Servicio de inyección de eventos de ratón y teclado nativos
pub struct InputInjector {
    screen_x: i32,
    screen_y: i32,
    screen_width: i32,
    screen_height: i32,
}

impl Default for InputInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl InputInjector {
    pub fn new() -> Self {
        Self {
            screen_x: 0,
            screen_y: 0,
            screen_width: 1920,
            screen_height: 1080,
        }
    }

    /// Configura los límites de la pantalla activa (para configuraciones multimonitor)
    pub fn set_screen_bounds(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.screen_x = x;
        self.screen_y = y;
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Inyecta un evento recibido por la red
    pub fn inject(&self, event: &InputEvent) {
        match event {
            InputEvent::MouseMove { x, y } => self.inject_mouse_move(*x, *y),
            InputEvent::MouseDown { button, .. } => self.inject_mouse_button(*button, true),
            InputEvent::MouseUp { button, .. } => self.inject_mouse_button(*button, false),
            InputEvent::MouseWheel { delta_x, delta_y } => {
                self.inject_mouse_wheel(*delta_x, *delta_y)
            }
            InputEvent::KeyDown { key_code, .. } => self.inject_key(*key_code as u32, true),
            InputEvent::KeyUp { key_code, .. } => self.inject_key(*key_code as u32, false),
        }
    }

    #[cfg(windows)]
    fn inject_mouse_move(&self, normalized_x: f32, normalized_y: f32) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        unsafe {
            let virtual_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let virtual_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let mut virtual_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let mut virtual_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

            if virtual_w <= 0 {
                virtual_w = 1920;
            }
            if virtual_h <= 0 {
                virtual_h = 1080;
            }

            let abs_x = self.screen_x + (normalized_x * self.screen_width as f32) as i32;
            let abs_y = self.screen_y + (normalized_y * self.screen_height as f32) as i32;

            let mapped_x = (((abs_x - virtual_x) as f64 * 65535.0) / virtual_w as f64) as i32;
            let mapped_y = (((abs_y - virtual_y) as f64 * 65535.0) / virtual_h as f64) as i32;

            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: mapped_x,
                        dy: mapped_y,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                        time: 0,
                        dwExtraInfo: 0x584E4453,
                    },
                },
            };

            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(not(windows))]
    fn inject_mouse_move(&self, _x: f32, _y: f32) {}

    #[cfg(windows)]
    fn inject_mouse_button(&self, button: MouseButton, is_down: bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

        let flags = match button {
            MouseButton::Left => {
                if is_down {
                    MOUSEEVENTF_LEFTDOWN
                } else {
                    MOUSEEVENTF_LEFTUP
                }
            }
            MouseButton::Right => {
                if is_down {
                    MOUSEEVENTF_RIGHTDOWN
                } else {
                    MOUSEEVENTF_RIGHTUP
                }
            }
            MouseButton::Middle => {
                if is_down {
                    MOUSEEVENTF_MIDDLEDOWN
                } else {
                    MOUSEEVENTF_MIDDLEUP
                }
            }
        };

        unsafe {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0x584E4453,
                    },
                },
            };

            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(not(windows))]
    fn inject_mouse_button(&self, _button: MouseButton, _is_down: bool) {}

    #[cfg(windows)]
    fn inject_mouse_wheel(&self, _delta_x: f32, delta_y: f32) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

        if delta_y != 0.0 {
            unsafe {
                let wheel_delta = (delta_y * 120.0) as i32;
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: wheel_delta as u32,
                            dwFlags: MOUSEEVENTF_WHEEL,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    #[cfg(not(windows))]
    fn inject_mouse_wheel(&self, _delta_x: f32, _delta_y: f32) {}

    #[cfg(windows)]
    fn inject_key(&self, key_code: u32, is_down: bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

        let flags = if is_down { 0 } else { KEYEVENTF_KEYUP };

        unsafe {
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key_code as u16,
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(not(windows))]
    fn inject_key(&self, _key_code: u32, _is_down: bool) {}
}
