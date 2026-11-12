//! Shell General Avionics Vector para Saitek / Logitech X-56 RHINO HOTAS.
//!
//! Implementa la arquitectura de 3 paneles (HeaderBar fija de 48px, Sidebar de 256px
//! con navegación de 4 subsistemas y tarjeta telemétrica inferior, Dock de acciones
//! tácticas y CentralPanel).

use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, Vec2};
use x56_core::i18n::{self, Language};
use x56_core::t;
use x56_core::{
    ButtonMonitor, DeviceProfile, FullProfile, ModeSlot, ProfileManager, TargetDevice, X56Device,
};

use crate::curve_math::LiveInputPoller;
use crate::icons;
use crate::tabs::calibrate::CalibrateTab;
use crate::tabs::curves::CurvesTab;
use crate::tabs::doctor::DoctorTab;
use crate::tabs::programming::ProgrammingTab;
use crate::tabs::rgb::RgbTab;
use crate::theme::{
    self, PhysicalSpring, COLOR_ERROR, COLOR_ON_PRIMARY_CONTAINER, COLOR_ON_SURFACE,
    COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE, COLOR_OUTLINE_VARIANT, COLOR_PRIMARY,
    COLOR_PRIMARY_CONTAINER, COLOR_PRIMARY_FIXED, COLOR_SURFACE, COLOR_SURFACE_CONTAINER,
    COLOR_SURFACE_HIGH, COLOR_SURFACE_LOW, COLOR_SURFACE_LOWEST, COLOR_TERTIARY_CONTAINER,
    COLOR_TERTIARY_FIXED_DIM, COLOR_WARNING_AMBER,
};

/// Los 4 subsistemas principales del sistema de diseño Avionics Vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveSubsystem {
    /// 1. Ejes y Curvas (Transfer Curve HUD, Hall 16-bit, Gimbal 2D, Muelles físicos).
    #[default]
    Curves,
    /// 2. Botones y Mapeo (Esquema técnico 2D, Matriz uinput M1/M2/S1).
    Mapping,
    /// 3. Iluminación RGB & Calibración (Control LED RGB y Asistente de Centro Hall).
    RgbCalibrate,
    /// 4. Perfiles & Exportación (Gestión de perfiles TOML y Diagnóstico Doctor).
    Profiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileDialogMode {
    Save,
    Load,
}

pub struct X56App {
    pub active_subsystem: ActiveSubsystem,
    pub curves_tab: CurvesTab,
    pub programming_tab: ProgrammingTab,
    pub rgb_tab: RgbTab,
    pub calibrate_tab: CalibrateTab,
    pub doctor_tab: DoctorTab,
    pub poller: LiveInputPoller,
    pub button_monitor: ButtonMonitor,

    // Estado de detección de hardware
    pub stick_detected: bool,
    pub throttle_detected: bool,

    // Gestión de perfiles y estado de sesión
    pub active_profile_name: String,
    pub profile_path_buf: String,
    pub show_profile_dialog: bool,
    pub profile_dialog_mode: ProfileDialogMode,
    pub show_live_test_modal: bool,
    pub top_banner_message: Option<(String, bool)>,
    pub sidebar_collapsed: bool,

    // Canal asíncrono para explorador de archivos nativo (rfd)
    pub file_dialog_rx: Option<std::sync::mpsc::Receiver<(ProfileDialogMode, Option<PathBuf>)>>,

    // Bandeja del sistema (Tray) y ejecución en segundo plano al cerrar
    pub minimize_to_tray_on_close: bool,
    pub window_visible_state: bool,
    pub tray_handle: Option<crate::tray::TrayHandle>,
}

impl Default for X56App {
    fn default() -> Self {
        let default_profile = if Path::new("examples/elite_dangerous_profile.toml").exists() {
            "examples/elite_dangerous_profile.toml"
        } else if Path::new("examples/combat_profile.toml").exists() {
            "examples/combat_profile.toml"
        } else {
            "examples/default_profile.toml"
        };

        let mut app = Self {
            active_subsystem: ActiveSubsystem::Curves,
            curves_tab: CurvesTab::default(),
            programming_tab: ProgrammingTab::default(),
            rgb_tab: RgbTab::default(),
            calibrate_tab: CalibrateTab::default(),
            doctor_tab: DoctorTab::default(),
            poller: LiveInputPoller::new(),
            button_monitor: ButtonMonitor::new(),
            stick_detected: false,
            throttle_detected: false,
            active_profile_name: "ELITE_DANGEROUS".into(),
            profile_path_buf: default_profile.into(),
            show_profile_dialog: false,
            profile_dialog_mode: ProfileDialogMode::Load,
            show_live_test_modal: false,
            top_banner_message: None,
            sidebar_collapsed: false,
            file_dialog_rx: None,
            minimize_to_tray_on_close: true,
            window_visible_state: true,
            tray_handle: None,
        };
        app.refresh_hardware_status();
        if Path::new(&app.profile_path_buf).exists() {
            app.load_profile_from_disk();
        }
        app
    }
}

impl X56App {
    pub fn new(egui_ctx: &egui::Context) -> Self {
        let mut app = Self::default();
        app.init_tray(egui_ctx);
        app
    }

    pub fn init_tray(&mut self, egui_ctx: &egui::Context) {
        let tray_handle = crate::tray::start_tray(egui_ctx.clone(), &self.active_profile_name);
        self.tray_handle = Some(tray_handle);
    }

    pub fn sync_tray_state(&self) {
        if let Some(ref tray) = self.tray_handle {
            if let Ok(mut p) = tray.active_profile.lock() {
                *p = self.active_profile_name.clone();
            }
        }
    }

