use crate::state::AppState;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use nxdesk_core::{
    DirectoryItemDto, InputEvent, MouseButton, PeerIdentity, SessionDecisionDto, SessionRole,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Paleta de colores idéntica a Dark.axaml de C# / Avalonia
pub mod theme {
    use eframe::egui::Color32;

    pub const BACKGROUND: Color32 = Color32::from_rgb(30, 30, 30);          // #1E1E1E
    pub const BACKGROUND_SECONDARY: Color32 = Color32::from_rgb(37, 37, 38); // #252526
    pub const BACKGROUND_DARK: Color32 = Color32::from_rgb(18, 18, 18);      // #121212
    pub const SURFACE: Color32 = Color32::from_rgb(42, 42, 42);              // #2A2A2A (DeviceCard)
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(62, 62, 66);        // #3E3E42
    pub const BORDER: Color32 = Color32::from_rgb(51, 51, 51);               // #333333
    pub const PRIMARY: Color32 = Color32::from_rgb(0, 122, 204);             // #007ACC (NxDesk Blue)
    pub const PRIMARY_HOVER: Color32 = Color32::from_rgb(28, 151, 234);      // #1C97EA
    pub const TEXT: Color32 = Color32::from_rgb(255, 255, 255);              // #FFFFFF
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(204, 204, 204);     // #CCCCCC
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(136, 136, 136);        // #888888
    pub const SUCCESS: Color32 = Color32::from_rgb(0, 204, 102);             // #00CC66 (Status Dot)
    pub const ERROR: Color32 = Color32::from_rgb(209, 52, 56);               // #D13438 (Disconnect)
}

pub struct NxDeskApp {
    state: AppState,
    target_id_input: String,
    is_secret_visible: bool,
    is_admin_mode: bool,
    is_recording: bool,
    is_screen_locked: bool,
    is_settings_open: bool,
    is_files_open: bool,
    is_terminal_open: bool,
    current_path: String,
    files_list: Vec<DirectoryItemDto>,
    terminal_input: String,
    terminal_output: String,
    terminal_history: Vec<String>,
    terminal_history_idx: Option<usize>,
    allow_terminal: bool,
    terminal_focus_requested: bool,
    toast_message: Option<(String, Instant)>,
    selected_role: SessionRole,
    allow_input: bool,
    allow_clipboard: bool,
    allow_files: bool,
    remote_texture: Option<egui::TextureHandle>,
    last_rendered_frame_seq: u64,
    last_mouse_pos: (f32, f32),
    last_mouse_time: Instant,
    session_start_time: Option<Instant>,
    recording_start_time: Option<Instant>,
    selected_screen_index: i32,
    /// Modo de visualización de pantalla remota: true = Ajustar a Ventana (Letterbox), false = 1:1 Nativo Original
    is_view_fit: bool,
    /// Suscripción al broadcast de salida de terminal (local + llegada del par remoto)
    terminal_rx: tokio::sync::broadcast::Receiver<String>,
}

impl NxDeskApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppState) -> Self {
        // Configurar tema oscuro nativo idéntico al tema Dark.axaml de C#
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(theme::TEXT);
        visuals.panel_fill = theme::BACKGROUND;
        visuals.window_fill = theme::BACKGROUND_SECONDARY;
        visuals.widgets.noninteractive.bg_fill = theme::SURFACE;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme::BORDER);
        visuals.widgets.inactive.bg_fill = theme::SURFACE;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, theme::BORDER);
        visuals.widgets.hovered.bg_fill = theme::SURFACE_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, theme::PRIMARY);
        visuals.widgets.active.bg_fill = theme::PRIMARY;
        cc.egui_ctx.set_visuals(visuals);

        let initial_path = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| "C:\\".to_string());

        state.set_egui_ctx(cc.egui_ctx.clone());

        // Suscribirse ANTES de mover `state` al literal del struct (orden de evaluación de campos)
        let terminal_rx = state.terminal_broadcast_tx.subscribe();

        Self {
            state,
            target_id_input: String::new(),
            is_secret_visible: false,
            is_admin_mode: false,
            is_recording: false,
            is_screen_locked: false,
            is_settings_open: false,
            is_files_open: false,
            is_terminal_open: false,
            current_path: initial_path,
            files_list: Vec::new(),
            terminal_input: String::new(),
            terminal_output: "NxDesk Terminal Remota Segura [VUL-01 Activa]\r\n".to_string(),
            terminal_history: Vec::new(),
            terminal_history_idx: None,
            allow_terminal: true,
            terminal_focus_requested: false,
            toast_message: None,
            selected_role: SessionRole::FullControl,
            allow_input: true,
            allow_clipboard: true,
            allow_files: true,
            remote_texture: None,
            last_rendered_frame_seq: 0,
            last_mouse_pos: (-1.0, -1.0),
            last_mouse_time: Instant::now(),
            session_start_time: None,
            recording_start_time: None,
            selected_screen_index: 0,
            is_view_fit: true,
            terminal_rx,
        }
    }

    fn send_select_screen(&mut self, index: i32) {
        self.selected_screen_index = index;
        self.state.active_screen_index.store(index, std::sync::atomic::Ordering::Relaxed);
        let name = match index {
            0 => "Pantalla 1".to_string(),
            1 => "Pantalla 2".to_string(),
            -1 => "Mosaico".to_string(),
            _ => format!("Pantalla {}", index + 1),
        };
        if let Ok(mut lock) = self.state.active_screen_name.try_write() {
            *lock = name;
        }
        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
            if let Some(sig) = sig_opt.as_ref() {
                let sig_clone = Arc::clone(sig);
                tokio::spawn(async move {
                    let msg = nxdesk_core::SignalingMessage::new("select_screen", index.to_string());
                    let _ = sig_clone.send_message(&msg).await;
                });
            }
        }
        self.remote_texture = None;
    }

    fn send_view_mode(&mut self, is_fit: bool) {
        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
            if let Some(sig) = sig_opt.as_ref() {
                let sig_clone = Arc::clone(sig);
                let mode = if is_fit {
                    let lw = self.state.local_width.load(std::sync::atomic::Ordering::Relaxed);
                    let lh = self.state.local_height.load(std::sync::atomic::Ordering::Relaxed);
                    let max_d = lw.max(lh);
                    if max_d > 0 {
                        max_d.to_string()
                    } else {
                        "fit".to_string()
                    }
                } else {
                    "native".to_string()
                };
                tokio::spawn(async move {
                    let msg = nxdesk_core::SignalingMessage::new("view_mode", &mode);
                    let _ = sig_clone.send_message(&msg).await;
                });
            }
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast_message = Some((msg.into(), Instant::now()));
    }

    fn copy_to_clipboard(&mut self, ui: &mut egui::Ui, text: &str, label: &str) {
        ui.ctx().copy_text(text.to_string());
        self.show_toast(format!("📋 {} copiado al portapapeles.", label));
    }

    //primer cambio de app.rs
    fn initiate_connect(&mut self, target: String) {
        let clean_target = target.trim().replace(' ', "");
        if clean_target.is_empty() {
            self.show_toast("Por favor ingrese un ID o IP válido.");
            return;
        }

        let state_clone = self.state.clone();
        let target_clone = clean_target.clone();
       
       
        tokio::spawn(async move {
            state_clone
                .set_active_remote_peer(Some(target_clone.clone()))
                .await;
            if let Some(sig) = state_clone.get_signaling_client().await {
                let _ = sig.join_room(&target_clone).await;
                // Emitir solicitud formal de sesión para que el host remoto despliegue el diálogo
                let (my_id, my_alias) = {
                    let cfg = state_clone.config.read().await;
                    (cfg.device_id.clone(), cfg.device_alias.clone())
                };
                let now_ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let req_dto = nxdesk_core::SessionRequestDto {
                    request_id: format!("req-{}-{}", my_id, now_ts),
                    client_id: my_id,
                    client_alias: my_alias,
                    target_id: Some(target_clone.clone()),
                    requested_role: nxdesk_core::SessionRole::FullControl,
                    allow_input: true,
                    allow_clipboard: true,
                    allow_file_transfer: true,
                    timestamp: now_ts,
                };
                if let Ok(json_str) = serde_json::to_string(&req_dto) {
                    let msg = nxdesk_core::SignalingMessage::new("session_request", &json_str)
                        .with_target(&target_clone);
                    let _ = sig.send_message(&msg).await;
                }
                // Solicitar catálogo de pantallas remotas al anfitrión
                let query_screens = nxdesk_core::SignalingMessage::new("query_screens", "")
                    .with_target(&target_clone);
                let _ = sig.send_message(&query_screens).await;

                // Notificar resolución y modo de visualización inicial (Fit adaptativo para 60 FPS estables)
                let lw = state_clone.local_width.load(std::sync::atomic::Ordering::Relaxed);
                let lh = state_clone.local_height.load(std::sync::atomic::Ordering::Relaxed);
                let max_d = lw.max(lh);
                let initial_mode = if max_d > 1366 { "1366".to_string() } else if max_d > 0 { max_d.to_string() } else { "fit".to_string() };
                let view_mode = nxdesk_core::SignalingMessage::new("view_mode", &initial_mode)
                    .with_target(&target_clone);
                let _ = sig.send_message(&view_mode).await;

                // Anunciar endpoint UDP para enlace P2P directo de ultra baja latencia
                if let Some(local_ep) = sig.p2p_local_endpoint() {
                    let ep_msg = nxdesk_core::SignalingMessage::new("udp_endpoint", &local_ep)
                        .with_target(&target_clone);
                    let _ = sig.send_message(&ep_msg).await;
                }
            }
        });

        self.session_start_time = Some(Instant::now());
        self.show_toast(format!("Conectando a {}...", clean_target));
    }
    //2do cambio
    fn reset_session_ui(&mut self) {
        // Si había una grabación en curso, cerrar el AVI antes de desconectar (si no,
        // quedaría abierto y sin índice para siempre).
        if self.is_recording {
            self.stop_recording();
        }
        self.remote_texture = None;
        self.last_rendered_frame_seq = 0;
        self.last_mouse_pos = (-1.0, -1.0);
        self.session_start_time = None;
        self.selected_screen_index = 0;
        self.is_files_open = false;
        self.is_terminal_open = false;
        self.is_recording = false;
        self.recording_start_time = None;
        self.is_screen_locked = false;
        self.terminal_output.clear();
        self.terminal_input.clear();
    }

    /// 🔴 Inicia la grabación de la sesión actual a un AVI MJPEG en Documentos\NxDesk Recordings.
    #[allow(dead_code)]
    fn start_recording(&mut self) {
        let dir = std::env::var("USERPROFILE")
            .map(|p| std::path::PathBuf::from(p).join("Documents").join("NxDesk Recordings"))
            .unwrap_or_else(|_| std::path::PathBuf::from("."));

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let out_path = dir.join(format!("NxDesk_Grabacion_{}.avi", stamp));

        let (rw, rh) = self.state.get_telemetry_snapshot().remote_resolution;
        let (lw, lh) = self.state.get_telemetry_snapshot().local_resolution;
        let (w, h) = if rw > 0 && rh > 0 {
            (rw, rh)
        } else if lw > 0 && lh > 0 {
            (lw, lh)
        } else {
            (1280, 720)
        };

        match crate::recorder::SessionRecorder::start_with_dimensions(out_path.clone(), w, h) {
            Ok(rec) => {
                // Intercambio atómico sin retener el lock en llamadas a métodos de self.
                let recorder_slot = self.state.session_recorder.clone();
                let previous = match recorder_slot.lock() {
                    Ok(mut g) => std::mem::replace(&mut *g, Some(rec)),
                    Err(_) => {
                        self.show_toast("⛔ No se pudo iniciar la grabación (estado bloqueado).");
                        return;
                    }
                };
                if let Some(old) = previous {
                    std::thread::Builder::new()
                        .name("nxdesk-rec-config".to_string())
                        .spawn(move || {
                            if let Ok(p) = old.finish() {
                                tracing::info!("[RecAnterior] Cerrado en {}", p.display());
                            }
                        })
                        .ok();
                }
                self.is_recording = true;
                self.recording_start_time = Some(Instant::now());
                self.show_toast(format!(
                    "🔴 Grabando sesión → {}",
                    out_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                self.show_toast(format!("⛔ Error al iniciar grabación: {}", e));
            }
        }
    }

    /// ⏹ Detiene la grabación y ensambla el AVI en segundo plano (no congela la UI).
    fn stop_recording(&mut self) {
        let recorder = match self.state.session_recorder.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        self.is_recording = false;
        self.recording_start_time = None;
        if let Some(rec) = recorder {
            self.show_toast("⏹ Grabación finalizada. Ensamblando video AVI...");
            let state = self.state.clone();
            std::thread::Builder::new()
                .name("nxdesk-rec-finalize".to_string())
                .spawn(move || {
                    let note = match rec.finish() {
                        Ok(path) => {
                            let mb = std::fs::metadata(&path)
                                .map(|m| m.len() / (1024 * 1024))
                                .unwrap_or(0)
                                .max(1);
                            format!("✅ Grabación guardada: {} ({} MB)", path.display(), mb)
                        }
                        Err(e) => {
                            let s = e.to_string();
                            if s.contains("ningún frame") {
                                "⚠️ La grabación se detuvo sin fotogramas capturados.".to_string()
                            } else {
                                format!("⛔ Error al guardar la grabación: {}", e)
                            }
                        }
                    };
                    if let Ok(mut n) = state.recording_note.lock() {
                        *n = Some(note);
                    }
                })
                .ok();
        }
    }

 fn disconnect_session(&mut self, ctx: &egui::Context) {   
    self.reset_session_ui();
    let state_clone = self.state.clone();
    tokio::spawn(async move {
        state_clone.clear_pending_session().await;
        let _ = state_clone.end_session(true).await; // Notifica session_end con target_id, resetea P2P y limpia estado
    });
    self.show_toast("Sesión finalizada.");
    ctx.request_repaint();
}

    /// Renderiza la barra superior (MainWindow.axaml Header)
    fn render_header(&mut self, ui: &mut egui::Ui, is_session_active: bool, active_peer: Option<String>) {
        egui::Frame::new()
            .fill(theme::BACKGROUND_SECONDARY)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Logo NxDesk idéntico a MainWindow.axaml: Text "Nx" en Bold Primary + "Desk" en Light
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Nx")
                                .size(20.0)
                                .strong()
                                .color(theme::PRIMARY),
                        );
                        ui.label(
                            RichText::new("Desk")
                                .size(20.0)
                                .color(theme::TEXT),
                        );
                    });

                    ui.add_space(20.0);

                    // Centro: Pre-sesión (Caja de conexión) vs En-Sesión (Floating Call Toolbar)
                    if !is_session_active {
                        // Pre-Session Controls: ID de Sala + SearchBox con botón arrow
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("ID de Sala:")
                                    .size(13.0)
                                    .color(theme::TEXT_SECONDARY),
                            );

                            let search_edit = ui.add_sized(
                                [280.0, 28.0],
                                egui::TextEdit::singleline(&mut self.target_id_input)
                                    .hint_text("Ingrese ID...")
                                    .margin(egui::Margin::symmetric(10, 4)),
                            );

                            let connect_btn = egui::Button::new(
                                RichText::new("➔")
                                    .size(14.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(theme::PRIMARY);

                            if ui.add_sized([32.0, 28.0], connect_btn).on_hover_text("Conectar").clicked()
                                || (search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                            {
                                let target = self.target_id_input.clone();
                                self.initiate_connect(target);
                            }
                        });
                    } else {
                        // In-Session Call Controls (Pill Bar como en MainWindow.axaml)
                        egui::Frame::new()
                            .fill(theme::BACKGROUND)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(16.0)
                            .inner_margin(egui::Margin::symmetric(12, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Control remoto toggle
                                    let ctrl_color = if self.allow_input { theme::PRIMARY } else { theme::TEXT_MUTED };
                                    if ui.button(RichText::new("🎮 Control").color(ctrl_color)).clicked() {
                                        self.allow_input = !self.allow_input;
                                        self.show_toast(if self.allow_input { "Control remoto activado." } else { "Control remoto pausado." });
                                    }

                                    // Transferencia de archivos
                                    if ui.button(RichText::new("📁 Archivos").color(theme::TEXT)).clicked() {
                                        self.is_files_open = !self.is_files_open;
                                    }

                                    // Terminal remota interactiva
                                    let term_color = if self.is_terminal_open { theme::PRIMARY } else { theme::TEXT };
                                    if ui.button(RichText::new("💻 Terminal").color(term_color)).on_hover_text("Terminal remota segura").clicked() {
                                        self.is_terminal_open = !self.is_terminal_open;
                                        if self.is_terminal_open {
                                            self.terminal_focus_requested = true;
                                        }
                                    }

                                    // Bloqueo de pantalla (compacto)
                                    let lock_color = if self.is_screen_locked { theme::PRIMARY } else { theme::TEXT_MUTED };
                                    if ui.button(RichText::new("🔒").color(lock_color)).on_hover_text("Bloquear pantalla remota").clicked() {
                                        self.is_screen_locked = !self.is_screen_locked;
                                        self.show_toast(if self.is_screen_locked { "Pantalla remota bloqueada." } else { "Pantalla remota desbloqueada." });
                                    }

                                    // Grabación de pantalla con temporizador compacto
                                    if self.is_recording {
                                        let elapsed = self.recording_start_time.map(|t| t.elapsed()).unwrap_or_default();
                                        let mins = elapsed.as_secs() / 60;
                                        let secs = elapsed.as_secs() % 60;
                                        let rec_label = format!("⏹ REC {:02}:{:02}", mins, secs);
                                        if ui.button(RichText::new(rec_label).strong().color(Color32::from_rgb(239, 68, 68))).on_hover_text("Detener y guardar video AVI").clicked() {
                                            self.stop_recording();
                                        }
                                        ui.ctx().request_repaint_after(Duration::from_millis(500));
                                    } else {
                                        let rec_btn = egui::Button::new(RichText::new("🔴 Grabar").color(theme::TEXT_MUTED));
                                        if ui.add(rec_btn).on_hover_text("Grabación pausada temporalmente para optimizar latencia (<120ms)").clicked() {
                                            self.show_toast("ℹ️ Grabación desactivada temporalmente para lograr latencia ultrabaja.");
                                        }
                                    }

                                    ui.separator();

                                    // Selector de Pantallas / Multimonitor dinámico y ordenado
                                    let remote_screens = self
                                        .state
                                        .remote_screens
                                        .try_read()
                                        .map(|s| s.clone())
                                        .unwrap_or_default();
                                    let active_idx = self
                                        .state
                                        .active_screen_index
                                        .load(std::sync::atomic::Ordering::Relaxed);
                                    let active_name = self
                                        .state
                                        .active_screen_name
                                        .try_read()
                                        .map(|n| n.clone())
                                        .unwrap_or_else(|_| "Pantalla 1".to_string());

                                    let scr_label = format!("🖥️ {} ▾", active_name);
                                    ui.menu_button(RichText::new(scr_label).strong().color(theme::PRIMARY), |ui| {
                                        if remote_screens.is_empty() {
                                            if ui.selectable_label(active_idx == 0, "🖥️ Pantalla 1 (Principal)").clicked() {
                                                self.send_select_screen(0);
                                                self.show_toast("Conmutando a Pantalla 1...");
                                                ui.close();
                                            }
                                            if ui.selectable_label(active_idx == 1, "🖥️ Pantalla 2 (Secundaria)").clicked() {
                                                self.send_select_screen(1);
                                                self.show_toast("Conmutando a Pantalla 2...");
                                                ui.close();
                                            }
                                            if ui.selectable_label(active_idx == -1, "🖥️ Todas las Pantallas (Mosaico)").clicked() {
                                                self.send_select_screen(-1);
                                                self.show_toast("Conmutando a Mosaico completo...");
                                                ui.close();
                                            }
                                        } else {
                                            for screen in &remote_screens {
                                                let is_curr = screen.index == active_idx;
                                                let prefix = if is_curr { "✓ " } else { "  " };
                                                let opt_text = format!("{}{} ({}x{})", prefix, screen.name, screen.width, screen.height);
                                                if ui.selectable_label(is_curr, opt_text).clicked() {
                                                    self.send_select_screen(screen.index);
                                                    self.show_toast(format!("Cambiando a {}...", screen.name));
                                                    ui.close();
                                                }
                                            }
                                            if ui.selectable_label(active_idx == -1, "  🖥️ Todas las Pantallas (Mosaico)").clicked() {
                                                self.send_select_screen(-1);
                                                self.show_toast("Conmutando a Mosaico completo...");
                                                ui.close();
                                            }
                                        }
                                    });

                                    ui.separator();

                                    // Selector de Modo de Visualización (1:1 Nativo vs Ajustar a Ventana)
                                    let (fit_label, fit_tooltip) = if self.is_view_fit {
                                        ("🔍 1:1", "Conmutar a Modo 1:1 Original (Pixel-Perfect sin borrosidad, con desplazamiento)")
                                    } else {
                                        ("⛶ Ajustar", "Conmutar a Modo Ajustar a Ventana (Escala automática)")
                                    };
                                    let fit_color = if self.is_view_fit { theme::TEXT } else { theme::PRIMARY };
                                    if ui.button(RichText::new(fit_label).color(fit_color)).on_hover_text(fit_tooltip).clicked() {
                                        self.is_view_fit = !self.is_view_fit;
                                        self.remote_texture = None;
                                        self.send_view_mode(self.is_view_fit);
                                        self.show_toast(if self.is_view_fit {
                                            "Visualización: Ajustada a ventana (60 FPS fluidos)"
                                        } else {
                                            "Visualización: 1:1 Pixel-Perfect Original"
                                        });
                                    }

                                    ui.separator();

                                    // Botón rojo desconectar idéntico a Avalonia - SIEMPRE VISIBLE Y ACCESIBLE
                                    let dc_btn = egui::Button::new(
                                        RichText::new("🚪 Desconectar")
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(theme::ERROR);

                                    if ui.add(dc_btn).on_hover_text("Finalizar sesión").clicked() {
                                        self.disconnect_session(ui.ctx());
                                    }
                                });
                            });
                    }

                    // Controles a la derecha (MainWindow.axaml Right Controls)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Botón de Configuración
                        if ui.button(RichText::new("⚙").size(16.0)).on_hover_text("Configuración").clicked() {
                            self.is_settings_open = !self.is_settings_open;
                        }

                        // Modo Administrador Toggle (Escudo)
                        let admin_color = if self.is_admin_mode { theme::PRIMARY } else { theme::TEXT_MUTED };
                        if ui.button(RichText::new("🛡 Admin").color(admin_color)).on_hover_text("Acceso Administrador").clicked() {
                            self.is_admin_mode = !self.is_admin_mode;
                            if self.is_admin_mode {
                                self.is_terminal_open = true;
                                self.terminal_focus_requested = true;
                            }
                        }

                        // Indicador de estado de red
                        let is_sig = self.state.is_signaling_connected();
                        let dot_color = if is_sig { theme::SUCCESS } else { Color32::from_rgb(245, 158, 11) };
                        let status_text = if is_sig { "En línea" } else { "Red Local (LAN)" };

                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 5.0, dot_color);
                            ui.label(RichText::new(status_text).size(12.0).color(theme::TEXT_SECONDARY));
                        });

                        if let Some(peer) = active_peer {
                            let telem = self.state.get_telemetry_snapshot();
                            let ping_str = if telem.rtt_ms > 0 {
                                format!("{} ms", telem.rtt_ms)
                            } else {
                                "<5 ms".to_string()
                            };
                            let stats_pill = format!("{} FPS • {}", telem.rx_fps, ping_str);
                            ui.label(RichText::new(stats_pill).size(12.0).strong().color(Color32::from_rgb(34, 197, 94)));
                            ui.label(RichText::new(format!("Conectado a: {}", peer)).strong().color(theme::PRIMARY));
                        }
                    });
                });
            });
    }

    /// Renderiza la vista principal idéntica a HomeView.axaml de C#
    fn render_home_view(&mut self, ui: &mut egui::Ui) {
        let (my_id, my_alias, host_password) = {
            if let Ok(cfg) = self.state.config.try_read() {
                (cfg.device_id.clone(), cfg.device_alias.clone(), cfg.access_password.clone())
            } else {
                ("111-222-333".to_string(), "NxDesk-Host".to_string(), None)
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(10.0);

            // ==========================================
            // SECCIÓN 1: "Este puesto de trabajo" (HomeView.axaml)
            // ==========================================
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::symmetric(20, 24))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Este puesto de trabajo")
                                .size(15.0)
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 2.0 - 120.0);
                            // ID AnyDesk en grande y color Primary (#007ACC)
                            ui.label(
                                RichText::new(&my_id)
                                    .size(28.0)
                                    .strong()
                                    .color(theme::PRIMARY),
                            );

                            ui.add_space(10.0);

                            // Botón copiar ID
                            let copy_btn = egui::Button::new(RichText::new("📋 Copiar").size(12.0).color(Color32::WHITE))
                                .fill(theme::SURFACE);
                            if ui.add(copy_btn).on_hover_text("Copiar ID al portapapeles").clicked() {
                                self.copy_to_clipboard(ui, &my_id, "ID del puesto");
                            }
                        });

                        ui.add_space(10.0);

                        // Fila de credenciales del equipo y clave anfitrión
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 2.0 - 150.0);
                            ui.label(RichText::new(format!("Equipo: {}", my_alias)).size(13.0).color(theme::TEXT_MUTED));
                            ui.label("•");

                            let pw_text = if self.is_secret_visible {
                                host_password.as_deref().unwrap_or("Sin clave")
                            } else {
                                "••••••••"
                            };
                            ui.label(RichText::new(format!("Clave: {}", pw_text)).size(13.0).strong().color(Color32::from_rgb(245, 158, 11)));

                            let toggle_text = if self.is_secret_visible { "🙈 Ocultar" } else { "👁 Ver" };
                            if ui.small_button(toggle_text).clicked() {
                                self.is_secret_visible = !self.is_secret_visible;
                            }
                        });
                    });
                });

            ui.add_space(16.0);

            // ==========================================
            // SECCIÓN 2: "Descubierto" (DeviceCard WrapGrid de HomeView.axaml)
            // ==========================================
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new("Descubierto")
                        .size(16.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new("(Equipos detectados automáticamente en red local UDP y WAN)")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
            });

            ui.add_space(12.0);

            let peers: Vec<PeerIdentity> = {
                if let Ok(p) = self.state.discovered_peers.try_read() {
                    p.values().cloned().collect()
                } else {
                    Vec::new()
                }
            };

            if peers.is_empty() {
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(10.0)
                    .inner_margin(24.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Buscando equipos en la red local o WAN...").color(theme::TEXT_MUTED));
                            ui.label(RichText::new("Asegúrese de que otros equipos tengan NxDesk abierto.").size(12.0).color(theme::TEXT_MUTED));
                        });
                    });
            } else {
                ui.horizontal_wrapped(|ui| {
                    for peer in peers {
                        // DeviceCard: idéntico al estilo DeviceCard de HomeView.axaml
                        egui::Frame::new()
                            .fill(theme::SURFACE)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(12.0)
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.set_width(200.0);
                                ui.set_height(115.0);

                                ui.vertical(|ui| {
                                    // Fila superior: Punto verde de estado (#00CC66) y botón de conexión rápida
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                                        ui.painter().circle_filled(rect.center(), 5.0, theme::SUCCESS);

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let conn_btn = egui::Button::new(
                                                RichText::new("🔒 Conectar").size(11.0).strong().color(Color32::WHITE),
                                            )
                                            .fill(theme::PRIMARY);

                                            if ui.add(conn_btn).on_hover_text("Conectar a este puesto de trabajo").clicked() {
                                                self.initiate_connect(peer.id.clone());
                                            }
                                        });
                                    });

                                    ui.add_space(8.0);

                                    // Información del equipo
                                    ui.label(
                                        RichText::new(&peer.alias)
                                            .size(15.0)
                                            .strong()
                                            .color(theme::TEXT),
                                    );
                                    ui.label(
                                        RichText::new(&peer.id)
                                            .size(13.0)
                                            .color(theme::TEXT_SECONDARY),
                                    );
                                    ui.label(
                                        RichText::new(format!("{}:{}", peer.ip_address, peer.port))
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED),
                                    );
                                });
                            });

                        ui.add_space(10.0);
                    }
                });
            }
        });
    }

    fn send_remote_input(&self, event: InputEvent) {
        // Botón 🎮 Control / checkbox de la configuración: si el control está en pausa,
        // NINGÚN evento de ratón o teclado sale hacia el anfitrión remoto.
        if !self.allow_input {
            return;
        }
        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
            if let Some(sig) = sig_opt.as_ref() {
                let _ = sig.send_input_direct(&event);
            }
        }
    }

    fn send_clipboard_sync(&self, text: &str) {
        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
            if let Some(sig) = sig_opt.as_ref() {
                let sig_clone = Arc::clone(sig);
                let msg = nxdesk_core::SignalingMessage::new("clipboard", text);
                tokio::spawn(async move {
                    let _ = sig_clone.send_message(&msg).await;
                });
            }
        }
    }

    /// Renderiza la pantalla remota (Paridad 1:1 con RemoteView de C# con interactividad total de entrada)
    fn render_remote_view(&mut self, ui: &mut egui::Ui, peer_id: &str) {
        // 1. Optimización VRAM/CPU: Actualizar textura inmediatamente desde el ColorImage pre-decodificado en background
        let current_seq = self.state.frame_sequence.load(Ordering::Relaxed);
        let needs_update = current_seq != self.last_rendered_frame_seq || self.remote_texture.is_none();
        if needs_update {
            if let Ok(guard) = self.state.latest_decoded_image.try_read() {
                if let Some(ref arc_img) = *guard {
                    // Sonda de RENDER: costo de subida a textura GPU (TeX set/load) +
                    // edad del frame al pintarse (decode→primer frame que lo contiene)
                    let t_upload = Instant::now();
                    let painted_age_ms = {
                        let now_seen = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        let decoded_at = self.state.last_decoded_unix_ms.load(Ordering::Relaxed);
                        now_seen
                            .saturating_sub(decoded_at)
                            .min(10_000)
                            as u32
                    };
                    self.last_rendered_frame_seq = current_seq;
                    let size = arc_img.size;
                    let image_data = egui::ImageData::Color(std::sync::Arc::clone(arc_img));
                    let tex_options = if self.is_view_fit {
                        egui::TextureOptions::LINEAR
                    } else {
                        egui::TextureOptions::NEAREST
                    };
                    if let Some(ref mut tex) = self.remote_texture {
                        if tex.size() == size {
                            tex.set(image_data, tex_options);
                        } else {
                            *tex = ui.ctx().load_texture(
                                "remote_screen_texture",
                                image_data,
                                tex_options,
                            );
                        }
                    } else {
                        self.remote_texture = Some(ui.ctx().load_texture(
                            "remote_screen_texture",
                            image_data,
                            tex_options,
                        ));
                    }
                    self.state.record_render_timing(
                        t_upload.elapsed().as_micros() as u32,
                        painted_age_ms,
                    );
                }
            }


        }

        if let Some(ref tex) = self.remote_texture {
            let tex_size = tex.size_vec2();
            let tex_aspect = tex_size.x / tex_size.y.max(1.0);
            let avail = ui.available_size();
            let avail_aspect = avail.x / avail.y.max(1.0);

            let (rect, interact_resp) = if self.is_view_fit {
                // Encaje proporcional (letterbox) SIEMPRE: la imagen ocupa el máximo dentro del
                // panel respetando 1:1 con la pantalla remota y queda CENTRADA
                let target_size = if avail_aspect > tex_aspect {
                    egui::vec2((avail.y * tex_aspect).floor(), avail.y)
                } else {
                    egui::vec2(avail.x, (avail.x / tex_aspect).floor())
                };
                let (full_rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
                ui.painter().rect_filled(full_rect, 0.0, theme::BACKGROUND_DARK);

                let r = egui::Rect::from_center_size(full_rect.center(), target_size);
                ui.painter().image(
                    tex.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                let resp = ui.interact(r, ui.auto_id_with("remote_screen_interact"), egui::Sense::click_and_drag());
                (r, resp)
            } else {
                // Modo 1:1 Pixel-Perfect Nativo: renderizado a tamaño real con ScrollArea bidireccional
                let mut inner_r = egui::Rect::NOTHING;
                let mut inner_resp = None;
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let (r, resp) = ui.allocate_exact_size(tex_size, egui::Sense::click_and_drag());
                        ui.painter().image(
                            tex.id(),
                            r,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        inner_r = r;
                        inner_resp = Some(resp);
                    });
                (inner_r, inner_resp.unwrap_or_else(|| ui.interact(egui::Rect::NOTHING, ui.auto_id_with("remote_screen_interact_empty"), egui::Sense::hover())))
            };

            // 2. Captura y despacho de eventos de ratón con coordenadas normalizadas (0.0 - 1.0)
            let is_pointer_active = interact_resp.hovered() || interact_resp.dragged();
            let pointer_pos = interact_resp
                .hover_pos()
                .or_else(|| ui.input(|i| i.pointer.interact_pos()))
                .or_else(|| ui.input(|i| i.pointer.latest_pos()));

            if is_pointer_active {
                if let Some(hover_pos) = pointer_pos {
                    let norm_x = ((hover_pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let norm_y = ((hover_pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);

                    // Cadencia fluida a 60 Hz (~16 ms): seguimiento suave sin saturar el canal de control
                    let now = Instant::now();
                    if now.duration_since(self.last_mouse_time) >= Duration::from_millis(16)
                        && ((norm_x - self.last_mouse_pos.0).abs() > 0.0008
                            || (norm_y - self.last_mouse_pos.1).abs() > 0.0008)
                    {
                        self.last_mouse_pos = (norm_x, norm_y);
                        self.last_mouse_time = now;
                        self.send_remote_input(InputEvent::MouseMove {
                            x: norm_x,
                            y: norm_y,
                        });
                    }

                    // Botón izquierdo
                    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                        interact_resp.request_focus();
                        self.send_remote_input(InputEvent::MouseDown {
                            button: MouseButton::Left,
                            x: norm_x,
                            y: norm_y,
                        });
                    }
                    if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                        self.send_remote_input(InputEvent::MouseUp {
                            button: MouseButton::Left,
                            x: norm_x,
                            y: norm_y,
                        });
                    }

                    // Botón derecho
                    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
                        self.send_remote_input(InputEvent::MouseDown {
                            button: MouseButton::Right,
                            x: norm_x,
                            y: norm_y,
                        });
                    }
                    if ui.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)) {
                        self.send_remote_input(InputEvent::MouseUp {
                            button: MouseButton::Right,
                            x: norm_x,
                            y: norm_y,
                        });
                    }

                    // Botón central
                    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)) {
                        self.send_remote_input(InputEvent::MouseDown {
                            button: MouseButton::Middle,
                            x: norm_x,
                            y: norm_y,
                        });
                    }
                    if ui.input(|i| i.pointer.button_released(egui::PointerButton::Middle)) {
                        self.send_remote_input(InputEvent::MouseUp {
                            button: MouseButton::Middle,
                            x: norm_x,
                            y: norm_y,
                        });
                    }

                    // Rueda de desplazamiento (Scroll)
                    let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
                    if scroll_delta.x.abs() > 0.1 || scroll_delta.y.abs() > 0.1 {
                        self.send_remote_input(InputEvent::MouseWheel {
                            delta_x: scroll_delta.x,
                            delta_y: scroll_delta.y,
                        });
                    }
                }
            }

            // 3. Captura y despacho de eventos de teclado y portapapeles hacia la PC remota
            // Activo siempre que la terminal integrada de la app no esté abierta
            if (interact_resp.hovered() || interact_resp.has_focus()) && !self.is_terminal_open {
                // Tecla Windows (Super): Garantizar ciclo completo KeyDown + KeyUp para abrir Menú Inicio en el Host
                if ui.input(|i| i.key_pressed(egui::Key::SuperLeft) || i.key_pressed(egui::Key::SuperRight)) {
                    self.send_remote_input(InputEvent::KeyDown {
                        key_code: 0x5B, // VK_LWIN
                        key_char: None,
                    });
                    self.send_remote_input(InputEvent::KeyUp { key_code: 0x5B });
                }

                let events = ui.input(|i| i.events.clone());
                for event in events {
                    match event {
                        egui::Event::Key {
                            key,
                            pressed,
                            repeat: _,
                            ..
                        } => {
                            let vk = egui_key_to_vk(key);
                            if vk != 0 {
                                if pressed {
                                    self.send_remote_input(InputEvent::KeyDown {
                                        key_code: vk,
                                        key_char: None,
                                    });
                                } else {
                                    self.send_remote_input(InputEvent::KeyUp { key_code: vk });
                                }
                            }
                        }
                        egui::Event::Copy => {
                            // Usuario pulsó Ctrl+C en la pantalla remota (interrumpir o copiar en PC remota)
                            self.send_remote_input(InputEvent::KeyDown {
                                key_code: 0x43, // VK_C
                                key_char: None,
                            });
                            self.send_remote_input(InputEvent::KeyUp { key_code: 0x43 });
                        }
                        egui::Event::Cut => {
                            // Usuario pulsó Ctrl+X en la pantalla remota
                            self.send_remote_input(InputEvent::KeyDown {
                                key_code: 0x58, // VK_X
                                key_char: None,
                            });
                            self.send_remote_input(InputEvent::KeyUp { key_code: 0x58 });
                        }
                        egui::Event::Paste(text) => {
                            if self.allow_clipboard {
                                self.send_clipboard_sync(&text);
                            }
                            // Inyectar combinación física Ctrl+V en el host remoto
                            self.send_remote_input(InputEvent::KeyDown {
                                key_code: 0x56, // VK_V
                                key_char: None,
                            });
                            self.send_remote_input(InputEvent::KeyUp { key_code: 0x56 });
                        }
                        egui::Event::Text(ref text) => {
                            for ch in text.chars() {
                                // No duplicar caracteres básicos ASCII que ya viajan por egui::Event::Key
                                // pero permitir símbolos internacionales o compuestos: ñ, ¿, ¡, @, #, $, %, &, á, etc.
                                if !ch.is_control() && !ch.is_ascii_alphanumeric() && ch != ' ' {
                                    let is_direct_oem_key = matches!(
                                        ch,
                                        '.' | ',' | '-' | '+' | '=' | ';' | ':' | '/' | '?' | '\\' | '|' | '`' | '[' | '{' | ']' | '}' | '\'' | '"'
                                    );
                                    if !is_direct_oem_key {
                                        self.send_remote_input(InputEvent::KeyDown {
                                            key_code: 0,
                                            key_char: Some(ch.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // 4. Floating Live Telemetry HUD Overlay (FPS, RTT ms, Bitrate, Resolution)
            let telem = self.state.get_telemetry_snapshot();
            let ping_str = if telem.rtt_ms > 0 {
                format!("{} ms", telem.rtt_ms)
            } else {
                "<5 ms".to_string()
            };
            let _bitrate_str = if telem.rx_kbps >= 1000 {
                format!("{:.1} Mbps", telem.rx_kbps as f32 / 1000.0)
            } else {
                format!("{} kbps", telem.rx_kbps)
            };
            let (rw, rh) = telem.remote_resolution;
            let res_str = if rw > 0 && rh > 0 {
                format!("{}x{}", rw, rh)
            } else {
                "720p".to_string()
            };

            let hud_width = 540.0;
            let hud_height = 28.0;
            let clip_rect = ui.clip_rect();
            let hud_rect = egui::Rect::from_min_size(
                egui::pos2(clip_rect.max.x - hud_width - 16.0, clip_rect.min.y + 12.0),
                egui::vec2(hud_width, hud_height),
            );

            // Fondo translúcido tipo glassmorphism oscuro
            ui.painter().rect(
                hud_rect,
                14.0,
                Color32::from_rgba_premultiplied(15, 23, 42, 220),
                Stroke::new(1.0, Color32::from_rgba_premultiplied(51, 65, 85, 200)),
                egui::StrokeKind::Inside,
            );

            let dot_color = if telem.rtt_ms <= 60 {
                Color32::from_rgb(34, 197, 94) // Verde suave (<60 ms)
            } else if telem.rtt_ms <= 120 {
                Color32::from_rgb(34, 197, 94) // Verde (<120 ms)
            } else if telem.rtt_ms <= 180 {
                Color32::from_rgb(234, 179, 8) // Amarillo
            } else {
                Color32::from_rgb(239, 68, 68) // Rojo
            };

            // Indicador de conexión en tiempo real
            ui.painter().circle_filled(
                egui::pos2(hud_rect.min.x + 16.0, hud_rect.center().y),
                4.5,
                dot_color,
            );

            let stages_str = if telem.net_ms > 0.0 {
                format!("Cap:{:.1}m|Enc:{:.1}m|Net:{:.1}m|Dec:{:.1}m", telem.cap_ms, telem.enc_ms, telem.net_ms, telem.dec_ms)
            } else {
                format!("Cap:{:.1}m|Enc:{:.1}m|Dec:{:.1}m", telem.cap_ms, telem.enc_ms, telem.dec_ms)
            };
            let mode_str = if self.is_view_fit { "Ajustar" } else { "1:1 Nativo" };
            let mem_str = format!("{:.0} MB RAM", telem.ram_mb);
            let hud_text = format!(
                "{} FPS  •  {}  •  {}  •  {}  •  {} [{}]",
                telem.rx_fps, ping_str, mem_str, stages_str, res_str, mode_str
            );
            ui.painter().text(
                egui::pos2(hud_rect.min.x + 28.0, hud_rect.center().y),
                egui::Align2::LEFT_CENTER,
                hud_text,
                egui::FontId::proportional(11.5),
                Color32::from_rgb(241, 245, 249),
            );
        } else {
            egui::Frame::new()
                .fill(theme::BACKGROUND_DARK)
                .inner_margin(40.0)
                .show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(format!("Conectando con {}", peer_id))
                                    .size(18.0)
                                    .strong()
                                    .color(theme::PRIMARY),
                            );
                            ui.add_space(12.0);
                            let occupancy = self.state.get_room_occupancy();
                            if occupancy <= 1 {
                                ui.label(
                                    RichText::new("⏳ Esperando que el equipo remoto inicie NxDesk o se una a la sala...")
                                        .size(13.0)
                                        .color(Color32::from_rgb(255, 170, 0)),
                                );
                            } else {
                                ui.label(
                                    RichText::new("🔔 Solicitud enviada al equipo remoto. Esperando que el usuario pulse 'Aceptar'...")
                                        .size(13.0)
                                        .color(theme::SUCCESS),
                                );
                            }
                            ui.add_space(16.0);
                            if ui.add(egui::Button::new(RichText::new("❌ Cancelar").color(Color32::WHITE)).fill(theme::ERROR)).clicked() {
                                self.disconnect_session(ui.ctx());
                            }
                        });
                    });
                });
        }
    }

    /// Modal de Configuración (Settings)
    fn render_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.is_settings_open {
            return;
        }

        egui::Window::new("⚙ Configuración de NxDesk")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(380.0);

                ui.label(RichText::new("Parámetros del Puesto de Trabajo").strong().color(theme::TEXT));
                ui.add_space(8.0);

                let is_sharing = self.state.is_screen_sharing.load(Ordering::SeqCst);
                ui.label(format!("Captura de Pantalla: {}", if is_sharing { "Activa" } else { "Inactiva" }));

                ui.add_space(8.0);
                if ui.button("🖥 Conmutar Monitor Primario / Secundario").clicked() {
                    let state_clone = self.state.clone();
                    tokio::spawn(async move {
                        let _ = state_clone.select_screen(0).await;
                    });
                    self.show_toast("Monitor conmutado.");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                if ui.button("Cerrar").clicked() {
                    self.is_settings_open = false;
                }
            });
    }

    /// Modal de Archivos Remotos
    fn render_files_dialog(&mut self, ctx: &egui::Context) {
        if !self.is_files_open {
            return;
        }

        egui::Window::new("📁 Transferencia de Archivos [VUL-02 Confinada]")
            .collapsible(false)
            .default_size([560.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Ruta: {}", self.current_path)).size(12.0).color(theme::TEXT_SECONDARY));
                    if ui.button("🔄 Refrescar").clicked() {
                        let resp = nxdesk_file_transfer::list_directory(&self.current_path);
                        self.files_list = resp.items;
                    }
                });

                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in &self.files_list {
                        let icon = if item.is_directory { "📁" } else { "📄" };
                        ui.label(format!("{} {} ({} KB)", icon, item.name, item.size / 1024));
                    }
                });

                ui.add_space(8.0);
                if ui.button("Cerrar").clicked() {
                    self.is_files_open = false;
                }
            });
    }

    /// Terminal Remota Segura (bidireccional): envía comandos al par por la red y muestra
    /// la salida local y la del par remoto (canal "terminal_out").
    fn render_terminal_dialog(&mut self, ctx: &egui::Context) {
        if !self.is_terminal_open {
            return;
        }

        // Bombear la salida (local y del par) al buffer visible, con límite de scroll-back
        // para que la UI nunca degenere en memoria tras sesiones largas.
        while let Ok(chunk) = self.terminal_rx.try_recv() {
            self.terminal_output.push_str(&chunk);
        }
        if self.terminal_output.len() > 128_000 {
            let cut = self.terminal_output.len() - 96_000;
            let safe_cut = self.terminal_output.floor_char_boundary(cut);
            let boundary = self.terminal_output[safe_cut..]
                .find('\n')
                .map(|p| safe_cut + p + 1)
                .unwrap_or(safe_cut);
            self.terminal_output.drain(..boundary);
        }

        let target_peer = {
            if let Ok(r) = self.state.active_remote_peer.try_read() {
                r.clone()
            } else {
                None
            }
        }
        .or_else(|| {
            if let Ok(s) = self.state.active_session.try_read() {
                s.as_ref().map(|x| x.peer_id.clone())
            } else {
                None
            }
        });

        egui::Window::new("💻 Terminal Remota Segura [VUL-01 Verificada]")
            .collapsible(false)
            .resizable(true)
            .default_size([720.0, 480.0])
            .min_size([450.0, 320.0])
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                // Salida con scroll vertical/horizontal adaptativo y wrap de texto:
                // nunca desborda la caja ni se sale de los márgenes de la ventana.
                let avail_height = (ui.available_height() - 56.0).max(180.0);
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(avail_height)
                    .min_scrolled_height(avail_height)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&self.terminal_output)
                                    .monospace()
                                    .color(theme::TEXT)
                                    .size(12.0),
                            )
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Wrap),
                        );
                    });

                ui.separator();

                ui.horizontal(|ui| {
                    // Entrada de comandos: altura fija, horizontalmente contenido
                    let cmd_edit = ui.add(
                        egui::TextEdit::singleline(&mut self.terminal_input)
                            .hint_text("Comando... (Enter para enviar al equipo remoto)")
                            .desired_width(ui.available_width() - 260.0)
                            .min_size(egui::Vec2::new(0.0, 24.0))
                            .clip_text(true),
                    );

                    if self.terminal_focus_requested {
                        cmd_edit.request_focus();
                        self.terminal_focus_requested = false;
                    }

                    if cmd_edit.has_focus() {
                        // Navegación de historial con Flechas Arriba/Abajo
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && !self.terminal_history.is_empty() {
                            let new_idx = match self.terminal_history_idx {
                                Some(idx) => if idx > 0 { idx - 1 } else { 0 },
                                None => self.terminal_history.len() - 1,
                            };
                            self.terminal_history_idx = Some(new_idx);
                            if let Some(cmd) = self.terminal_history.get(new_idx) {
                                self.terminal_input = cmd.clone();
                            }
                        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            if let Some(idx) = self.terminal_history_idx {
                                if idx + 1 < self.terminal_history.len() {
                                    let new_idx = idx + 1;
                                    self.terminal_history_idx = Some(new_idx);
                                    if let Some(cmd) = self.terminal_history.get(new_idx) {
                                        self.terminal_input = cmd.clone();
                                    }
                                } else {
                                    self.terminal_history_idx = None;
                                    self.terminal_input.clear();
                                }
                            }
                        }

                        // Escape: Limpiar entrada actual
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.terminal_input.clear();
                            self.terminal_history_idx = None;
                        }
                    }

                    let send_clicked = ui.button("Enviar ➔").clicked();
                    let enter_pressed = (cmd_edit.has_focus() || cmd_edit.lost_focus())
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if (send_clicked || enter_pressed) && !self.terminal_input.trim().is_empty() {
                        let cmd = self.terminal_input.trim().to_string();
                        if self.terminal_history.last() != Some(&cmd) {
                            self.terminal_history.push(cmd.clone());
                            if self.terminal_history.len() > 100 {
                                self.terminal_history.remove(0);
                            }
                        }
                        self.terminal_history_idx = None;

                        // Echo local + envío real por la red al par autorizado
                        self.terminal_output.push_str(&format!("> {}\r\n", cmd));
                        self.terminal_input.clear();
                        cmd_edit.request_focus();

                        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
                            if let Some(sig) = sig_opt.as_ref() {
                                let sig = std::sync::Arc::clone(sig);
                                let mut msg = nxdesk_core::SignalingMessage::new("terminal_cmd", &cmd);
                                if let Some(ref target) = target_peer {
                                    msg = msg.with_target(target);
                                }
                                tokio::spawn(async move {
                                    let _ = sig.send_message(&msg).await;
                                });
                            }
                        }
                    }

                    // Interrupción inmediata: Botón [⏹ Ctrl+C] o Atajo físico Ctrl+C
                    let ctrl_c_clicked = ui.button("⏹ Ctrl+C").clicked();
                    let ctrl_c_key = cmd_edit.has_focus()
                        && ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::C));

                    if ctrl_c_clicked || ctrl_c_key {
                        self.terminal_input.clear();
                        self.terminal_history_idx = None;
                        self.terminal_output.push_str("^C\r\n[Enviando interrupción Ctrl+C...]\r\n");
                        cmd_edit.request_focus();

                        if let Ok(sig_opt) = self.state.signaling_client.try_read() {
                            if let Some(sig) = sig_opt.as_ref() {
                                let sig = std::sync::Arc::clone(sig);
                                let mut msg = nxdesk_core::SignalingMessage::new("terminal_ctrl_c", "");
                                if let Some(ref target) = target_peer {
                                    msg = msg.with_target(target);
                                }
                                tokio::spawn(async move {
                                    let _ = sig.send_message(&msg).await;
                                });
                            }
                        }
                    }

                    // Limpiar pantalla: Botón [🗑 Limpiar] o Atajo físico Ctrl+L
                    let clear_clicked = ui.button("🗑 Limpiar").clicked();
                    let ctrl_l_key = cmd_edit.has_focus()
                        && ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::L));

                    if clear_clicked || ctrl_l_key {
                        self.terminal_output.clear();
                        self.terminal_output.push_str("NxDesk Terminal Remota Segura [VUL-01 Activa]\r\n");
                    }
                });

                ui.add_space(4.0);
                if ui.button("Cerrar").clicked() {
                    self.is_terminal_open = false;
                }
            });
    }

    /// Cuadro de diálogo modal nativo AnyDesk para Petición/Aceptación de Sesión (ConnectionTypeDialog.axaml)
    fn check_and_render_incoming_session_dialog(&mut self, ctx: &egui::Context) {
        let pending = {
            if let Ok(p) = self.state.pending_session.try_read() {
                p.clone()
            } else {
                None
            }
        };

        if let Some(req) = pending {
            // Solicitar atención urgente en la barra de tareas de Windows y enfocar ventana
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Critical,
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

            let mut accept = false;
            let mut reject = false;

            egui::Window::new("🔔 Solicitud de Conexión Entrante")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(420.0);

                    // Banner de alerta visible y destacado
                    egui::Frame::new()
                        .fill(Color32::from_rgb(45, 30, 10))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(255, 170, 0)))
                        .inner_margin(8.0)
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("⚠️").size(18.0));
                                ui.label(
                                    RichText::new("¡PETICIÓN DE CONTROL REMOTO ENTRANTE!")
                                        .strong()
                                        .color(Color32::from_rgb(255, 200, 50)),
                                );
                            });
                        });
                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("Un usuario remoto solicita conectarse a este equipo:")
                            .size(13.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.add_space(8.0);

                    egui::Frame::new()
                        .fill(theme::BACKGROUND_SECONDARY)
                        .inner_margin(12.0)
                        .corner_radius(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("👤").size(24.0));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&req.client_alias).size(15.0).strong().color(theme::TEXT));
                                    ui.label(RichText::new(format!("ID: {}", req.client_id)).size(12.0).color(theme::TEXT_MUTED));
                                });
                            });
                        });

                    ui.add_space(12.0);
                    ui.label(RichText::new("Tipo de Conexión:").strong().color(theme::TEXT));
                    ui.radio_value(&mut self.selected_role, SessionRole::FullControl, "Control Total (Interactivo)");
                    ui.radio_value(&mut self.selected_role, SessionRole::ViewOnly, "Solo Ver (Visualización de Pantalla)");

                    ui.add_space(8.0);
                    ui.checkbox(&mut self.allow_input, "Permitir entrada de ratón y teclado");
                    ui.checkbox(&mut self.allow_clipboard, "Permitir portapapeles bidireccional");
                    ui.checkbox(&mut self.allow_files, "Permitir transferencia de archivos");
                    ui.checkbox(&mut self.allow_terminal, "Permitir terminal remota interactiva [VUL-01]");

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let reject_btn = egui::Button::new(
                            RichText::new("❌ Rechazar").strong().color(Color32::WHITE),
                        )
                        .fill(theme::ERROR);

                        if ui.add_sized([120.0, 32.0], reject_btn).clicked() {
                            reject = true;
                        }

                        ui.add_space(16.0);

                        let accept_btn = egui::Button::new(
                            RichText::new("✔ Aceptar").strong().color(Color32::WHITE),
                        )
                        .fill(theme::PRIMARY);

                        if ui.add_sized([120.0, 32.0], accept_btn).clicked() {
                            accept = true;
                        }
                    });
                });

            if accept || reject {
                let state_clone = self.state.clone();
                let role = self.selected_role;
                let requester_id = req.client_id.clone();
                let decision = SessionDecisionDto {
                    request_id: req.request_id.clone(),
                    client_id: Some(requester_id.clone()),
                    target_id: Some(requester_id.clone()),
                    accept,
                    granted_role: Some(role),
                    allow_input: self.allow_input,
                    allow_clipboard: self.allow_clipboard,
                    allow_file_transfer: self.allow_files,
                    allow_terminal: self.allow_terminal,
                    rejection_reason: if accept { None } else { Some("Conexión rechazada por el anfitrión.".to_string()) },
                };

                tokio::spawn(async move {
                    if let Some(sig) = state_clone.get_signaling_client().await {
                        if let Ok(json_str) = serde_json::to_string(&decision) {
                            let msg = nxdesk_core::SignalingMessage::new("session_decision", &json_str)
                                .with_target(&requester_id);
                            let _ = sig.send_message(&msg).await;
                        }
                        if accept {
                            // Enviar inmediatamente endpoint UDP para acelerar handshake directo
                            if let Some(local_ep) = sig.p2p_local_endpoint() {
                                let ep_msg = nxdesk_core::SignalingMessage::new("udp_endpoint", &local_ep)
                                    .with_target(&requester_id);
                                let _ = sig.send_message(&ep_msg).await;
                            }
                        }
                    }
                    let _ = state_clone.decide_session(decision).await;
                });

                self.show_toast(if accept { "Conexión aceptada." } else { "Conexión rechazada." });
            }
        }
    }

    /// Widget de sesión remota activa en el Host (HostDisconnectWindow.axaml)
    fn check_and_render_active_session_widget(&mut self, ctx: &egui::Context) {
        let active = {
            if let Ok(a) = self.state.active_session.try_read() {
                a.clone()
            } else {
                None
            }
        };

        if let Some(session) = active {
            let mut disconnect = false;

            egui::Area::new(egui::Id::NULL.with("host_disconnect_panel"))
                .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -20.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(theme::BACKGROUND_SECONDARY)
                        .stroke(Stroke::new(1.0, theme::PRIMARY))
                        .corner_radius(12.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                                ui.painter().circle_filled(rect.center(), 6.0, theme::SUCCESS);

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Sesión Remota Activa (Anfitrión)").strong().color(theme::TEXT));
                                    ui.label(RichText::new(format!("Controlado por: {} ({})", session.peer_alias, session.peer_id)).size(11.0).color(theme::TEXT_MUTED));

                                    // Telemetría en vivo del Anfitrión (FPS de captura, Ping/Latencia RTT ms y Bitrate de subida)
                                    let telem = self.state.get_telemetry_snapshot();
                                    let ping_str = if telem.rtt_ms > 0 { format!("{} ms", telem.rtt_ms) } else { "<5 ms".to_string() };
                                    let kbps_str = if telem.tx_kbps >= 1000 { format!("{:.1} Mbps", telem.tx_kbps as f32 / 1000.0) } else { format!("{} kbps", telem.tx_kbps) };
                                    let (lw, lh) = telem.local_resolution;
                                    let res_str = if lw > 0 && lh > 0 { format!("{}x{}", lw, lh) } else { "1280x720".to_string() };

                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("🎥 {} FPS", telem.tx_fps)).size(11.0).strong().color(Color32::from_rgb(34, 197, 94)));
                                        ui.label(RichText::new("•").size(11.0).color(theme::BORDER));
                                        ui.label(RichText::new(format!("⚡ {}", ping_str)).size(11.0).strong().color(Color32::from_rgb(56, 189, 248)));
                                        ui.label(RichText::new("•").size(11.0).color(theme::BORDER));
                                        ui.label(RichText::new(format!("📊 {}", kbps_str)).size(11.0).color(theme::TEXT_MUTED));
                                        ui.label(RichText::new("•").size(11.0).color(theme::BORDER));
                                        ui.label(RichText::new(format!("🖥 {}", res_str)).size(11.0).color(theme::TEXT_MUTED));
                                    });
                                });

                                ui.add_space(8.0);

                                let dc_btn = egui::Button::new(
                                    RichText::new("Desconectar").strong().color(Color32::WHITE),
                                )
                                .fill(theme::ERROR);

                                if ui.add(dc_btn).clicked() {
                                    disconnect = true;
                                }
                            });
                        });
                });

         if disconnect {
             // Limpiar el estado visual local inmediatamente.
             // ctx es el parámetro de la función (fuera del closure de egui::Area no existe `ui`).
             self.disconnect_session(ctx);
         }
        }
    }
}

