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
    ButtonMonitor, CheckStatus, DeviceProfile, FullProfile, ModeSlot, ProfileManager,
    SessionConfig, TargetDevice, X56Device,
};

use crate::curve_math::LiveInputPoller;
use crate::icons;
use crate::tabs::calibrate::CalibrateTab;
use crate::tabs::curves::CurvesTab;
use crate::tabs::doctor::DoctorTab;
use crate::tabs::programming::ProgrammingTab;
use crate::tabs::rgb::RgbTab;
use crate::theme::{
    self, PhysicalSpring, COLOR_ACCENT, COLOR_BORDER_HAIRLINE, COLOR_NEGATIVE, COLOR_POSITIVE,
    COLOR_SURFACE, COLOR_SURFACE_CONTAINER, COLOR_SURFACE_HIGH, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY, COLOR_TOP_HIGHLIGHT, COLOR_WARNING, FONT_BODY,
    FONT_HEADING, FONT_META, RADIUS_CONTROL, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS,
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
    pub stick_node: Option<String>,
    pub throttle_node: Option<String>,
    pub last_hw_scan: std::time::Instant,

    // Gestión de perfiles y estado de sesión
    pub active_profile_name: String,
    pub profile_path_buf: String,
    pub show_live_test_modal: bool,
    pub top_banner_message: Option<(String, bool)>,
    pub sidebar_collapsed: bool,

    // Canal asíncrono para explorador de archivos nativo (rfd)
    pub file_dialog_rx: Option<std::sync::mpsc::Receiver<(ProfileDialogMode, Option<PathBuf>)>>,

    // Bandeja del sistema (Tray) y ejecución en segundo plano al cerrar
    pub minimize_to_tray_on_close: bool,
    pub window_visible_state: bool,
    pub tray_handle: Option<crate::tray::TrayHandle>,
    pub last_uinput_active: bool,
}

impl Default for X56App {
    fn default() -> Self {
        let session = SessionConfig::load();
        if let Some(ref lang_code) = session.language {
            if lang_code.eq_ignore_ascii_case("en-US") || lang_code.eq_ignore_ascii_case("en") {
                i18n::set_language(Language::EnUs);
            } else if lang_code.eq_ignore_ascii_case("es-ES")
                || lang_code.eq_ignore_ascii_case("es")
            {
                i18n::set_language(Language::EsEs);
            }
        }
        let initial_path = session.resolve_initial_profile_path();

        let mut app = Self {
            active_subsystem: ActiveSubsystem::Curves,
            curves_tab: CurvesTab::default(),
            programming_tab: ProgrammingTab::default(),
            rgb_tab: RgbTab::default(),
            calibrate_tab: CalibrateTab::default(),
            doctor_tab: DoctorTab::default(),
            poller: LiveInputPoller::new(),
            button_monitor: ButtonMonitor::new(),
            stick_node: None,
            throttle_node: None,
            last_hw_scan: std::time::Instant::now() - std::time::Duration::from_secs(10),
            active_profile_name: "Default".into(),
            profile_path_buf: initial_path.display().to_string(),
            show_live_test_modal: false,
            top_banner_message: None,
            sidebar_collapsed: false,
            file_dialog_rx: None,
            minimize_to_tray_on_close: session.minimize_to_tray_on_close,
            window_visible_state: true,
            tray_handle: None,
            last_uinput_active: session.uinput_active,
        };
        app.refresh_hardware_status();
        app.button_monitor.set_active(session.uinput_active);
        if Path::new(&app.profile_path_buf).exists() {
            app.load_profile_from_disk();
        }
        app
    }
}

impl X56App {
    pub fn new(
        egui_ctx: &egui::Context,
        instance_listener: Option<std::os::unix::net::UnixListener>,
    ) -> Self {
        let mut app = Self::default();
        app.init_tray(egui_ctx, instance_listener);
        app
    }

    pub fn init_tray(
        &mut self,
        egui_ctx: &egui::Context,
        instance_listener: Option<std::os::unix::net::UnixListener>,
    ) {
        let tray_handle = crate::tray::start_tray(
            egui_ctx.clone(),
            &self.active_profile_name,
            instance_listener,
        );
        self.tray_handle = Some(tray_handle);
    }

