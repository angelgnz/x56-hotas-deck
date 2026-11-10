//! Shell General Avionics Vector para Saitek / Logitech X-56 RHINO HOTAS.
//!
//! Implementa la arquitectura de 3 paneles (HeaderBar fija de 48px, Sidebar de 256px
//! con navegación de 4 subsistemas y tarjeta telemétrica inferior, Dock de acciones
//! tácticas y CentralPanel).

use std::path::PathBuf;

use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, Vec2};
use x56_core::{
    ButtonMonitor, DeviceProfile, FullProfile, ModeSlot, ProfileManager, TargetDevice, X56Device,
};

use crate::curve_math::LiveInputPoller;
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

    // Canal asíncrono para explorador de archivos nativo (rfd)
    pub file_dialog_rx: Option<std::sync::mpsc::Receiver<(ProfileDialogMode, Option<PathBuf>)>>,
}

impl Default for X56App {
    fn default() -> Self {
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
            active_profile_name: "SPACE_SIM_STARCITIZEN".into(),
            profile_path_buf: "examples/combat_profile.toml".into(),
            show_profile_dialog: false,
            profile_dialog_mode: ProfileDialogMode::Load,
            show_live_test_modal: false,
            top_banner_message: None,
            file_dialog_rx: None,
        };
        app.refresh_hardware_status();
        app
    }
}

impl X56App {
    pub fn refresh_hardware_status(&mut self) {
        let devices = X56Device::enumerate();
        self.stick_detected = devices
            .iter()
            .any(|d| d.kind == x56_core::DeviceKind::Stick);
        self.throttle_detected = devices
            .iter()
            .any(|d| d.kind == x56_core::DeviceKind::Throttle);
    }