    pub fn poll_tray_commands(&mut self, ctx: &egui::Context) {
        let mut show_win = false;
        let mut hide_win = false;
        let mut toggle_win = false;
        let mut quit_app = false;

        if let Some(ref tray) = self.tray_handle {
            while let Ok(cmd) = tray.rx.try_recv() {
                match cmd {
                    crate::tray::TrayCommand::ShowWindow => {
                        show_win = true;
                    }
                    crate::tray::TrayCommand::HideWindow => {
                        hide_win = true;
                    }
                    crate::tray::TrayCommand::ToggleWindow => {
                        toggle_win = true;
                    }
                    crate::tray::TrayCommand::Quit => {
                        quit_app = true;
                    }
                }
            }
        }

        if toggle_win {
            if self.window_visible_state {
                self.window_visible_state = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                self.window_visible_state = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        } else if show_win {
            self.window_visible_state = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        } else if hide_win {
            self.window_visible_state = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        if let Some(ref tray) = self.tray_handle {
            tray.window_visible.store(
                self.window_visible_state,
                std::sync::atomic::Ordering::Relaxed,
            );
        }

        if quit_app {
            self.minimize_to_tray_on_close = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    pub fn refresh_hardware_status(&mut self) {
        let devices = X56Device::enumerate();
        self.stick_detected = devices
            .iter()
            .any(|d| d.kind == x56_core::DeviceKind::Stick);
        self.throttle_detected = devices
            .iter()
            .any(|d| d.kind == x56_core::DeviceKind::Throttle);
    }

    /// Guarda el perfil completo actual en `self.profile_path_buf`, incluyendo `DeviceProfile.spring`
    /// y la iluminación RGB configurada por cada modo (`M1`, `M2`, `S1`) y dispositivo.
    pub fn save_current_profile(&mut self) {
        let path = PathBuf::from(&self.profile_path_buf);
        self.rgb_tab
            .ensure_all_modes_have_rgb(&mut self.programming_tab.profile);
        let stick_rgb_hex = self.rgb_tab.fallback_stick_hex.clone();
        let throttle_rgb_hex = self.rgb_tab.fallback_throttle_hex.clone();
        let spring_str = self.curves_tab.installed_spring.as_str().to_string();

        match ProfileManager::dump_current(
            &self.active_profile_name,
            Some(&stick_rgb_hex),
            Some(&throttle_rgb_hex),
        ) {
            Ok(mut prof) => {
                let stick_prof = prof.stick.get_or_insert_with(|| DeviceProfile {
                    rgb: Some(stick_rgb_hex.clone()),
                    spring: Some(spring_str.clone()),
                    axes: Vec::new(),
                });
                stick_prof.spring = Some(spring_str);

                prof.programming = Some(self.programming_tab.current_programming_profile());
                match prof.save_to_file(&path) {
                    Ok(_) => {
                        self.top_banner_message = Some((
                            format!(
                                "Perfil '{}' guardado en {}",
                                self.active_profile_name,
                                path.display()
                            ),
                            false,
                        ));
                        self.show_profile_dialog = false;
                    }
                    Err(err) => {
                        self.top_banner_message =
                            Some((format!("Error guardando perfil TOML: {err}"), true));
                    }
                }
            }
            Err(err) => {
                self.top_banner_message =
                    Some((format!("Error volcando configuración: {err}"), true));
            }
        }
    }

    /// Carga un perfil desde `self.profile_path_buf` y sincroniza todos los subsistemas.
    pub fn load_profile_from_disk(&mut self) {
        let path = PathBuf::from(&self.profile_path_buf);
        match FullProfile::load_from_file(&path) {
            Ok(prof) => {
                self.active_profile_name = prof.name.to_uppercase().replace(' ', "_");
                self.sync_tray_state();

                if let Some(ref stick_prof) = prof.stick {
                    if let Some(ref sp_str) = stick_prof.spring {
                        if let Some(sp) = PhysicalSpring::from_str_opt(sp_str) {
                            self.curves_tab.installed_spring = sp;
                        }
                    }
                }

                if let Some(ref prog) = prof.programming {
                    self.programming_tab
                        .load_programming_profile(prog.clone(), &self.button_monitor);
                }
                let fallback_stick = prof.stick.as_ref().and_then(|s| s.rgb.clone());
                let fallback_throttle = prof.throttle.as_ref().and_then(|t| t.rgb.clone());
                self.button_monitor
                    .set_fallback_rgb(fallback_stick, fallback_throttle);

                let active_hw_mode = self.button_monitor.snapshot().active_mode;
                self.rgb_tab.sync_from_loaded_profile(
                    &prof,
                    &self.programming_tab.profile,
                    active_hw_mode,
                );

                match ProfileManager::apply(&prof) {
                    Ok(summary) => {
                        self.curves_tab.read_from_hardware();
                        let prog_cnt = prof
                            .programming
                            .as_ref()
                            .map(|p| p.total_assigned_count())
                            .unwrap_or(0);
                        self.top_banner_message = Some((
                            format!(
                                "Perfil '{}' activo ({} ejes stick, {} gases, {} botones)",
                                self.active_profile_name,
                                summary.stick_axes_updated,
                                summary.throttle_axes_updated,
                                prog_cnt
                            ),
                            false,
                        ));
                        self.show_profile_dialog = false;
                    }
                    Err(err) => {
                        self.top_banner_message =
                            Some((format!("Error aplicando perfil: {err}"), true));
                    }
                }
            }
            Err(err) => {
                self.top_banner_message =
                    Some((format!("Error cargando archivo TOML: {err}"), true));
            }
        }
    }

    /// Lanza un explorador de archivos nativo del sistema operativo en un hilo secundario
    /// para no congelar ni retrasar el renderizado a 30+ FPS de egui.
    pub fn spawn_file_dialog(&mut self, mode: ProfileDialogMode) {
        if self.file_dialog_rx.is_some() {
            return; // Ya hay un diálogo nativo en curso
        }

        let current_path = PathBuf::from(&self.profile_path_buf);
        let start_dir = if current_path.is_file() {
            current_path.parent().map(|p| p.to_path_buf())
        } else if current_path.is_dir() {
            Some(current_path.clone())
        } else {
            std::env::current_dir()
                .ok()
                .map(|d| d.join("examples"))
                .filter(|p| p.exists())
                .or_else(|| std::env::current_dir().ok())
        };

        let default_name = current_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
            .unwrap_or_else(|| format!("{}.toml", self.active_profile_name.to_lowercase()));

        let (tx, rx) = std::sync::mpsc::channel();
        self.file_dialog_rx = Some(rx);

        std::thread::spawn(move || {
            let filter_toml = t!("filter-toml");
            let filter_all = t!("filter-all");
            let mut dialog = rfd::FileDialog::new()
                .add_filter(&filter_toml, &["toml"])
                .add_filter(&filter_all, &["*"]);

            if let Some(dir) = start_dir {
                dialog = dialog.set_directory(dir);
            }

            let result = match mode {
                ProfileDialogMode::Load => {
                    dialog = dialog.set_title(t!("dialog-load-title"));
                    dialog.pick_file()
                }
                ProfileDialogMode::Save => {
                    dialog = dialog
                        .set_title(t!("dialog-save-title"))
                        .set_file_name(&default_name);
                    dialog.save_file()
                }
            };
            let _ = tx.send((mode, result));
        });
    }

    /// Procesa de forma no bloqueante cualquier resultado pendiente del diálogo nativo.
    pub fn poll_file_dialog(&mut self) {
        let Some(rx) = self.file_dialog_rx.as_ref() else {
            return;
        };

        match rx.try_recv() {
            Ok((mode, Some(path))) => {
                self.file_dialog_rx = None;
                match mode {
                    ProfileDialogMode::Load => {
                        self.profile_path_buf = path.display().to_string();
                        self.load_profile_from_disk();
                    }
                    ProfileDialogMode::Save => {
                        let mut p = path.display().to_string();
                        if !p.ends_with(".toml") {
                            p.push_str(".toml");
                        }
                        self.profile_path_buf = p;
                        self.save_current_profile();
                    }
                }
            }
            Ok((_, None)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Diálogo cancelado por el usuario o cerrado sin selección
                self.file_dialog_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // El explorador de archivos sigue abierto por el usuario; la UI continúa a 30+ FPS
            }
        }
    }

    /// 1. HeaderBar superior fija (48px de alto).
    ///
    /// Título "X-56 RHINO // RUST-EVDEV" con icono de vuelo,
    /// pill telemétrico "/dev/hidraw0 & hidraw1 | 1000Hz POLLING",
    /// badge "ACTIVE PROFILE: SPACE_SIM_STARCITIZEN" y avatar.
    fn render_header_bar(&mut self, ui: &mut Ui) {
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1200.0);

        ui.horizontal_centered(|ui| {
            // Botón de alternar / colapsar barra lateral para pantallas compactas
            let toggle_label = if self.sidebar_collapsed {
                icons::MENU
            } else {
                icons::PANEL_LEFT_CLOSE
            };
            let toggle_tooltip = if self.sidebar_collapsed {
                "Expandir navegación lateral"
            } else {
                "Colapsar navegación lateral para maximizar espacio de trabajo"
            };
            if ui
                .button(RichText::new(toggle_label).size(13.0).monospace().strong())
                .on_hover_text(toggle_tooltip)
                .clicked()
            {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }

            ui.add_space(4.0);

            // Título principal adaptativo con icono de vuelo
            let app_title = if screen_w < 680.0 {
                format!("{} X-56", icons::PLANE)
            } else if screen_w < 950.0 {
                format!("{} X-56 RHINO", icons::PLANE)
            } else {
                format!("{} X-56 RHINO // RUST-EVDEV", icons::PLANE)
            };

            ui.label(
                RichText::new(app_title)
                    .color(COLOR_PRIMARY)
                    .size(13.0)
                    .strong()
                    .monospace(),
            );

            // Pill telemétrico "/dev/hidraw0 & hidraw1 | 1000Hz POLLING" solo si hay suficiente espacio (>= 860px)
            if screen_w >= 860.0 {
                ui.add_space(8.0);
                Frame::new()
                    .fill(COLOR_SURFACE_LOW)
                    .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(
                                dot_rect.center(),
                                3.5,
                                COLOR_TERTIARY_CONTAINER,
                            );
                            let poll_str = if screen_w >= 1150.0 {
                                "/dev/hidraw0 & hidraw1 | 1000Hz POLLING"
                            } else {
                                "/dev/hidraw0,1 | 1kHz"
                            };
                            ui.label(
                                RichText::new(poll_str)
                                    .color(COLOR_ON_SURFACE_VARIANT)
                                    .size(10.5)
                                    .strong()
                                    .monospace(),
                            );
                        });
                    });
            }

            // Derecha: Avatar táctico, botones rápidos y Badge ACTIVE PROFILE
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if screen_w >= 700.0 {
                    // Avatar de piloto
                    Frame::new()
                        .fill(COLOR_SURFACE_HIGH)
                        .stroke(Stroke::new(1.2, COLOR_PRIMARY_CONTAINER))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::symmetric(7, 4))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{} PLT", icons::PALETTE))
                                    .color(COLOR_PRIMARY)
                                    .size(10.5)
                                    .strong()
                                    .monospace(),
                            );
                        });
                    ui.add_space(6.0);
                }

                // Botón rápido de Guardar con explorador nativo
                let save_lbl = if screen_w < 780.0 {
                    icons::SAVE.to_string()
                } else {
                    format!("{} {}", icons::SAVE, t!("header-save-profile"))
                };
                if theme::tactical_button(ui, &save_lbl, false, false)
                    .on_hover_text(t!("header-save-profile-tooltip"))
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Save);
                }

                ui.add_space(4.0);

                // Botón rápido de Cargar con explorador nativo
                let load_lbl = if screen_w < 780.0 {
                    icons::FOLDER_OPEN.to_string()
                } else {
                    format!("{} {}", icons::FOLDER_OPEN, t!("header-load-profile"))
                };
                if theme::tactical_button(ui, &load_lbl, false, false)
                    .on_hover_text(t!("header-load-profile-tooltip"))
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Load);
                }

                ui.add_space(6.0);

                // Badge ACTIVE PROFILE
                let prof_badge_text = if screen_w < 900.0 {
                    format!("P: {}", self.active_profile_name)
                } else {
                    format!("{}: {}", t!("header-profile"), self.active_profile_name)
                };
                let prof_btn = egui::Button::new(
                    RichText::new(prof_badge_text)
                        .color(COLOR_PRIMARY_CONTAINER)
                        .size(10.0)
                        .strong()
                        .monospace(),
                )
                .fill(COLOR_SURFACE_LOW)
                .stroke(Stroke::new(1.0, COLOR_PRIMARY_CONTAINER))
                .corner_radius(CornerRadius::same(4));

                if ui
                    .add(prof_btn)
                    .on_hover_text("Haz clic para abrir el diálogo detallado de perfiles")
                    .clicked()
                {
                    self.profile_dialog_mode = ProfileDialogMode::Load;
                    self.show_profile_dialog = true;
                }
            });
        });
    }

    /// Sidebar colapsada en modo icono (52px de ancho) para pantallas compactas.
    fn render_collapsed_sidebar(&mut self, ui: &mut Ui) {
        let snap = self.button_monitor.snapshot();
        if self.programming_tab.follow_hardware_mode {
            self.programming_tab.selected_mode = snap.active_mode;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            let nav_items = [
                (
                    ActiveSubsystem::Curves,
                    icons::SLIDERS_HORIZONTAL,
                    "Ejes y Curvas\nHALL 16-BIT & TRANSFER CURVES",
                ),
                (
                    ActiveSubsystem::Mapping,
                    icons::GAMEPAD,
                    "Botones y Mapeo\nUINPUT MATRIX // M1-M2-S1",
                ),
                (
                    ActiveSubsystem::RgbCalibrate,
                    icons::LIGHTBULB,
                    "Iluminación RGB & Calibración\nLED PWM & HALL ZERO",
                ),
                (
                    ActiveSubsystem::Profiles,
                    icons::BOXES,
                    "Perfiles & Exportación\nTOML PROFILES & DOCTOR",
                ),
            ];

            for (sub, icon, tooltip) in nav_items {
                let is_active = self.active_subsystem == sub;
                let bg = if is_active {
                    COLOR_SURFACE_HIGH
                } else {
                    COLOR_SURFACE_CONTAINER
                };
                let border = if is_active {
                    Stroke::new(1.5, COLOR_PRIMARY_CONTAINER)
                } else {
                    Stroke::new(1.0, COLOR_OUTLINE_VARIANT)
                };

                let btn = egui::Button::new(RichText::new(icon).size(16.0))
                    .fill(bg)
                    .stroke(border)
                    .corner_radius(CornerRadius::same(6));

                if ui
                    .add_sized(Vec2::new(38.0, 38.0), btn)
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    self.active_subsystem = sub;
                    if sub == ActiveSubsystem::Profiles {
                        self.doctor_tab.refresh();
                    }
                }

                ui.add_space(6.0);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                // Indicador de modo
                let mode_str = snap.active_mode.as_str();
                let mode_btn = egui::Button::new(
                    RichText::new(mode_str)
                        .color(COLOR_ON_PRIMARY_CONTAINER)
                        .size(9.0)
                        .strong()
                        .monospace(),
                )
                .fill(COLOR_PRIMARY_CONTAINER)
                .corner_radius(CornerRadius::same(4));
                ui.add_sized(Vec2::new(36.0, 20.0), mode_btn)
                    .on_hover_text(format!("Modo de hardware actual: {mode_str}"));

                ui.add_space(6.0);

                // Punto uinput neon
                let (dot_r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot_r.center(), 4.0, COLOR_TERTIARY_CONTAINER);
            });
        });
    }

    /// 2. Sidebar izquierda responsiva.
    ///
    /// Subsystem Navigation con los 4 subsistemas aprobados y tarjeta inferior
    /// con UINPUT DAEMON: ONLINE, MODE SWITCH (M1/M2/S1) y LATENCY: 0.94 ms.
    fn render_sidebar(&mut self, ui: &mut Ui) {
        if self.sidebar_collapsed {
            self.render_collapsed_sidebar(ui);
            return;
        }

        let snap = self.button_monitor.snapshot();
        if self.programming_tab.follow_hardware_mode {
            self.programming_tab.selected_mode = snap.active_mode;
        }

        ui.vertical(|ui| {
            ui.label(
                RichText::new("SUBSYSTEM NAVIGATION")
                    .color(COLOR_OUTLINE)
                    .size(10.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(8.0);

            let nav_items = [
                (
                    ActiveSubsystem::Curves,
                    format!("{}  {}", icons::SLIDERS_HORIZONTAL, t!("nav-curves")),
                    "HALL 16-BIT & TRANSFER CURVES",
                ),
                (
                    ActiveSubsystem::Mapping,
                    format!("{}  {}", icons::GAMEPAD, t!("nav-programming")),
                    "UINPUT MATRIX // M1-M2-S1",
                ),
                (
                    ActiveSubsystem::RgbCalibrate,
                    format!("{}  {}", icons::LIGHTBULB, t!("nav-rgb")),
                    "LED PWM & HALL CENTER ZERO",
                ),
                (
                    ActiveSubsystem::Profiles,
                    format!("{}  {}", icons::BOXES, t!("nav-doctor")),
                    "TOML PROFILES & SYSTEM DOCTOR",
                ),
            ];

            for (sub, ref title, subtitle) in nav_items {
                let is_active = self.active_subsystem == sub;
                let bg = if is_active {
                    COLOR_SURFACE_HIGH
                } else {
                    COLOR_SURFACE_CONTAINER
                };
                let border = if is_active {
                    Stroke::new(1.5, COLOR_PRIMARY_CONTAINER)
                } else {
                    Stroke::new(1.0, COLOR_OUTLINE_VARIANT)
                };
                let title_col = if is_active {
                    COLOR_PRIMARY_CONTAINER
                } else {
                    COLOR_ON_SURFACE
                };

                let resp = Frame::new()
                    .fill(bg)
                    .stroke(border)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // Barra vertical de acento si está activo
                            let (bar_rect, _) =
                                ui.allocate_exact_size(Vec2::new(3.0, 28.0), Sense::hover());
                            if is_active {
                                ui.painter()
                                    .rect_filled(bar_rect, 1.5, COLOR_PRIMARY_CONTAINER);
                            } else {
                                ui.painter()
                                    .rect_filled(bar_rect, 1.5, COLOR_OUTLINE_VARIANT);
                            }

                            ui.vertical(|ui| {
                                ui.label(RichText::new(title).color(title_col).size(11.5).strong());
                                ui.label(
                                    RichText::new(subtitle)
                                        .color(COLOR_OUTLINE)
                                        .size(9.0)
                                        .monospace(),
                                );
                            });
                        });
                    })
                    .response;

                if resp.interact(Sense::click()).clicked() {
                    self.active_subsystem = sub;
                    if sub == ActiveSubsystem::Profiles {
                        self.doctor_tab.refresh();
                    }
                }

                ui.add_space(6.0);
            }

            // Empujar la tarjeta de estado de daemon al fondo de la Sidebar
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                Frame::new()
                    .fill(COLOR_SURFACE_LOWEST)
                    .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.vertical(|ui| {
                            // UINPUT DAEMON: ONLINE (verde neón #34f885)
                            ui.horizontal(|ui| {
                                let (dot_r, _) =
                                    ui.allocate_exact_size(Vec2::new(9.0, 9.0), Sense::hover());
                                ui.painter().circle_filled(
                                    dot_r.center(),
                                    4.0,
                                    COLOR_TERTIARY_CONTAINER,
                                );
                                ui.label(
                                    RichText::new("UINPUT DAEMON: ONLINE")
                                        .color(COLOR_TERTIARY_CONTAINER)
                                        .size(10.5)
                                        .strong()
                                        .monospace(),
                                );
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // MODE SWITCH con pills M1 (activo cian) / M2 / S1 sincronizados
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("MODE SWITCH:")
                                        .color(COLOR_OUTLINE)
                                        .size(9.5)
                                        .strong()
                                        .monospace(),
                                );

                                for mode in ModeSlot::ALL {
                                    let is_mode_active = self.programming_tab.selected_mode == mode;
                                    let (pill_bg, pill_fg, pill_border) = if is_mode_active {
                                        (
                                            COLOR_PRIMARY_CONTAINER,
                                            COLOR_ON_PRIMARY_CONTAINER,
                                            COLOR_PRIMARY,
                                        )
                                    } else {
                                        (
                                            COLOR_SURFACE_CONTAINER,
                                            COLOR_ON_SURFACE_VARIANT,
                                            COLOR_OUTLINE_VARIANT,
                                        )
                                    };

                                    let btn = egui::Button::new(
                                        RichText::new(mode.as_str())
                                            .color(pill_fg)
                                            .size(10.0)
                                            .strong()
                                            .monospace(),
                                    )
                                    .fill(pill_bg)
                                    .stroke(Stroke::new(1.0, pill_border))
                                    .corner_radius(CornerRadius::same(4));

                                    if ui.add(btn).clicked() {
                                        self.programming_tab.follow_hardware_mode = false;
                                        self.programming_tab.selected_mode = mode;
                                    }
                                }
                            });

                            ui.add_space(8.0);

                            // LATENCY: 0.94 ms
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("LATENCY:")
                                        .color(COLOR_OUTLINE)
                                        .size(9.5)
                                        .monospace(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new("0.94 ms")
                                                .color(COLOR_PRIMARY_FIXED)
                                                .size(10.5)
                                                .strong()
                                                .monospace(),
                                        );
                                    },
                                );
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // Opción de quedar corriendo en segundo plano al cerrar
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.minimize_to_tray_on_close, "");
                                let lbl = RichText::new(t!("sidebar-run-in-background"))
                                    .color(if self.minimize_to_tray_on_close {
                                        COLOR_PRIMARY_CONTAINER
                                    } else {
                                        COLOR_OUTLINE
                                    })
                                    .size(9.0)
                                    .strong()
                                    .monospace();
                                if ui
                                    .add(egui::Label::new(lbl).sense(Sense::click()))
                                    .clicked()
                                {
                                    self.minimize_to_tray_on_close =
                                        !self.minimize_to_tray_on_close;
                                }
                            })
                            .response
                            .on_hover_text(t!("sidebar-run-in-background-desc"));

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // Selector de Idioma (Project Fluent)
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{}:", t!("sidebar-language")))
                                        .color(COLOR_OUTLINE)
                                        .size(9.5)
                                        .strong()
                                        .monospace(),
                                );

                                let cur_lang = i18n::current_language();
                                for (lang, label) in
                                    [(Language::EnUs, "EN"), (Language::EsEs, "ES")]
                                {
                                    let is_active = cur_lang == lang;
                                    let (pill_bg, pill_fg, pill_border) = if is_active {
                                        (
                                            COLOR_PRIMARY_CONTAINER,
                                            COLOR_ON_PRIMARY_CONTAINER,
                                            COLOR_PRIMARY,
                                        )
                                    } else {
                                        (
                                            COLOR_SURFACE_CONTAINER,
                                            COLOR_ON_SURFACE_VARIANT,
                                            COLOR_OUTLINE_VARIANT,
                                        )
                                    };

                                    let btn = egui::Button::new(
                                        RichText::new(label)
                                            .color(pill_fg)
                                            .size(10.0)
                                            .strong()
                                            .monospace(),
                                    )
                                    .fill(pill_bg)
                                    .stroke(Stroke::new(1.0, pill_border))
                                    .corner_radius(CornerRadius::same(4));

                                    if ui.add(btn).clicked() {
                                        i18n::set_language(lang);
                                    }
                                }
                            });
                        });
                    });
            });
        });
    }

    /// 3. Dock inferior de acciones tácticas.
    ///
    /// "MODAL TEST EN VIVO", "RECALIBRAR CENTRO CERO", "REVERTIR CAMBIOS", "APLICAR A UINPUT // GUARDAR".
    fn render_bottom_dock(&mut self, ui: &mut Ui) {
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1200.0);

        ui.horizontal_centered(|ui| {
            // Estado / Mensaje informativo a la izquierda (oculto en pantallas muy estrechas si hay colisión)
            if screen_w >= 650.0 {
                if let Some((ref msg, is_err)) = self.top_banner_message {
                    let col = if is_err {
                        COLOR_ERROR
                    } else {
                        COLOR_TERTIARY_CONTAINER
                    };
                    ui.label(
                        RichText::new(format!("{} {msg}", icons::CHEVRON_RIGHT))
                            .color(col)
                            .size(11.0)
                            .monospace(),
                    );
                } else if let Some((ref msg, is_err)) = self.curves_tab.status_message {
                    let col = if is_err {
                        COLOR_ERROR
                    } else {
                        COLOR_TERTIARY_FIXED_DIM
                    };
                    ui.label(
                        RichText::new(format!("{} {msg}", icons::CHEVRON_RIGHT))
                            .color(col)
                            .size(11.0)
                            .monospace(),
                    );
                } else {
                    let tele_str = if screen_w >= 900.0 {
                        format!(
                            "{} TELEMETRÍA LISTA // USB HID INTERFACE 02",
                            icons::CHEVRON_RIGHT
                        )
                    } else {
                        format!("{} USB HID OK", icons::CHEVRON_RIGHT)
                    };
                    ui.label(
                        RichText::new(tele_str)
                            .color(COLOR_ON_SURFACE_VARIANT)
                            .size(10.5)
                            .monospace(),
                    );
                }
            }

            // Botones de acción táctica a la derecha adaptativos
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 4. APLICAR A UINPUT // GUARDAR (acento primario)
                let apply_lbl = if screen_w < 720.0 {
                    icons::SAVE.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} APLICAR", icons::SAVE)
                } else {
                    format!("{} APLICAR A UINPUT // GUARDAR", icons::SAVE)
                };

                if theme::tactical_button(ui, &apply_lbl, false, true)
                    .on_hover_text("Aplicar configuración actual a uinput y guardar perfil")
                    .clicked()
                {
                    self.curves_tab.apply_to_hardware();
                    self.button_monitor
                        .set_profile(self.programming_tab.current_programming_profile());
                    self.button_monitor.set_active(true);
                    self.save_current_profile();
                }

                ui.add_space(6.0);

                // 3. REVERTIR CAMBIOS
                let revert_lbl = if screen_w < 720.0 {
                    icons::ROTATE_CCW.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} REVERTIR", icons::ROTATE_CCW)
                } else {
                    format!("{} REVERTIR CAMBIOS", icons::ROTATE_CCW)
                };

                if theme::tactical_button(ui, &revert_lbl, false, false)
                    .on_hover_text("Revertir cambios a los valores del hardware")
                    .clicked()
                {
                    self.curves_tab.reset_current_axis();
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = Some((
                        "Cambios revertidos a los valores del hardware.".into(),
                        false,
                    ));
                }

                ui.add_space(6.0);

                // 2. RECALIBRAR CENTRO CERO
                let recal_lbl = if screen_w < 720.0 {
                    icons::CROSSHAIR.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} CALIBRAR", icons::CROSSHAIR)
                } else {
                    format!("{} RECALIBRAR CENTRO CERO", icons::CROSSHAIR)
                };

                if theme::tactical_button(ui, &recal_lbl, false, false)
                    .on_hover_text("Muestrear y recalibrar centro magnético Hall")
                    .clicked()
                {
                    self.calibrate_tab.execute_calibration(true);
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = Some((
                        "Centro magnético Hall muestreado y recalibrado.".into(),
                        false,
                    ));
                }

                ui.add_space(6.0);

                // 1. MODAL TEST EN VIVO
                let test_lbl = if screen_w < 720.0 {
                    icons::PLAY.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} TEST", icons::PLAY)
                } else {
                    format!("{} MODAL TEST EN VIVO", icons::PLAY)
                };

                if theme::tactical_button(ui, &test_lbl, self.show_live_test_modal, false)
                    .on_hover_text("Abrir ventana modal de telemetría y pruebas en vivo")
                    .clicked()
                {
                    self.show_live_test_modal = !self.show_live_test_modal;
                    self.programming_tab.show_test_window = self.show_live_test_modal;
                }
            });
        });
    }

    /// Subsistema 3: Agrupa Iluminación RGB y Calibrador Hall en diseño Avionics Vector.
    fn render_rgb_calibrate_subsystem(&mut self, ui: &mut Ui) {
        let mut save_requested = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::card_frame().show(ui, |ui| {
                    if self
                        .rgb_tab
                        .ui(ui, &mut self.programming_tab.profile, &self.button_monitor)
                    {
                        save_requested = true;
                    }
                });
                ui.add_space(12.0);
                theme::card_frame().show(ui, |ui| {
                    self.calibrate_tab.ui(ui);
                });
            });
        if save_requested {
            self.save_current_profile();
        }
    }

    /// Subsistema 4: Gestión de Perfiles TOML, exportación y Diagnóstico Doctor.
    fn render_profiles_subsystem(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::card_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} GESTIÓN DE PERFILES TOML & EXPORTACIÓN",
                            icons::FILE_TEXT
                        ))
                        .color(COLOR_PRIMARY)
                        .size(13.0)
                        .strong()
                        .monospace(),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("NOMBRE DEL PERFIL:")
                                .color(COLOR_OUTLINE)
                                .size(11.0)
                                .monospace(),
                        );
                        ui.text_edit_singleline(&mut self.active_profile_name);

                        ui.separator();

                        ui.label(
                            RichText::new("ARCHIVO TOML:")
                                .color(COLOR_OUTLINE)
                                .size(11.0)
                                .monospace(),
                        );
                        ui.text_edit_singleline(&mut self.profile_path_buf);
                        let exp_btn = format!("{} EXPLORAR...", icons::FOLDER);
                        if theme::tactical_button(ui, &exp_btn, false, false).clicked() {
                            self.spawn_file_dialog(ProfileDialogMode::Load);
                        }
                    });

                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        let load_btn = format!("{} CARGAR Y APLICAR PERFIL", icons::FOLDER_OPEN);
                        if theme::tactical_button(ui, &load_btn, false, true).clicked() {
                            self.load_profile_from_disk();
                        }

                        let exp_toml = format!("{} EXPORTAR / GUARDAR TOML", icons::SAVE);
                        if theme::tactical_button(ui, &exp_toml, false, false).clicked() {
                            self.save_current_profile();
                        }

                        let save_as = format!("{} GUARDAR COMO...", icons::SAVE);
                        if theme::tactical_button(ui, &save_as, false, false)
                            .on_hover_text(
                                "Guardar perfil en nueva ubicación con el explorador de archivos",
                            )
                            .clicked()
                        {
                            self.spawn_file_dialog(ProfileDialogMode::Save);
                        }

                        let reset_btn = format!("{} RESTAURAR VALORES DE FÁBRICA", icons::FACTORY);
                        if theme::tactical_button(ui, &reset_btn, false, false).clicked() {
                            let _ = ProfileManager::reset_defaults(TargetDevice::All);
                            self.curves_tab.read_from_hardware();
                            self.top_banner_message = Some((
                                "Valores de fábrica restablecidos en Stick y Throttle.".into(),
                                false,
                            ));
                        }

                        ui.separator();

                        theme::telemetry_badge(
                            ui,
                            "SPRING SAVED:",
                            self.curves_tab.installed_spring.label(),
                            self.curves_tab.installed_spring.color(),
                        );
                    });
                });

                ui.add_space(12.0);

                theme::card_frame().show(ui, |ui| {
                    self.doctor_tab.ui(ui);
                });
            });
    }

    fn render_profile_modal(&mut self, ctx: &egui::Context) {
        if !self.show_profile_dialog {
            return;
        }

        let title = match self.profile_dialog_mode {
            ProfileDialogMode::Save => "GUARDAR PERFIL TOML",
            ProfileDialogMode::Load => "CARGAR PERFIL TOML",
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Nombre identificador del perfil:")
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(11.0),
                );
                ui.text_edit_singleline(&mut self.active_profile_name);
                ui.add_space(4.0);

                ui.label(
                    RichText::new("Ruta del archivo .toml:")
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(11.0),
                );
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.profile_path_buf);
                    let exp_btn = format!("{} EXPLORAR...", icons::FOLDER);
                    if theme::tactical_button(ui, &exp_btn, false, false)
                        .on_hover_text("Abrir explorador de archivos nativo del sistema")
                        .clicked()
                    {
                        self.spawn_file_dialog(self.profile_dialog_mode);
                    }
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    match self.profile_dialog_mode {
                        ProfileDialogMode::Save => {
                            let save_btn = format!("{} GUARDAR", icons::SAVE);
                            if theme::tactical_button(ui, &save_btn, false, true).clicked() {
                                self.save_current_profile();
                            }
                        }
                        ProfileDialogMode::Load => {
                            let load_btn = format!("{} CARGAR Y APLICAR", icons::FOLDER_OPEN);
                            if theme::tactical_button(ui, &load_btn, false, true).clicked() {
                                self.load_profile_from_disk();
                            }
                        }
                    }

                    let save_as = format!("{} GUARDAR COMO...", icons::SAVE);
                    if theme::tactical_button(ui, &save_as, false, false)
                        .on_hover_text("Guardar con nuevo nombre o ubicación en el explorador")
                        .clicked()
                    {
                        self.profile_dialog_mode = ProfileDialogMode::Save;
                        self.spawn_file_dialog(ProfileDialogMode::Save);
                    }

                    let cancel_btn = format!("{} CANCELAR", icons::X);
                    if theme::tactical_button(ui, &cancel_btn, false, false).clicked() {
                        self.show_profile_dialog = false;
                    }
                });
            });
    }

    fn render_live_test_modal(&mut self, ctx: &egui::Context) {
        if !self.show_live_test_modal {
            return;
        }

        let live_axes = self.poller.get_state();
        let snap = self.button_monitor.snapshot();
        let mut open = self.show_live_test_modal;

        let win_title = format!(
            "{} MODAL TEST EN VIVO // TELEMETRÍA HOTAS & UINPUT",
            icons::ACTIVITY
        );
        egui::Window::new(win_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::telemetry_badge(
                        ui,
                        "ACTIVE MODE:",
                        snap.active_mode.as_str(),
                        COLOR_PRIMARY_CONTAINER,
                    );
                    theme::telemetry_badge(
                        ui,
                        "SLD:",
                        if snap.slider_sld_on { "ON" } else { "OFF" },
                        COLOR_TERTIARY_CONTAINER,
                    );
                    theme::telemetry_badge(
                        ui,
                        "PRESSED BTNS:",
                        &snap.pressed_buttons.len().to_string(),
                        COLOR_WARNING_AMBER,
                    );
                });

                ui.add_space(8.0);
                theme::sunken_frame().show(ui, |ui| {
                    let (mag, deg) = live_axes.gimbal_vector();
                    ui.label(
                        RichText::new(format!(
                            "STICK GIMBAL  -> X: {:+.3} | Y: {:+.3} | Z (TWIST): {:+.3} | VECT: {:.3} ({:02.0}°)",
                            live_axes.stick_x, live_axes.stick_y, live_axes.stick_z, mag, deg
                        ))
                        .color(COLOR_TERTIARY_CONTAINER)
                        .size(11.0)
                        .monospace(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "MINISTICK/POV -> SLEW X: {:+.2} | SLEW Y: {:+.2} | POV1: {}",
                            live_axes.ministick_x,
                            live_axes.ministick_y,
                            live_axes.pov_direction_label()
                        ))
                        .color(COLOR_PRIMARY_FIXED)
                        .size(11.0)
                        .monospace(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "DUAL THROTTLE -> LEFT: {:.1}% | RIGHT: {:.1}% | R1:{:+.2} R2:{:+.2} R3:{:+.2} R4:{:+.2}",
                            live_axes.throttle_left * 100.0,
                            live_axes.throttle_right * 100.0,
                            live_axes.rotary_1,
                            live_axes.rotary_2,
                            live_axes.rotary_3,
                            live_axes.rotary_4
                        ))
                        .color(COLOR_ON_SURFACE)
                        .size(11.0)
                        .monospace(),
                    );
                    let (t_mag, t_deg) = live_axes.throttle_gimbal_vector();
                    let t_push = snap.pressed_buttons.contains("thr_ministick_push");
                    ui.label(
                        RichText::new(format!(
                            "THR MINI-GIMB -> SLEW X: {:+.3} | SLEW Y: {:+.3} | VECT: {:.3} ({:02.0}°) | PUSH: {}",
                            live_axes.throttle_ministick_x,
                            live_axes.throttle_ministick_y,
                            t_mag,
                            t_deg,
                            if t_push { "[PULSADO]" } else { "[REPOSO]" }
                        ))
                        .color(if t_push {
                            COLOR_TERTIARY_CONTAINER
                        } else {
                            COLOR_PRIMARY_CONTAINER
                        })
                        .size(11.0)
                        .monospace(),
                    );
                });

                ui.add_space(6.0);
                ui.label(
                    RichText::new("Campo de prueba de inyección de teclado (/dev/uinput):")
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(11.0),
                );
                ui.text_edit_singleline(&mut self.programming_tab.test_input_text);
            });

        self.show_live_test_modal = open;
        self.programming_tab.show_test_window = open;
    }
}

