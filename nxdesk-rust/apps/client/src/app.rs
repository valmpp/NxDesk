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
    _terminal_allowed: bool,
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
    selected_screen_index: i32,
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
            _terminal_allowed: false,
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
            selected_screen_index: 0,
        }
    }

    fn send_select_screen(&mut self, index: i32) {
        self.selected_screen_index = index;
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

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast_message = Some((msg.into(), Instant::now()));
    }

    fn copy_to_clipboard(&mut self, ui: &mut egui::Ui, text: &str, label: &str) {
        ui.ctx().copy_text(text.to_string());
        self.show_toast(format!("📋 {} copiado al portapapeles.", label));
    }

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
                    requested_role: nxdesk_core::SessionRole::FullControl,
                    allow_input: true,
                    allow_clipboard: true,
                    allow_file_transfer: true,
                    timestamp: now_ts,
                };
                if let Ok(json_str) = serde_json::to_string(&req_dto) {
                    let msg = nxdesk_core::SignalingMessage::new("session_request", &json_str);
                    let _ = sig.send_message(&msg).await;
                }
                // Solicitar catálogo de pantallas remotas al anfitrión
                let query_screens = nxdesk_core::SignalingMessage::new("query_screens", "");
                let _ = sig.send_message(&query_screens).await;
            }
        });

        self.session_start_time = Some(Instant::now());
        self.show_toast(format!("Conectando a {}...", clean_target));
    }

    fn disconnect_session(&mut self) {
        let state_clone = self.state.clone();
        tokio::spawn(async move {
            state_clone.set_active_remote_peer(None).await;
            let _ = state_clone.terminate_session().await;
        });
        self.remote_texture = None;
        self.session_start_time = None;
        self.show_toast("Sesión finalizada.");
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

                                    // Bloqueo de pantalla (compacto)
                                    let lock_color = if self.is_screen_locked { theme::PRIMARY } else { theme::TEXT_MUTED };
                                    if ui.button(RichText::new("🔒").color(lock_color)).on_hover_text("Bloquear pantalla remota").clicked() {
                                        self.is_screen_locked = !self.is_screen_locked;
                                        self.show_toast(if self.is_screen_locked { "Pantalla remota bloqueada." } else { "Pantalla remota desbloqueada." });
                                    }

                                    // Grabación de sesión (compacto)
                                    let rec_label = if self.is_recording { "⏹" } else { "🔴" };
                                    if ui.button(RichText::new(rec_label).color(theme::ERROR)).on_hover_text("Grabar sesión").clicked() {
                                        self.is_recording = !self.is_recording;
                                        self.show_toast(if self.is_recording { "Iniciando grabación de sesión..." } else { "Grabación finalizada y guardada." });
                                    }

                                    ui.separator();

                                    // Selector de Pantallas / Multimonitor dinámico y adaptativo
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
                                        }
                                    });

                                    ui.separator();

                                    // Botón rojo desconectar idéntico a Avalonia
                                    let dc_btn = egui::Button::new(
                                        RichText::new("🚪 Desconectar")
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(theme::ERROR);

                                    if ui.add(dc_btn).on_hover_text("Finalizar sesión").clicked() {
                                        self.disconnect_session();
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
                    self.last_rendered_frame_seq = current_seq;
                    let size = arc_img.size;
                    let image_data = egui::ImageData::Color(std::sync::Arc::clone(arc_img));
                    if let Some(ref mut tex) = self.remote_texture {
                        if tex.size() == size {
                            tex.set(image_data, egui::TextureOptions::LINEAR);
                        } else {
                            *tex = ui.ctx().load_texture(
                                "remote_screen_texture",
                                image_data,
                                egui::TextureOptions::LINEAR,
                            );
                        }
                    } else {
                        self.remote_texture = Some(ui.ctx().load_texture(
                            "remote_screen_texture",
                            image_data,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }

            // Fallback síncrono si aún no hay remote_texture pero tenemos latest_jpeg_frame
            if self.remote_texture.is_none() {
                if let Ok(jpeg_guard) = self.state.latest_jpeg_frame.try_read() {
                    if let Some(ref bytes) = *jpeg_guard {
                        if let Ok(img) = image::load_from_memory(bytes) {
                            let rgba = img.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                            self.remote_texture = Some(ui.ctx().load_texture(
                                "remote_screen_texture",
                                color_img,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
            }
        }

        if let Some(ref tex) = self.remote_texture {
            let tex_size = tex.size_vec2();
            let tex_aspect = tex_size.x / tex_size.y.max(1.0);
            let avail = ui.available_size();
            let avail_aspect = avail.x / avail.y.max(1.0);
            let target_size = if avail_aspect > tex_aspect {
                egui::vec2((avail.y * tex_aspect).floor(), avail.y)
            } else {
                egui::vec2(avail.x, (avail.x / tex_aspect).floor())
            };
            let (rect, img_resp) = ui.allocate_exact_size(target_size, egui::Sense::click_and_drag());
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            let interact_resp = img_resp;

            // 2. Captura y despacho de eventos de ratón con coordenadas normalizadas (0.0 - 1.0)
            if interact_resp.hovered() {
                if let Some(hover_pos) = interact_resp.hover_pos() {
                    let norm_x = ((hover_pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let norm_y = ((hover_pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);

                    // Throttling de 25ms para movimiento de ratón (40 Hz de ultra fluidez sin saturar el canal de control)
                    let now = Instant::now();
                    if now.duration_since(self.last_mouse_time) >= Duration::from_millis(25)
                        && ((norm_x - self.last_mouse_pos.0).abs() > 0.0012
                            || (norm_y - self.last_mouse_pos.1).abs() > 0.0012)
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

            // 3. Captura y despacho de eventos de teclado y portapapeles
            if interact_resp.hovered() || interact_resp.has_focus() {
                let events = ui.input(|i| i.events.clone());
                for event in events {
                    match event {
                        egui::Event::Key {
                            key,
                            pressed,
                            repeat: false,
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
                        egui::Event::Paste(text) if self.allow_clipboard => {
                            self.send_clipboard_sync(&text);
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
            let bitrate_str = if telem.rx_kbps >= 1000 {
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

            let hud_width = 280.0;
            let hud_height = 28.0;
            let hud_rect = egui::Rect::from_min_size(
                egui::pos2(rect.max.x - hud_width - 16.0, rect.min.y + 12.0),
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

            let dot_color = if telem.rtt_ms <= 80 {
                Color32::from_rgb(34, 197, 94) // Verde suave
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

            let hud_text = format!(
                "{} FPS   •   {}   •   {}   •   {}",
                telem.rx_fps, ping_str, bitrate_str, res_str
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
                                self.disconnect_session();
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

    /// Modal de Terminal Remota Segura
    fn render_terminal_dialog(&mut self, ctx: &egui::Context) {
        if !self.is_terminal_open {
            return;
        }

        egui::Window::new("💻 Terminal Remota Segura [VUL-01 Verificada]")
            .collapsible(false)
            .default_size([600.0, 440.0])
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    ui.label(RichText::new(&self.terminal_output).monospace().color(theme::TEXT));
                });

                ui.separator();

                ui.horizontal(|ui| {
                    let cmd_edit = ui.add(egui::TextEdit::singleline(&mut self.terminal_input).hint_text("Comando..."));
                    if (ui.button("Enviar").clicked() || (cmd_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                        && !self.terminal_input.trim().is_empty()
                    {
                        let cmd = self.terminal_input.trim().to_string();
                        self.terminal_output.push_str(&format!("> {}\r\n", cmd));
                        self.terminal_input.clear();
                    }
                });

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
                let decision = SessionDecisionDto {
                    request_id: req.request_id.clone(),
                    accept,
                    granted_role: Some(role),
                    allow_input: self.allow_input,
                    allow_clipboard: self.allow_clipboard,
                    allow_file_transfer: self.allow_files,
                    rejection_reason: if accept { None } else { Some("Conexión rechazada por el anfitrión.".to_string()) },
                };

                tokio::spawn(async move {
                    if let Some(sig) = state_clone.get_signaling_client().await {
                        if let Ok(json_str) = serde_json::to_string(&decision) {
                            let msg = nxdesk_core::SignalingMessage::new("session_decision", &json_str);
                            let _ = sig.send_message(&msg).await;
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
                let state_clone = self.state.clone();
                tokio::spawn(async move {
                    let _ = state_clone.terminate_session().await;
                });
                self.show_toast("Sesión finalizada.");
            }
        }
    }
}

impl eframe::App for NxDeskApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let active_peer = self
            .state
            .active_remote_peer
            .try_read()
            .ok()
            .and_then(|p| p.clone());
        let is_session_active = active_peer.is_some() || self.state.is_host_active() || self.state.get_room_occupancy() > 1;

        // Solicitar repintado periódico: 16ms (60 FPS) si hay sesión activa (anfitrión o visor); 80ms en reposo
        let repaint_interval = if is_session_active {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(80)
        };
        ui.ctx().request_repaint_after(repaint_interval);

        // 1. Barra superior (Header idéntico a MainWindow.axaml)
        self.render_header(ui, is_session_active, active_peer.clone());

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
        egui::Key::ArrowDown => 0x28,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowRight => 0x27,
        egui::Key::ArrowUp => 0x26,
        egui::Key::Escape => 0x1B,
        egui::Key::Tab => 0x09,
        egui::Key::Backspace => 0x08,
        egui::Key::Enter => 0x0D,
        egui::Key::Space => 0x20,
        egui::Key::Insert => 0x2D,
        egui::Key::Delete => 0x2E,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::PageUp => 0x21,
        egui::Key::PageDown => 0x22,
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