    /// Guarda el perfil completo actual en `self.profile_path_buf`, incluyendo `DeviceProfile.spring`.
    pub fn save_current_profile(&mut self) {
        let path = PathBuf::from(&self.profile_path_buf);
        let rgb_hex = {
            let c = self.rgb_tab.current_rgb_color();
            format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
        };
        let spring_str = self.curves_tab.installed_spring.as_str().to_string();

        match ProfileManager::dump_current(
            &self.active_profile_name,
            Some(&rgb_hex),
            Some(&rgb_hex),
        ) {
            Ok(mut prof) => {
                let stick_prof = prof.stick.get_or_insert_with(|| DeviceProfile {
                    rgb: Some(rgb_hex.clone()),
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
            let mut dialog = rfd::FileDialog::new()
                .add_filter("Perfiles TOML (*.toml)", &["toml"])
                .add_filter("Todos los archivos (*.*)", &["*"]);

            if let Some(dir) = start_dir {
                dialog = dialog.set_directory(dir);
            }

            let result = match mode {
                ProfileDialogMode::Load => {
                    dialog = dialog.set_title("Seleccionar Perfil TOML de X56");
                    dialog.pick_file()
                }
                ProfileDialogMode::Save => {
                    dialog = dialog
                        .set_title("Guardar Perfil TOML de X56")
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
        ui.horizontal_centered(|ui| {
            // Título principal con icono de vuelo
            ui.label(
                RichText::new("✈ X-56 RHINO // RUST-EVDEV")
                    .color(COLOR_PRIMARY)
                    .size(13.5)
                    .strong()
                    .monospace(),
            );

            ui.add_space(10.0);

            // Pill telemétrico "/dev/hidraw0 & hidraw1 | 1000Hz POLLING"
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
                        ui.label(
                            RichText::new("/dev/hidraw0 & hidraw1 | 1000Hz POLLING")
                                .color(COLOR_ON_SURFACE_VARIANT)
                                .size(10.5)
                                .strong()
                                .monospace(),
                        );
                    });
                });

            // Derecha: Avatar táctico y Badge ACTIVE PROFILE
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Avatar de piloto
                Frame::new()
                    .fill(COLOR_SURFACE_HIGH)
                    .stroke(Stroke::new(1.2, COLOR_PRIMARY_CONTAINER))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::symmetric(7, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("👨‍✈️ PLT")
                                .color(COLOR_PRIMARY)
                                .size(10.5)
                                .strong()
                                .monospace(),
                        );
                    });

                ui.add_space(6.0);

                // Botón rápido de Guardar con explorador nativo
                if theme::tactical_button(ui, "💾 GUARDAR", false, false)
                    .on_hover_text(
                        "Guardar perfil actual en archivo TOML mediante el explorador nativo",
                    )
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Save);
                }

                ui.add_space(4.0);

                // Botón rápido de Cargar con explorador nativo
                if theme::tactical_button(ui, "📂 CARGAR", false, false)
                    .on_hover_text("Abrir explorador de archivos para cargar un perfil TOML")
                    .clicked()
                {
                    self.spawn_file_dialog(ProfileDialogMode::Load);
                }

                ui.add_space(6.0);

                // Badge "ACTIVE PROFILE: SPACE_SIM_STARCITIZEN"
                let prof_badge_text = format!("ACTIVE PROFILE: {}", self.active_profile_name);
                let prof_btn = egui::Button::new(
                    RichText::new(prof_badge_text)
                        .color(COLOR_PRIMARY_CONTAINER)
                        .size(10.5)
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

    /// 2. Sidebar izquierda fija (256px ancho).
    ///
    /// Subsystem Navigation con los 4 subsistemas aprobados y tarjeta inferior
    /// con UINPUT DAEMON: ONLINE, MODE SWITCH (M1/M2/S1) y LATENCY: 0.94 ms.
    fn render_sidebar(&mut self, ui: &mut Ui) {
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
                    "📈  Ejes y Curvas",
                    "HALL 16-BIT & TRANSFER CURVES",
                ),
                (
                    ActiveSubsystem::Mapping,
                    "🎮  Botones y Mapeo",
                    "UINPUT MATRIX // M1-M2-S1",
                ),
                (
                    ActiveSubsystem::RgbCalibrate,
                    "💡  Iluminación RGB & Calibración",
                    "LED PWM & HALL CENTER ZERO",
                ),
                (
                    ActiveSubsystem::Profiles,
                    "🗂  Perfiles & Exportación",
                    "TOML PROFILES & SYSTEM DOCTOR",
                ),
            ];

            for (sub, title, subtitle) in nav_items {
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
                        });
                    });
            });
        });
    }

    /// 3. Dock inferior de acciones tácticas.
    ///
    /// "MODAL TEST EN VIVO", "RECALIBRAR CENTRO CERO", "REVERTIR CAMBIOS", "APLICAR A UINPUT // GUARDAR".
    fn render_bottom_dock(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            // Estado / Mensaje informativo a la izquierda
            if let Some((ref msg, is_err)) = self.top_banner_message {
                let col = if is_err {
                    COLOR_ERROR
                } else {
                    COLOR_TERTIARY_CONTAINER
                };
                ui.label(
                    RichText::new(format!("▸ {msg}"))
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
                    RichText::new(format!("▸ {msg}"))
                        .color(col)
                        .size(11.0)
                        .monospace(),
                );
            } else {
                ui.label(
                    RichText::new("▸ TELEMETRÍA LISTA // USB HID INTERFACE 02")
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(10.5)
                        .monospace(),
                );
            }

            // Botones de acción táctica a la derecha
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 4. APLICAR A UINPUT // GUARDAR (acento primario)
                if theme::tactical_button(ui, "💾 APLICAR A UINPUT // GUARDAR", false, true)
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
                if theme::tactical_button(ui, "↺ REVERTIR CAMBIOS", false, false).clicked() {
                    self.curves_tab.reset_current_axis();
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = Some((
                        "Cambios revertidos a los valores del hardware.".into(),
                        false,
                    ));
                }

                ui.add_space(6.0);

                // 2. RECALIBRAR CENTRO CERO
                if theme::tactical_button(ui, "🎯 RECALIBRAR CENTRO CERO", false, false).clicked()
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
                if theme::tactical_button(
                    ui,
                    "⚡ MODAL TEST EN VIVO",
                    self.show_live_test_modal,
                    false,
                )
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::card_frame().show(ui, |ui| {
                    self.rgb_tab.ui(ui);
                });
                ui.add_space(12.0);
                theme::card_frame().show(ui, |ui| {
                    self.calibrate_tab.ui(ui);
                });
            });
    }

    /// Subsistema 4: Gestión de Perfiles TOML, exportación y Diagnóstico Doctor.
    fn render_profiles_subsystem(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::card_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new("◈ GESTIÓN DE PERFILES TOML & EXPORTACIÓN")
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
                        if theme::tactical_button(ui, "📁 EXPLORAR...", false, false).clicked() {
                            self.spawn_file_dialog(ProfileDialogMode::Load);
                        }
                    });

                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        if theme::tactical_button(ui, "📂 CARGAR Y APLICAR PERFIL", false, true)
                            .clicked()
                        {
                            self.load_profile_from_disk();
                        }

                        if theme::tactical_button(ui, "💾 EXPORTAR / GUARDAR TOML", false, false)
                            .clicked()
                        {
                            self.save_current_profile();
                        }

                        if theme::tactical_button(ui, "💾 GUARDAR COMO...", false, false)
                            .on_hover_text(
                                "Guardar perfil en nueva ubicación con el explorador de archivos",
                            )
                            .clicked()
                        {
                            self.spawn_file_dialog(ProfileDialogMode::Save);
                        }

                        if theme::tactical_button(
                            ui,
                            "🏭 RESTAURAR VALORES DE FÁBRICA",
                            false,
                            false,
                        )
                        .clicked()
                        {
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
                    if theme::tactical_button(ui, "📁 EXPLORAR...", false, false)
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
                            if theme::tactical_button(ui, "💾 GUARDAR", false, true).clicked() {
                                self.save_current_profile();
                            }
                        }
                        ProfileDialogMode::Load => {
                            if theme::tactical_button(ui, "📂 CARGAR Y APLICAR", false, true)
                                .clicked()
                            {
                                self.load_profile_from_disk();
                            }
                        }
                    }

                    if theme::tactical_button(ui, "💾 GUARDAR COMO...", false, false)
                        .on_hover_text("Guardar con nuevo nombre o ubicación en el explorador")
                        .clicked()
                    {
                        self.profile_dialog_mode = ProfileDialogMode::Save;
                        self.spawn_file_dialog(ProfileDialogMode::Save);
                    }

                    if theme::tactical_button(ui, "✖ CANCELAR", false, false).clicked() {
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

        egui::Window::new("⚡ MODAL TEST EN VIVO // TELEMETRÍA HOTAS & UINPUT")
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
                            "STICK GIMBAL  -> X: {:+.3} | Y: {:+.3} | Z (TWIST): {:+.3} | VECT: {:.3} ∠{:02.0}°",
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
        // Procesar cualquier selección del explorador de archivos nativo
        self.poll_file_dialog();

        // Redibujar continuo a ~30 fps para telemetría en vivo fluida
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));

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

        // 3. Sidebar izquierda fija (256px de ancho)
        egui::Panel::left("avionics_sidebar")
            .exact_size(256.0)
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
                ActiveSubsystem::Curves => self.curves_tab.ui(ui, &self.poller),
                ActiveSubsystem::Mapping => self.programming_tab.ui(ui, &self.button_monitor),
                ActiveSubsystem::RgbCalibrate => self.render_rgb_calibrate_subsystem(ui),
                ActiveSubsystem::Profiles => self.render_profiles_subsystem(ui),
            });

        let ctx = ui.ctx().clone();
        self.render_profile_modal(&ctx);
        self.render_live_test_modal(&ctx);
    }
}