impl eframe::App for NxDeskApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_app_ui(ui);
    }
}

/// Cuerpo de render de la app (Header + Toasts + Vista principal + Modales).
/// Separado de `ui` para mantener la paridad con el layout original.
impl NxDeskApp {
    fn render_app_ui(&mut self, ui: &mut egui::Ui) {
        let active_peer = self
            .state
            .active_remote_peer
            .try_read()
            .ok()
            .and_then(|p| p.clone());   
       let is_session_active = active_peer.is_some()
    || self.state.is_host_active()
    || self.state.is_viewer_active();
        
        // Solicitar repintado periódico: 16ms (60 FPS) si hay sesión activa (anfitrión o visor); 80ms en reposo
        let repaint_interval = if is_session_active {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(80)
        };
        ui.ctx().request_repaint_after(repaint_interval);

        // 1. Barra superior (Header idéntico a MainWindow.axaml)
        self.render_header(ui, is_session_active, active_peer.clone());

        // 1b. Nota diferida de grabación (resultado del ensamblado AVI en background).
        if let Some(msg) = self
            .state
            .recording_note
            .lock()
            .ok()
            .and_then(|mut n| n.take())
        {
            self.show_toast(msg);
        }

        // 2. Notificaciones Toast temporales
        if let Some((ref msg, time)) = self.toast_message {
            if time.elapsed() < Duration::from_secs(3) {
                egui::Frame::new()
                    .fill(theme::BACKGROUND_SECONDARY)
                    .stroke(Stroke::new(1.0, theme::PRIMARY))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).color(theme::TEXT));
                    });
            } else {
                self.toast_message = None;
            }
        }

        ui.add_space(4.0);

        // 3. Contenido Principal: HomeView.axaml vs RemoteView.axaml
        if let Some(peer) = active_peer {
            self.render_remote_view(ui, &peer);
        } else {
            self.render_home_view(ui);
        }

        // 4. Modales flotantes (Configuración, Archivos, Terminal, Aceptación de sesión)
        self.render_settings_dialog(ui.ctx());
        self.render_files_dialog(ui.ctx());
        self.render_terminal_dialog(ui.ctx());
        self.check_and_render_incoming_session_dialog(ui.ctx());
        self.check_and_render_active_session_widget(ui.ctx());
    }
}