    pub fn sync_tray_state(&self) {
        if let Some(ref tray) = self.tray_handle {
            if let Ok(mut p) = tray.active_profile.lock() {
                *p = self.active_profile_name.clone();
            }
            if let Ok(mut u) = tray.uinput_online.lock() {
                *u = self.button_monitor.uinput_available();
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        } else if show_win {
            self.window_visible_state = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
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
    /// Reenumera dispositivos USB como máximo una vez cada 2 segundos y cachea los nodos hidraw.
    pub fn refresh_hardware_status(&mut self) {
        if self.last_hw_scan.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.last_hw_scan = std::time::Instant::now();
        let devices = X56Device::enumerate();
        let node = |kind: x56_core::DeviceKind| {
            devices.iter().find(|d| d.kind == kind).and_then(|d| {
                Path::new(d.path.to_string_lossy().as_ref())
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            })
        };
        self.stick_node = node(x56_core::DeviceKind::Stick);
        self.throttle_node = node(x56_core::DeviceKind::Throttle);
    }

    /// Guarda la configuración de sesión de usuario (`~/.config/x56linux/session.toml`).
    pub fn persist_session_config(&mut self) {
        self.last_uinput_active = self.button_monitor.is_active();
        let lang_str = match i18n::current_language() {
            Language::EnUs => "en-US".to_string(),
            Language::EsEs => "es-ES".to_string(),
        };
        let cfg = SessionConfig {
            last_profile_path: Some(self.profile_path_buf.clone()),
            uinput_active: self.last_uinput_active,
            minimize_to_tray_on_close: self.minimize_to_tray_on_close,
            language: Some(lang_str),
        };
        if let Err(err) = cfg.save() {
            log::warn!("Could not persist session config: {err}");
        }
    }

    /// Guarda el perfil completo actual en `self.profile_path_buf`, incluyendo `DeviceProfile.spring`
    /// y la iluminación RGB configurada por cada modo (`M1`, `M2`, `S1`) y dispositivo.
    pub fn save_current_profile(&mut self) {
        let path = PathBuf::from(&self.profile_path_buf);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
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
                        self.profile_path_buf = std::fs::canonicalize(&path)
                            .unwrap_or_else(|_| path.clone())
                            .display()
                            .to_string();
                        self.persist_session_config();
                        let path_str = path.display().to_string();
                        self.top_banner_message = Some((
                            t!(
                                "profile-saved-path",
                                "name" => &self.active_profile_name,
                                "path" => &path_str
                            ),
                            false,
                        ));
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        self.top_banner_message =
                            Some((t!("profile-save-err", "err" => &err_str), true));
                    }
                }
            }
            Err(err) => {
                let err_str = err.to_string();
                self.top_banner_message = Some((t!("profile-dump-err", "err" => &err_str), true));
            }
        }
    }

    /// Carga un perfil desde `self.profile_path_buf` y sincroniza todos los subsistemas.
    pub fn load_profile_from_disk(&mut self) {
        let path = PathBuf::from(&self.profile_path_buf);
        match FullProfile::load_from_file(&path) {
            Ok(prof) => {
                self.active_profile_name = prof.name.clone();
                self.sync_tray_state();

                if let Some(ref stick_prof) = prof.stick {
                    if let Some(ref sp_str) = stick_prof.spring {
                        if let Some(sp) = PhysicalSpring::from_str_opt(sp_str) {
                            self.curves_tab.installed_spring = sp;
                        }
                    }
                }

                match ProfileManager::apply(&prof) {
                    Ok(summary) => {
                        self.curves_tab.read_from_hardware();
                        let prog_cnt = prof
                            .programming
                            .as_ref()
                            .map(|p| p.total_assigned_count())
                            .unwrap_or(0);
                        let stick_s = summary.stick_axes_updated.to_string();
                        let thr_s = summary.throttle_axes_updated.to_string();
                        let btn_s = prog_cnt.to_string();
                        self.top_banner_message = Some((
                            t!(
                                "profile-loaded-summary",
                                "name" => &self.active_profile_name,
                                "stick" => &stick_s,
                                "throttle" => &thr_s,
                                "buttons" => &btn_s
                            ),
                            false,
                        ));
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        self.top_banner_message =
                            Some((t!("profile-apply-err", "err" => &err_str), true));
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
                self.profile_path_buf = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| path.clone())
                    .display()
                    .to_string();
                self.persist_session_config();
            }
            Err(err) => {
                let err_str = err.to_string();
                self.top_banner_message = Some((t!("profile-load-err", "err" => &err_str), true));
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

    /// 1. Top Header Bar (56px).
    fn render_header_bar(&mut self, ui: &mut Ui) {
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1280.0);

        ui.horizontal_centered(|ui| {
            let toggle_label = if self.sidebar_collapsed {
                icons::MENU
            } else {
                icons::PANEL_LEFT_CLOSE
            };
            let toggle_tooltip = if self.sidebar_collapsed {
                t!("sidebar-expand-tooltip")
            } else {
                t!("sidebar-collapse-tooltip")
            };
            if theme::ghost_button(ui, toggle_label)
                .on_hover_text(toggle_tooltip)
                .clicked()
            {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }

            ui.add_space(SPACE_SM);

            ui.label(
                RichText::new(t!("app-title"))
                    .color(COLOR_TEXT_PRIMARY)
                    .size(FONT_HEADING),
            );

            if screen_w >= 860.0 {
                ui.add_space(SPACE_MD);
                let any_hw = self.stick_node.is_some() || self.throttle_node.is_some();
                let dot_color = if any_hw {
                    COLOR_POSITIVE
                } else {
                    COLOR_WARNING
                };
                theme::status_dot(ui, dot_color, 8.0);
                ui.add_space(SPACE_XS);

                let poll_str = match (&self.stick_node, &self.throttle_node) {
                    (Some(s), Some(t)) => format!("{s} & {t} · 50 Hz"),
                    (Some(s), None) => format!("{s} · 50 Hz"),
                    (None, Some(t)) => format!("{t} · 50 Hz"),
                    (None, None) => t!("header-no-usb"),
                };
                ui.label(
                    RichText::new(poll_str)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_META)
                        .monospace(),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let prof_label = if screen_w < 900.0 {
                    self.active_profile_name.clone()
                } else {
                    format!("{}: {}", t!("header-profile"), self.active_profile_name)
                };
                if theme::secondary_button(ui, &prof_label)
                    .on_hover_text(t!("header-profile-tooltip"))
                    .clicked()
                {
                    self.active_subsystem = ActiveSubsystem::Profiles;
                    self.doctor_tab.refresh();
                }

                ui.add_space(SPACE_XS);

                let save_lbl = if screen_w < 780.0 {
                    icons::SAVE.to_string()
                } else {
                    format!("{} {}", icons::SAVE, t!("header-save-profile"))
                };
                if theme::secondary_button(ui, &save_lbl)
                    .on_hover_text(t!("header-save-profile-tooltip"))
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Save);
                }

                ui.add_space(SPACE_XS);

                let load_lbl = if screen_w < 780.0 {
                    icons::FOLDER_OPEN.to_string()
                } else {
                    format!("{} {}", icons::FOLDER_OPEN, t!("header-load-profile"))
                };
                if theme::secondary_button(ui, &load_lbl)
                    .on_hover_text(t!("header-load-profile-tooltip"))
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Load);
                }
            });
        });
    }

    /// Collapsed icon-only sidebar (56px wide, 8px horizontal padding -> 40px inner width).
    fn render_collapsed_sidebar(&mut self, ui: &mut Ui) {
        let snap = self.button_monitor.snapshot();
        if self.programming_tab.follow_hardware_mode {
            self.programming_tab.selected_mode = snap.active_mode;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(SPACE_XS);
            let nav_items = [
                (
                    ActiveSubsystem::Curves,
                    icons::SLIDERS_HORIZONTAL,
                    t!("nav-curves"),
                ),
                (
                    ActiveSubsystem::Mapping,
                    icons::GAMEPAD,
                    t!("nav-programming"),
                ),
                (
                    ActiveSubsystem::RgbCalibrate,
                    icons::LIGHTBULB,
                    t!("nav-rgb"),
                ),
                (ActiveSubsystem::Profiles, icons::BOXES, t!("nav-doctor")),
            ];

            for (sub, icon, tooltip) in nav_items {
                let is_active = self.active_subsystem == sub;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 36.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    let fill = if is_active {
                        COLOR_SURFACE_HIGH
                    } else if resp.hovered() {
                        COLOR_SURFACE_CONTAINER
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    painter.rect_filled(rect, CornerRadius::same(RADIUS_CONTROL), fill);
                    if is_active {
                        painter.hline(
                            (rect.min.x + RADIUS_CONTROL as f32)
                                ..=(rect.max.x - RADIUS_CONTROL as f32),
                            rect.min.y + 1.0,
                            Stroke::new(1.0, COLOR_TOP_HIGHLIGHT),
                        );
                        let accent_bar = egui::Rect::from_min_size(
                            egui::Pos2::new(rect.min.x, rect.min.y + 6.0),
                            Vec2::new(2.0, rect.height() - 12.0),
                        );
                        painter.rect_filled(accent_bar, CornerRadius::same(1), COLOR_ACCENT);
                    }
                    let icon_col = if is_active || resp.hovered() {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_MUTED
                    };
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(FONT_BODY),
                        icon_col,
                    );
                }

                if resp.on_hover_text(tooltip).clicked() {
                    self.active_subsystem = sub;
                    if sub == ActiveSubsystem::Profiles {
                        self.doctor_tab.refresh();
                    }
                }

                ui.add_space(SPACE_XS);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                let mode_str = snap.active_mode.as_str();
                theme::mode_detent(ui, mode_str, true)
                    .on_hover_text(t!("sidebar-mode-status", "mode" => mode_str));

                ui.add_space(SPACE_SM);

                let uinput_ready = self.button_monitor.uinput_available();
                let (uinput_color, uinput_tooltip) = if uinput_ready {
                    (COLOR_POSITIVE, t!("sidebar-uinput-online-tooltip"))
                } else {
                    (COLOR_NEGATIVE, t!("sidebar-uinput-offline-tooltip"))
                };
                let (dot_r, dot_resp) =
                    ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot_r.center(), 4.0, uinput_color);
                dot_resp.on_hover_text(uinput_tooltip);
            });
        });
    }

    /// 2. Left Sidebar (240px expanded / 56px collapsed).
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
            ui.add_space(SPACE_XS);
            ui.label(
                RichText::new(t!("sidebar-nav-heading"))
                    .color(COLOR_TEXT_MUTED)
                    .size(FONT_META),
            );
            ui.add_space(SPACE_SM);

            let nav_items = [
                (
                    ActiveSubsystem::Curves,
                    icons::SLIDERS_HORIZONTAL,
                    t!("nav-curves"),
                ),
                (
                    ActiveSubsystem::Mapping,
                    icons::GAMEPAD,
                    t!("nav-programming"),
                ),
                (
                    ActiveSubsystem::RgbCalibrate,
                    icons::LIGHTBULB,
                    t!("nav-rgb"),
                ),
                (ActiveSubsystem::Profiles, icons::BOXES, t!("nav-doctor")),
            ];

            for (sub, icon, title) in nav_items {
                let is_active = self.active_subsystem == sub;
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    let fill = if is_active {
                        COLOR_SURFACE_HIGH
                    } else if resp.hovered() {
                        COLOR_SURFACE_CONTAINER
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    painter.rect_filled(rect, CornerRadius::same(RADIUS_CONTROL), fill);

                    if is_active {
                        painter.hline(
                            (rect.min.x + RADIUS_CONTROL as f32)
                                ..=(rect.max.x - RADIUS_CONTROL as f32),
                            rect.min.y + 1.0,
                            Stroke::new(1.0, COLOR_TOP_HIGHLIGHT),
                        );
                        let accent_bar = egui::Rect::from_min_size(
                            egui::Pos2::new(rect.min.x, rect.min.y + 6.0),
                            Vec2::new(2.0, rect.height() - 12.0),
                        );
                        painter.rect_filled(accent_bar, CornerRadius::same(1), COLOR_ACCENT);
                    }

                    let icon_col = if is_active || resp.hovered() {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_MUTED
                    };
                    let text_col = if is_active {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    };

                    let icon_center =
                        egui::Pos2::new(rect.min.x + SPACE_MD + 10.0, rect.center().y);
                    painter.text(
                        icon_center,
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(FONT_BODY),
                        icon_col,
                    );

                    let label_pos =
                        egui::Pos2::new(rect.min.x + SPACE_MD + 20.0 + SPACE_SM, rect.center().y);
                    painter.text(
                        label_pos,
                        egui::Align2::LEFT_CENTER,
                        &title,
                        egui::FontId::proportional(FONT_BODY),
                        text_col,
                    );
                }

                if resp.clicked() {
                    self.active_subsystem = sub;
                    if sub == ActiveSubsystem::Profiles {
                        self.doctor_tab.refresh();
                    }
                }

                ui.add_space(SPACE_XS);
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                theme::subcard_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.vertical(|ui| {
                        // uinput router status
                        let uinput_ready = self.button_monitor.uinput_available();
                        let (uinput_color, uinput_label, uinput_tooltip) = if uinput_ready {
                            (
                                COLOR_POSITIVE,
                                t!("sidebar-uinput-online"),
                                t!("sidebar-uinput-online-tooltip"),
                            )
                        } else {
                            (
                                COLOR_NEGATIVE,
                                t!("sidebar-uinput-offline"),
                                t!("sidebar-uinput-offline-tooltip"),
                            )
                        };

                        ui.horizontal(|ui| {
                            let (icon_slot, _) =
                                ui.allocate_exact_size(Vec2::new(20.0, 16.0), Sense::hover());
                            ui.painter()
                                .circle_filled(icon_slot.center(), 4.0, uinput_color);
                            ui.label(
                                RichText::new(uinput_label)
                                    .color(COLOR_TEXT_SECONDARY)
                                    .size(FONT_META),
                            );
                        })
                        .response
                        .on_hover_text(uinput_tooltip);

                        ui.add_space(SPACE_XS);

                        // Clickable udev / Proton row
                        let udev_check = self
                            .doctor_tab
                            .report
                            .checks
                            .iter()
                            .find(|c| c.category == "udev");
                        let (proton_color, proton_icon, proton_label) = match udev_check {
                            Some(c) if c.status == CheckStatus::Ok && uinput_ready => (
                                COLOR_POSITIVE,
                                icons::SHIELD_CHECK,
                                t!("sidebar-proton-udev-ok"),
                            ),
                            Some(c) if c.status == CheckStatus::Fail => (
                                COLOR_NEGATIVE,
                                icons::ALERT_TRIANGLE,
                                t!("sidebar-proton-udev-fail"),
                            ),
                            None => (
                                COLOR_NEGATIVE,
                                icons::ALERT_TRIANGLE,
                                t!("sidebar-proton-udev-fail"),
                            ),
                            Some(_) => (
                                COLOR_WARNING,
                                icons::ALERT_TRIANGLE,
                                t!("sidebar-proton-udev-warn"),
                            ),
                        };
                        let uinput_status = if uinput_ready {
                            t!("sidebar-uinput-online")
                        } else {
                            t!("sidebar-uinput-offline")
                        };
                        let detail = match udev_check {
                            Some(check) => match &check.remedy {
                                Some(remedy) => {
                                    format!("{} ({}) [{}]", check.message, remedy, uinput_status)
                                }
                                None => format!("{} [{}]", check.message, uinput_status),
                            },
                            None => format!("{} [{}]", t!("sidebar-udev-not-run"), uinput_status),
                        };

                        let (udev_rect, udev_resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 24.0),
                            Sense::click(),
                        );
                        let udev_resp = udev_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if ui.is_rect_visible(udev_rect) {
                            let painter = ui.painter();
                            if udev_resp.hovered() {
                                painter.rect_filled(
                                    udev_rect,
                                    CornerRadius::same(RADIUS_CONTROL),
                                    COLOR_SURFACE_HIGH,
                                );
                            }
                            let icon_center =
                                egui::Pos2::new(udev_rect.min.x + 10.0, udev_rect.center().y);
                            painter.text(
                                icon_center,
                                egui::Align2::CENTER_CENTER,
                                proton_icon,
                                egui::FontId::proportional(FONT_META),
                                proton_color,
                            );
                            let label_pos = egui::Pos2::new(
                                udev_rect.min.x + 20.0 + SPACE_SM,
                                udev_rect.center().y,
                            );
                            painter.text(
                                label_pos,
                                egui::Align2::LEFT_CENTER,
                                &proton_label,
                                egui::FontId::proportional(FONT_META),
                                if udev_resp.hovered() {
                                    COLOR_TEXT_PRIMARY
                                } else {
                                    COLOR_TEXT_SECONDARY
                                },
                            );
                        }
                        if udev_resp
                            .on_hover_text(t!(
                                "sidebar-proton-udev-tooltip",
                                "detail" => detail.as_str()
                            ))
                            .clicked()
                        {
                            self.active_subsystem = ActiveSubsystem::Profiles;
                            self.doctor_tab.refresh();
                        }

                        ui.add_space(SPACE_SM);
                        ui.separator();
                        ui.add_space(SPACE_SM);

                        // Rotary mode detent selector (M1 / M2 / S1)
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(t!("sidebar-mode-switch"))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(FONT_META),
                            );
                            for mode in ModeSlot::ALL {
                                let is_mode_active = self.programming_tab.selected_mode == mode;
                                if theme::mode_detent(ui, mode.as_str(), is_mode_active).clicked() {
                                    self.programming_tab.follow_hardware_mode = false;
                                    self.programming_tab.selected_mode = mode;
                                }
                            }
                        });

                        ui.add_space(SPACE_SM);
                        ui.separator();
                        ui.add_space(SPACE_SM);

                        // Run in background on close (neutral text)
                        ui.horizontal(|ui| {
                            let prev_tray = self.minimize_to_tray_on_close;
                            ui.checkbox(&mut self.minimize_to_tray_on_close, "");
                            let lbl = RichText::new(t!("sidebar-run-in-background"))
                                .color(COLOR_TEXT_SECONDARY)
                                .size(FONT_META);
                            if ui
                                .add(egui::Label::new(lbl).sense(Sense::click()))
                                .clicked()
                            {
                                self.minimize_to_tray_on_close = !self.minimize_to_tray_on_close;
                            }
                            if self.minimize_to_tray_on_close != prev_tray {
                                self.persist_session_config();
                            }
                        })
                        .response
                        .on_hover_text(t!("sidebar-run-in-background-desc"));

                        ui.add_space(SPACE_SM);
                        ui.separator();
                        ui.add_space(SPACE_SM);

                        // Language segmented toggle (EN / ES)
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(t!("sidebar-language"))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(FONT_META),
                            );
                            let cur_lang = i18n::current_language();
                            for (lang, label) in [(Language::EnUs, "EN"), (Language::EsEs, "ES")] {
                                if theme::segmented_button(ui, label, cur_lang == lang).clicked() {
                                    i18n::set_language(lang);
                                    self.persist_session_config();
                                }
                            }
                        });
                    });
                });
            });
        });
    }

    /// 3. Bottom Action Dock (48px).
    fn render_bottom_dock(&mut self, ui: &mut Ui) {
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1280.0);

        ui.horizontal_centered(|ui| {
            if screen_w >= 650.0 {
                if let Some((ref msg, is_err)) = self.top_banner_message {
                    let col = if is_err {
                        COLOR_NEGATIVE
                    } else {
                        COLOR_POSITIVE
                    };
                    ui.label(RichText::new(msg).color(col).size(FONT_META));
                } else if let Some((ref msg, is_err)) = self.curves_tab.status_message {
                    let col = if is_err {
                        COLOR_NEGATIVE
                    } else {
                        COLOR_POSITIVE
                    };
                    ui.label(RichText::new(msg).color(col).size(FONT_META));
                } else {
                    let connected = self.stick_node.is_some() || self.throttle_node.is_some();
                    let tele_str = if !connected {
                        t!("dock-offline")
                    } else if screen_w >= 900.0 {
                        t!("dock-ready")
                    } else {
                        t!("dock-ready-short")
                    };
                    ui.label(
                        RichText::new(tele_str)
                            .color(COLOR_TEXT_SECONDARY)
                            .size(FONT_META),
                    );
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Single primary off-white CTA
                let apply_lbl = if screen_w < 720.0 {
                    icons::SAVE.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} {}", icons::SAVE, t!("dock-apply-short"))
                } else {
                    format!("{} {}", icons::SAVE, t!("dock-apply"))
                };

                if theme::primary_button(ui, &apply_lbl)
                    .on_hover_text(t!("dock-apply-tooltip"))
                    .clicked()
                {
                    self.curves_tab.apply_to_hardware();
                    self.button_monitor
                        .set_profile(self.programming_tab.current_programming_profile());
                    self.button_monitor.set_active(true);
                    self.save_current_profile();
                }

                ui.add_space(SPACE_SM);

                let revert_lbl = if screen_w < 720.0 {
                    icons::ROTATE_CCW.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} {}", icons::ROTATE_CCW, t!("dock-revert-short"))
                } else {
                    format!("{} {}", icons::ROTATE_CCW, t!("dock-revert"))
                };

                if theme::secondary_button(ui, &revert_lbl)
                    .on_hover_text(t!("dock-revert-tooltip"))
                    .clicked()
                {
                    self.curves_tab.reset_current_axis();
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = self.curves_tab.status_message.clone();
                }

                ui.add_space(SPACE_SM);

                let recal_lbl = if screen_w < 720.0 {
                    icons::CROSSHAIR.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} {}", icons::CROSSHAIR, t!("dock-sample-short"))
                } else {
                    format!("{} {}", icons::CROSSHAIR, t!("dock-sample"))
                };

                if theme::secondary_button(ui, &recal_lbl)
                    .on_hover_text(t!("dock-sample-tooltip"))
                    .clicked()
                {
                    self.calibrate_tab.execute_calibration(true);
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = self.calibrate_tab.status_message.clone();
                }

                ui.add_space(SPACE_SM);

                let test_lbl = if screen_w < 720.0 {
                    icons::PLAY.to_string()
                } else if screen_w < 1100.0 {
                    format!("{} {}", icons::PLAY, t!("dock-test-short"))
                } else {
                    format!("{} {}", icons::PLAY, t!("dock-test"))
                };

                let test_resp = if self.show_live_test_modal {
                    theme::segmented_button(ui, &test_lbl, true)
                } else {
                    theme::ghost_button(ui, &test_lbl)
                };
                if test_resp.on_hover_text(t!("dock-test-tooltip")).clicked() {
                    self.show_live_test_modal = !self.show_live_test_modal;
                }
            });
        });
    }

    /// Subsystem 3: RGB Lighting and Hall Center Calibration.
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
                ui.add_space(SPACE_MD);
                theme::card_frame().show(ui, |ui| {
                    self.calibrate_tab.ui(ui);
                });
            });
        if save_requested {
            self.save_current_profile();
        }
    }

    /// Subsystem 4: TOML Profiles & System Diagnostics.
    fn render_profiles_subsystem(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    theme::section_heading(ui, &t!("profile-section-title"));
                    ui.add_space(SPACE_MD);

                    // Row 1: Profile name
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(t!("profile-name-label"))
                                .color(COLOR_TEXT_SECONDARY)
                                .size(FONT_META),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.active_profile_name)
                                .desired_width(ui.available_width().min(380.0)),
                        );
                    });

                    ui.add_space(SPACE_SM);

                    // Row 2: TOML file path (full remaining width) + Browse button
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(t!("profile-path-label"))
                                .color(COLOR_TEXT_SECONDARY)
                                .size(FONT_META),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let exp_btn = format!("{} {}", icons::FOLDER, t!("profile-browse"));
                            if theme::secondary_button(ui, &exp_btn).clicked() {
                                self.spawn_file_dialog(ProfileDialogMode::Load);
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.profile_path_buf)
                                    .desired_width(ui.available_width()),
                            );
                        });
                    });

                    ui.add_space(SPACE_MD);

                    ui.horizontal_wrapped(|ui| {
                        let load_btn =
                            format!("{} {}", icons::FOLDER_OPEN, t!("profile-load-apply"));
                        if theme::secondary_button(ui, &load_btn).clicked() {
                            self.load_profile_from_disk();
                        }

                        let exp_toml = format!("{} {}", icons::SAVE, t!("profile-save"));
                        if theme::secondary_button(ui, &exp_toml).clicked() {
                            self.save_current_profile();
                        }

                        let save_as = format!("{} {}", icons::SAVE, t!("profile-save-as"));
                        if theme::secondary_button(ui, &save_as)
                            .on_hover_text(t!("profile-save-as-tooltip"))
                            .clicked()
                        {
                            self.spawn_file_dialog(ProfileDialogMode::Save);
                        }

                        let reset_btn =
                            format!("{} {}", icons::FACTORY, t!("profile-factory-reset"));
                        if theme::secondary_button(ui, &reset_btn).clicked() {
                            let connected =
                                self.stick_node.is_some() || self.throttle_node.is_some();
                            match ProfileManager::reset_defaults(TargetDevice::All) {
                                Ok(_) if connected => {
                                    self.curves_tab.read_from_hardware();
                                    self.top_banner_message =
                                        Some((t!("profile-factory-reset-ok"), false));
                                }
                                Ok(_) => {
                                    self.top_banner_message =
                                        Some((t!("profile-factory-reset-no-usb"), true));
                                }
                                Err(err) => {
                                    let err_str = err.to_string();
                                    self.top_banner_message = Some((
                                        t!("profile-factory-reset-err", "err" => &err_str),
                                        true,
                                    ));
                                }
                            }
                        }

                        ui.add_space(SPACE_SM);

                        theme::meta_kv(
                            ui,
                            &t!("profile-spring-saved"),
                            &self.curves_tab.installed_spring.localized_label(),
                            self.curves_tab.installed_spring.color(),
                        );
                    });
                });

                ui.add_space(SPACE_MD);

                theme::card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    self.doctor_tab.ui(ui);
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

        egui::Window::new(t!("live-test-title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(620.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::meta_kv(
                        ui,
                        &t!("live-test-mode"),
                        snap.active_mode.as_str(),
                        COLOR_ACCENT,
                    );
                    ui.add_space(SPACE_MD);
                    let sld_val = if snap.slider_sld_on {
                        t!("live-test-on")
                    } else {
                        t!("live-test-off")
                    };
                    let sld_col = if snap.slider_sld_on {
                        COLOR_POSITIVE
                    } else {
                        COLOR_TEXT_SECONDARY
                    };
                    theme::meta_kv(ui, &t!("live-test-sld"), &sld_val, sld_col);
                    ui.add_space(SPACE_MD);
                    let pressed_count = snap.pressed_buttons.len().to_string();
                    let pressed_col = if snap.pressed_buttons.is_empty() {
                        COLOR_TEXT_SECONDARY
                    } else {
                        COLOR_TEXT_PRIMARY
                    };
                    theme::meta_kv(ui, &t!("live-test-pressed"), &pressed_count, pressed_col);
                });

                ui.add_space(SPACE_SM);
                theme::canvas_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let (mag, deg) = live_axes.gimbal_vector();
                    let sx = format!("{:+.3}", live_axes.stick_x);
                    let sy = format!("{:+.3}", live_axes.stick_y);
                    let sz = format!("{:+.3}", live_axes.stick_z);
                    let svec = format!("{mag:.3}");
                    let sdeg = format!("{deg:03.0}");
                    ui.label(
                        RichText::new(t!(
                            "live-test-stick-row",
                            "x" => &sx,
                            "y" => &sy,
                            "z" => &sz,
                            "vec" => &svec,
                            "deg" => &sdeg
                        ))
                        .color(COLOR_TEXT_PRIMARY)
                        .size(FONT_META)
                        .monospace(),
                    );

                    let mx = format!("{:+.3}", live_axes.ministick_x);
                    let my = format!("{:+.3}", live_axes.ministick_y);
                    let pov = live_axes.pov_direction_label();
                    ui.label(
                        RichText::new(t!(
                            "live-test-mini-row",
                            "x" => &mx,
                            "y" => &my,
                            "pov" => pov
                        ))
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_META)
                        .monospace(),
                    );

                    let tl = format!("{:5.1}", live_axes.throttle_left * 100.0);
                    let tr = format!("{:5.1}", live_axes.throttle_right * 100.0);
                    let r1 = format!("{:+.2}", live_axes.rotary_1);
                    let r2 = format!("{:+.2}", live_axes.rotary_2);
                    let r3 = format!("{:+.2}", live_axes.rotary_3);
                    let r4 = format!("{:+.2}", live_axes.rotary_4);
                    ui.label(
                        RichText::new(t!(
                            "live-test-throttle-row",
                            "l" => &tl,
                            "r" => &tr,
                            "r1" => &r1,
                            "r2" => &r2,
                            "r3" => &r3,
                            "r4" => &r4
                        ))
                        .color(COLOR_TEXT_PRIMARY)
                        .size(FONT_META)
                        .monospace(),
                    );

                    let (t_mag, t_deg) = live_axes.throttle_gimbal_vector();
                    let t_push = snap.pressed_buttons.contains("thr_ministick_push");
                    let tx = format!("{:+.3}", live_axes.throttle_ministick_x);
                    let ty = format!("{:+.3}", live_axes.throttle_ministick_y);
                    let tvec = format!("{t_mag:.3}");
                    let tdeg = format!("{t_deg:03.0}");
                    let push_state = if t_push {
                        t!("live-test-state-pressed")
                    } else {
                        t!("live-test-state-idle")
                    };
                    ui.label(
                        RichText::new(t!(
                            "live-test-thr-mini-row",
                            "x" => &tx,
                            "y" => &ty,
                            "vec" => &tvec,
                            "deg" => &tdeg,
                            "push" => &push_state
                        ))
                        .color(if t_push {
                            COLOR_TEXT_PRIMARY
                        } else {
                            COLOR_TEXT_SECONDARY
                        })
                        .size(FONT_META)
                        .monospace(),
                    );
                });

                ui.add_space(SPACE_SM);
                ui.label(
                    RichText::new(t!("live-test-keyboard-label"))
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_META),
                );
                ui.add_space(SPACE_XS);
                ui.add(
                    egui::TextEdit::singleline(&mut self.programming_tab.test_input_text)
                        .desired_width(f32::INFINITY),
                );
            });

        self.show_live_test_modal = open;
    }
}