impl eframe::App for X56App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Procesar órdenes desde el system tray (mostrar ventana, salir)
        self.poll_tray_commands(ui.ctx());

        // Procesar cualquier selección del explorador de archivos nativo
        self.poll_file_dialog();

        // Si el usuario solicita cerrar la ventana y tiene activado el segundo plano,
        // cancelamos el cierre del proceso y ocultamos la ventana a la bandeja del sistema.
        if ui.input(|i| i.viewport().close_requested()) && self.minimize_to_tray_on_close {
            self.window_visible_state = false;
            if let Some(ref tray) = self.tray_handle {
                tray.window_visible
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Tasa de refresco dinámica: 30 FPS cuando está visible, 4 FPS cuando está en tray/segundo plano
        if self.window_visible_state {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        // 1. HeaderBar superior fija (48px de alto)
        egui::Panel::top("avionics_header_bar")
            .exact_size(48.0)
            .frame(theme::header_frame())
            .show(ui, |ui| {
                self.render_header_bar(ui);
            });

        // 2. Dock inferior de acciones tácticas
        egui::Panel::bottom("avionics_bottom_dock")
            .frame(theme::dock_frame())
            .show(ui, |ui| {
                self.render_bottom_dock(ui);
            });

        // 3. Sidebar izquierda responsiva / colapsable
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1200.0);
        let sidebar_w = if self.sidebar_collapsed {
            54.0
        } else if screen_w < 1100.0 {
            200.0
        } else {
            240.0
        };

        egui::Panel::left("avionics_sidebar")
            .exact_size(sidebar_w)
            .resizable(false)
            .frame(theme::sidebar_frame())
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });

        // 4. CentralPanel que delega al subsistema activo
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(COLOR_SURFACE)
                    .inner_margin(Margin::same(14)),
            )
            .show(ui, |ui| match self.active_subsystem {
                ActiveSubsystem::Curves => {
                    self.curves_tab.ui(ui, &self.poller, &self.button_monitor)
                }
                ActiveSubsystem::Mapping => {
                    self.programming_tab
                        .ui(ui, &self.button_monitor, &self.poller)
                }
                ActiveSubsystem::RgbCalibrate => self.render_rgb_calibrate_subsystem(ui),
                ActiveSubsystem::Profiles => self.render_profiles_subsystem(ui),
            });

        let ctx = ui.ctx().clone();
        self.render_profile_modal(&ctx);
        self.render_live_test_modal(&ctx);
    }
}