/// Lanza la aplicación de escritorio nativa NxDesk en Windows con OpenGL/Glow
pub fn run_desktop_app(state: AppState) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1020.0, 691.0])
            .with_min_inner_size([980.0, 600.0])
            .with_title("NxDesk")
            .with_active(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "NxDesk",
        options,
        Box::new(|cc| Ok(Box::new(NxDeskApp::new(cc, state)))),
    )
}

/// Mapea teclas de egui a códigos virtuales Win32 VK para inyección nativa
fn egui_key_to_vk(key: egui::Key) -> i32 {
    match key {
        // Flechas de navegación
        egui::Key::ArrowDown => 0x28,  // VK_DOWN
        egui::Key::ArrowLeft => 0x25,  // VK_LEFT
        egui::Key::ArrowRight => 0x27, // VK_RIGHT
        egui::Key::ArrowUp => 0x26,    // VK_UP

        // Teclas de control básicas
        egui::Key::Escape => 0x1B,     // VK_ESCAPE
        egui::Key::Tab => 0x09,        // VK_TAB
        egui::Key::Backspace => 0x08,  // VK_BACK
        egui::Key::Enter => 0x0D,      // VK_RETURN
        egui::Key::Space => 0x20,      // VK_SPACE
        egui::Key::Insert => 0x2D,     // VK_INSERT
        egui::Key::Delete => 0x2E,     // VK_DELETE
        egui::Key::Home => 0x24,       // VK_HOME
        egui::Key::End => 0x23,        // VK_END
        egui::Key::PageUp => 0x21,     // VK_PRIOR
        egui::Key::PageDown => 0x22,   // VK_NEXT

        // Teclas modificadoras
        egui::Key::ShiftLeft => 0xA0,   // VK_LSHIFT
        egui::Key::ShiftRight => 0xA1,  // VK_RSHIFT
        egui::Key::ControlLeft => 0xA2, // VK_LCONTROL
        egui::Key::ControlRight => 0xA3,// VK_RCONTROL
        egui::Key::AltLeft => 0xA4,     // VK_LMENU
        egui::Key::AltRight => 0xA5,    // VK_RMENU
        egui::Key::SuperLeft => 0x5B,   // VK_LWIN
        egui::Key::SuperRight => 0x5C,  // VK_RWIN

        // Símbolos y puntuación OEM estándar
        egui::Key::Colon => 0xBA,            // VK_OEM_1 (: / ;)
        egui::Key::Semicolon => 0xBA,        // VK_OEM_1 (; / :)
        egui::Key::Comma => 0xBC,            // VK_OEM_COMMA (,)
        egui::Key::Period => 0xBE,           // VK_OEM_PERIOD (.)
        egui::Key::Minus => 0xBD,            // VK_OEM_MINUS (-)
        egui::Key::Plus => 0xBB,             // VK_OEM_PLUS (+)
        egui::Key::Equals => 0xBB,           // VK_OEM_PLUS (=)
        egui::Key::Slash => 0xBF,            // VK_OEM_2 (/ / ?)
        egui::Key::Questionmark => 0xBF,     // VK_OEM_2 (?)
        egui::Key::Backslash => 0xDC,        // VK_OEM_5 (\ / |)
        egui::Key::Pipe => 0xDC,             // VK_OEM_5 (|)
        egui::Key::Backtick => 0xC0,         // VK_OEM_3 (` / ~)
        egui::Key::OpenBracket => 0xDB,      // VK_OEM_4 ([ / {)
        egui::Key::OpenCurlyBracket => 0xDB, // VK_OEM_4 ({)
        egui::Key::CloseBracket => 0xDD,     // VK_OEM_6 (] / })
        egui::Key::CloseCurlyBracket => 0xDD,// VK_OEM_6 (})
        egui::Key::Quote => 0xDE,            // VK_OEM_7 (' / ")

        // Números 0-9
        egui::Key::Num0 => 0x30,
        egui::Key::Num1 => 0x31,
        egui::Key::Num2 => 0x32,
        egui::Key::Num3 => 0x33,
        egui::Key::Num4 => 0x34,
        egui::Key::Num5 => 0x35,
        egui::Key::Num6 => 0x36,
        egui::Key::Num7 => 0x37,
        egui::Key::Num8 => 0x38,
        egui::Key::Num9 => 0x39,

        // Letras A-Z
        egui::Key::A => 0x41,
        egui::Key::B => 0x42,
        egui::Key::C => 0x43,
        egui::Key::D => 0x44,
        egui::Key::E => 0x45,
        egui::Key::F => 0x46,
        egui::Key::G => 0x47,
        egui::Key::H => 0x48,
        egui::Key::I => 0x49,
        egui::Key::J => 0x4A,
        egui::Key::K => 0x4B,
        egui::Key::L => 0x4C,
        egui::Key::M => 0x4D,
        egui::Key::N => 0x4E,
        egui::Key::O => 0x4F,
        egui::Key::P => 0x50,
        egui::Key::Q => 0x51,
        egui::Key::R => 0x52,
        egui::Key::S => 0x53,
        egui::Key::T => 0x54,
        egui::Key::U => 0x55,
        egui::Key::V => 0x56,
        egui::Key::W => 0x57,
        egui::Key::X => 0x58,
        egui::Key::Y => 0x59,
        egui::Key::Z => 0x5A,

        // Teclas de función F1-F12
        egui::Key::F1 => 0x70,
        egui::Key::F2 => 0x71,
        egui::Key::F3 => 0x72,
        egui::Key::F4 => 0x73,
        egui::Key::F5 => 0x74,
        egui::Key::F6 => 0x75,
        egui::Key::F7 => 0x76,
        egui::Key::F8 => 0x77,
        egui::Key::F9 => 0x78,
        egui::Key::F10 => 0x79,
        egui::Key::F11 => 0x7A,
        egui::Key::F12 => 0x7B,
        _ => 0,
    }
}