impl eframe::App for X56App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_tray_commands(ui.ctx());
        self.refresh_hardware_status();
        self.poll_file_dialog();

        if self.button_monitor.is_active() != self.last_uinput_active {
            self.persist_session_config();
        }

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

        // Smart repaint cadence: 30 FPS for live telemetry tabs/modal, 2 Hz for static tabs, 0.5 Hz in tray
        if self.window_visible_state {
            if matches!(
                self.active_subsystem,
                ActiveSubsystem::Curves | ActiveSubsystem::Mapping
            ) || self.show_live_test_modal
            {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            }
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(2));
        }

        // 1. Top Header Bar (56px, 1px bottom hairline)
        let header_resp = egui::Panel::top("avionics_header_bar")
            .exact_size(56.0)
            .frame(theme::header_frame())
            .show(ui, |ui| {
                self.render_header_bar(ui);
            });
        ui.painter().hline(
            header_resp.response.rect.x_range(),
            header_resp.response.rect.bottom(),
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
        );

        // 2. Bottom Action Dock (48px, 1px top hairline)
        let dock_resp = egui::Panel::bottom("avionics_bottom_dock")
            .exact_size(48.0)
            .frame(theme::dock_frame())
            .show(ui, |ui| {
                self.render_bottom_dock(ui);
            });
        ui.painter().hline(
            dock_resp.response.rect.x_range(),
            dock_resp.response.rect.top(),
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
        );

        // 3. Left Sidebar (240px expanded / 56px collapsed, 1px right hairline)
        let screen_w = ui
            .ctx()
            .input(|i| i.raw.screen_rect)
            .map(|r| r.width())
            .unwrap_or(1280.0);
        let sidebar_w = if self.sidebar_collapsed {
            56.0
        } else if screen_w < 1100.0 {
            200.0
        } else {
            240.0
        };
        let sidebar_frame = if self.sidebar_collapsed {
            theme::sidebar_frame().inner_margin(Margin::symmetric(SPACE_SM as i8, SPACE_LG as i8))
        } else {
            theme::sidebar_frame()
        };

        let sidebar_resp = egui::Panel::left("avionics_sidebar")
            .exact_size(sidebar_w)
            .resizable(false)
            .frame(sidebar_frame)
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });
        ui.painter().vline(
            sidebar_resp.response.rect.right(),
            sidebar_resp.response.rect.y_range(),
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
        );

        // 4. CentralPanel
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(COLOR_SURFACE)
                    .inner_margin(Margin::same(SPACE_LG as i8)),
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
        self.render_live_test_modal(&ctx);
    }
}
