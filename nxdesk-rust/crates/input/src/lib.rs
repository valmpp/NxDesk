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
            InputEvent::MouseDown { button, x, y } => {
                if *x > 0.0 || *y > 0.0 {
                    self.inject_mouse_move(*x, *y);
                }
                self.inject_mouse_button(*button, true);
            }
            InputEvent::MouseUp { button, x, y } => {
                if *x > 0.0 || *y > 0.0 {
                    self.inject_mouse_move(*x, *y);
                }
                self.inject_mouse_button(*button, false);
            }
            InputEvent::MouseWheel { delta_x, delta_y } => {
                self.inject_mouse_wheel(*delta_x, *delta_y)
            }
            InputEvent::KeyDown { key_code, key_char } => {
                if let Some(ref text) = key_char {
                    if !text.is_empty() {
                        self.inject_unicode_str(text);
                        return;
                    }
                }
                if *key_code != 0 {
                    self.inject_key(*key_code as u32, true);
                }
            }
            InputEvent::KeyUp { key_code, .. } => {
                if *key_code != 0 {
                    self.inject_key(*key_code as u32, false);
                }
            }
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
    fn is_extended_key(vk: u16) -> bool {
        matches!(
            vk,
            0x21..=0x28 // PageUp, PageDown, End, Home, Left, Up, Right, Down
            | 0x2D | 0x2E // Insert, Delete
            | 0x5B | 0x5C | 0x5D // LWIN, RWIN, APPS
            | 0xA3 // RCONTROL
            | 0xA5 // RMENU (Right Alt / AltGr)
        )
    }

    #[cfg(windows)]
    fn inject_key(&self, key_code: u32, is_down: bool) {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

        let mut flags = if is_down { 0 } else { KEYEVENTF_KEYUP };
        if Self::is_extended_key(key_code as u16) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }

        let scan_code = unsafe { MapVirtualKeyW(key_code, 0) } as u16;

        unsafe {
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key_code as u16,
                        wScan: scan_code,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let ret = SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
            if ret == 0 {
                let err = GetLastError();
                tracing::warn!(
                    "⚠️ [InputInjector] SendInput(VK 0x{:02X}, down={}) falló con Win32 error={}",
                    key_code,
                    is_down,
                    err
                );
            }
        }
    }

    #[cfg(windows)]
    fn inject_unicode_char(&self, ch: char) {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

        let mut utf16_buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut utf16_buf);

        for &code_unit in encoded.iter() {
            unsafe {
                // KeyDown
                let input_down = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: code_unit,
                            dwFlags: KEYEVENTF_UNICODE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                let ret_down = SendInput(1, &input_down, std::mem::size_of::<INPUT>() as i32);
                if ret_down == 0 {
                    tracing::warn!(
                        "⚠️ [InputInjector] SendInput(UNICODE down U+{:04X}) falló con Win32 error={}",
                        code_unit,
                        GetLastError()
                    );
                }

                // KeyUp
                let input_up = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: code_unit,
                            dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                let ret_up = SendInput(1, &input_up, std::mem::size_of::<INPUT>() as i32);
                if ret_up == 0 {
                    tracing::warn!(
                        "⚠️ [InputInjector] SendInput(UNICODE up U+{:04X}) falló con Win32 error={}",
                        code_unit,
                        GetLastError()
                    );
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn inject_unicode_char(&self, _ch: char) {}

    /// Inyecta una cadena de texto como caracteres Unicode nativos vía KEYEVENTF_UNICODE
    pub fn inject_unicode_str(&self, text: &str) {
        for ch in text.chars() {
            self.inject_unicode_char(ch);
        }
    }

    #[cfg(not(windows))]
    fn inject_key(&self, _key_code: u32, _is_down: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_extended_key() {
        // Extended keys (flechas, navegación, Win, modificadores derechos)
        assert!(InputInjector::is_extended_key(0x21)); // PageUp
        assert!(InputInjector::is_extended_key(0x22)); // PageDown
        assert!(InputInjector::is_extended_key(0x23)); // End
        assert!(InputInjector::is_extended_key(0x24)); // Home
        assert!(InputInjector::is_extended_key(0x25)); // Left
        assert!(InputInjector::is_extended_key(0x26)); // Up
        assert!(InputInjector::is_extended_key(0x27)); // Right
        assert!(InputInjector::is_extended_key(0x28)); // Down
        assert!(InputInjector::is_extended_key(0x2D)); // Insert
        assert!(InputInjector::is_extended_key(0x2E)); // Delete
        assert!(InputInjector::is_extended_key(0x5B)); // LWIN
        assert!(InputInjector::is_extended_key(0x5C)); // RWIN
        assert!(InputInjector::is_extended_key(0xA3)); // RCONTROL
        assert!(InputInjector::is_extended_key(0xA5)); // RMENU (AltGr)

        // Non-extended keys
        assert!(!InputInjector::is_extended_key(0x41)); // 'A'
        assert!(!InputInjector::is_extended_key(0x10)); // Shift
        assert!(!InputInjector::is_extended_key(0xA2)); // LCONTROL
        assert!(!InputInjector::is_extended_key(0xA4)); // LMENU (Left Alt)
        assert!(!InputInjector::is_extended_key(0x0D)); // Enter
    }

    #[test]
    fn test_injector_dispatches_events_without_panic() {
        let injector = InputInjector::new();

        // 1. Teclas alfanuméricas
        injector.inject(&InputEvent::KeyDown { key_code: 0x41, key_char: None });
        injector.inject(&InputEvent::KeyUp { key_code: 0x41 });

        // 2. Símbolos OEM (. , / \ -)
        injector.inject(&InputEvent::KeyDown { key_code: 0xBE, key_char: None }); // .
        injector.inject(&InputEvent::KeyUp { key_code: 0xBE });
        injector.inject(&InputEvent::KeyDown { key_code: 0xBC, key_char: None }); // ,
        injector.inject(&InputEvent::KeyUp { key_code: 0xBC });
        injector.inject(&InputEvent::KeyDown { key_code: 0xBD, key_char: None }); // -
        injector.inject(&InputEvent::KeyUp { key_code: 0xBD });
        injector.inject(&InputEvent::KeyDown { key_code: 0xBF, key_char: None }); // /
        injector.inject(&InputEvent::KeyUp { key_code: 0xBF });
        injector.inject(&InputEvent::KeyDown { key_code: 0xDC, key_char: None }); // \
        injector.inject(&InputEvent::KeyUp { key_code: 0xDC });

        // 3. Caracteres Unicode y símbolos internacionales (@ # $ % & ñ ¿ ¡)
        let unicode_symbols = ["ñ", "¿", "¡", "@", "#", "$", "%", "&", "á", "€"];
        for sym in unicode_symbols {
            injector.inject(&InputEvent::KeyDown {
                key_code: 0,
                key_char: Some(sym.to_string()),
            });
        }

        // 4. Atajos (Ctrl+C, Ctrl+V, Alt+Tab, Win)
        // Ctrl+C
        injector.inject(&InputEvent::KeyDown { key_code: 0xA2, key_char: None }); // LControl
        injector.inject(&InputEvent::KeyDown { key_code: 0x43, key_char: None }); // C
        injector.inject(&InputEvent::KeyUp { key_code: 0x43 });
        injector.inject(&InputEvent::KeyUp { key_code: 0xA2 });

        // Ctrl+V
        injector.inject(&InputEvent::KeyDown { key_code: 0xA2, key_char: None }); // LControl
        injector.inject(&InputEvent::KeyDown { key_code: 0x56, key_char: None }); // V
        injector.inject(&InputEvent::KeyUp { key_code: 0x56 });
        injector.inject(&InputEvent::KeyUp { key_code: 0xA2 });

        // Alt+Tab
        injector.inject(&InputEvent::KeyDown { key_code: 0xA4, key_char: None }); // LAlt
        injector.inject(&InputEvent::KeyDown { key_code: 0x09, key_char: None }); // Tab
        injector.inject(&InputEvent::KeyUp { key_code: 0x09 });
        injector.inject(&InputEvent::KeyUp { key_code: 0xA4 });

        // Win
        injector.inject(&InputEvent::KeyDown { key_code: 0x5B, key_char: None }); // LWin
        injector.inject(&InputEvent::KeyUp { key_code: 0x5B });

        // AltGr+Q
        injector.inject(&InputEvent::KeyDown { key_code: 0xA5, key_char: None }); // RMenu (AltGr)
        injector.inject(&InputEvent::KeyDown { key_code: 0, key_char: Some("@".to_string()) });
        injector.inject(&InputEvent::KeyUp { key_code: 0xA5 });
    }
}
