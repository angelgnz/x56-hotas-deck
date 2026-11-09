//! Aplicación principal eframe/egui para Saitek / Logitech X56 HOTAS.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Ui};
use x56_core::{ButtonMonitor, FullProfile, ModeSlot, ProfileManager, TargetDevice, X56Device};

use crate::curve_math::LiveInputPoller;
use crate::tabs::calibrate::CalibrateTab;
use crate::tabs::curves::CurvesTab;
use crate::tabs::doctor::DoctorTab;
use crate::tabs::programming::ProgrammingTab;
use crate::tabs::rgb::RgbTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Curves,
    Programming,
    Rgb,
    Calibrate,
    Doctor,
}

pub struct X56App {
    pub active_tab: ActiveTab,
    pub curves_tab: CurvesTab,
    pub programming_tab: ProgrammingTab,
    pub rgb_tab: RgbTab,
    pub calibrate_tab: CalibrateTab,
    pub doctor_tab: DoctorTab,
    pub poller: LiveInputPoller,
    pub button_monitor: ButtonMonitor,

    // Hardware status
    pub stick_detected: bool,
    pub throttle_detected: bool,

    // Gestión de perfiles
    pub profile_path_buf: String,
    pub show_profile_dialog: bool,
    pub profile_dialog_mode: ProfileDialogMode,
    pub top_banner_message: Option<(String, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileDialogMode {
    Save,
    Load,
}

impl Default for X56App {
    fn default() -> Self {
        let mut app = Self {
            active_tab: ActiveTab::Curves,
            curves_tab: CurvesTab::default(),
            programming_tab: ProgrammingTab::default(),
            rgb_tab: RgbTab::default(),
            calibrate_tab: CalibrateTab::default(),
            doctor_tab: DoctorTab::default(),
            poller: LiveInputPoller::new(),
            button_monitor: ButtonMonitor::new(),
            stick_detected: false,
            throttle_detected: false,
            profile_path_buf: "examples/combat_profile.toml".into(),
            show_profile_dialog: false,
            profile_dialog_mode: ProfileDialogMode::Load,
            top_banner_message: None,
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

    fn render_top_bar(&mut self, ui: &mut Ui) {
        let snap = self.button_monitor.snapshot();
        ui.horizontal(|ui| {
            ui.heading("🦅 Saitek / Logitech X56 Control Center");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Indicador en vivo del Selector Rotativo de Modo (M1/M2/S1) y SLD
                let mode_color = match snap.active_mode {
                    ModeSlot::M1 => Color32::from_rgb(70, 210, 255),
                    ModeSlot::M2 => Color32::from_rgb(255, 175, 60),
                    ModeSlot::S1 => Color32::from_rgb(190, 120, 255),
                };
                let sld_str = if snap.slider_sld_on {
                    "SLD:ON"
                } else {
                    "SLD:OFF"
                };
                ui.colored_label(
                    mode_color,
                    RichText::new(format!(
                        "🎛 Modo: {} | {}",
                        snap.active_mode.as_str(),
                        sld_str
                    ))
                    .strong(),
                );

                ui.separator();

                // Indicador de Throttle
                let throttle_col = if self.throttle_detected || snap.throttle_connected {
                    Color32::from_rgb(80, 220, 80)
                } else {
                    Color32::from_rgb(140, 140, 140)
                };
                ui.colored_label(throttle_col, "🚀 Throttle");

                // Indicador de Stick
                let stick_col = if self.stick_detected || snap.stick_connected {
                    Color32::from_rgb(80, 220, 80)
                } else {
                    Color32::from_rgb(140, 140, 140)
                };
                ui.colored_label(stick_col, "🕹️ Stick");

                ui.separator();

                if ui.button("Restaurar Fábrica").clicked() {
                    let _ = ProfileManager::reset_defaults(TargetDevice::All);
                    self.curves_tab.read_from_hardware();
                    self.top_banner_message = Some((
                        "Valores de fábrica restablecidos en el hardware.".into(),
                        false,
                    ));
                }

                if ui.button("💾 Guardar Perfil").clicked() {
                    self.profile_dialog_mode = ProfileDialogMode::Save;
                    self.show_profile_dialog = true;
                }

                if ui.button("📂 Cargar Perfil").clicked() {
                    self.profile_dialog_mode = ProfileDialogMode::Load;
                    self.show_profile_dialog = true;
                }
            });
        });
    }

    fn render_nav_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tabs = [
                (ActiveTab::Curves, "📈 Curvas y Ejes"),
                (ActiveTab::Programming, "🎮 Programación (Botones y Modos)"),
                (ActiveTab::Rgb, "💡 Iluminación RGB"),
                (ActiveTab::Calibrate, "🎯 Calibrador Hall"),
                (ActiveTab::Doctor, "🩺 Diagnóstico Doctor"),
            ];

            for (tab, label) in tabs {
                let is_active = self.active_tab == tab;
                if ui.selectable_label(is_active, label).clicked() {
                    self.active_tab = tab;
                    if tab == ActiveTab::Doctor {
                        self.doctor_tab.refresh();
                    }
                }
            }
        });
    }

    fn render_profile_modal(&mut self, ctx: &egui::Context) {
        if !self.show_profile_dialog {
            return;
        }

        let title = match self.profile_dialog_mode {
            ProfileDialogMode::Save => "Guardar Perfil TOML",
            ProfileDialogMode::Load => "Cargar Perfil TOML",
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Ruta del archivo .toml:");
                ui.text_edit_singleline(&mut self.profile_path_buf);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    match self.profile_dialog_mode {
                        ProfileDialogMode::Save => {
                            if ui.button("Guardar").clicked() {
                                let path = PathBuf::from(&self.profile_path_buf);
                                match ProfileManager::dump_current("Perfil X56", None, None) {
                                    Ok(mut prof) => {
                                        prof.programming = Some(
                                            self.programming_tab.current_programming_profile(),
                                        );
                                        match prof.save_to_file(&path) {
                                            Ok(_) => {
                                                self.top_banner_message = Some((
                                                    format!("Perfil guardado en {}", path.display()),
                                                    false,
                                                ));
                                                self.show_profile_dialog = false;
                                            }
                                            Err(err) => {
                                                self.top_banner_message = Some((
                                                    format!("Error guardando perfil: {err}"),
                                                    true,
                                                ));
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        self.top_banner_message = Some((
                                            format!("Error volcando configuración: {err}"),
                                            true,
                                        ));
                                    }
                                }
                            }
                        }
                        ProfileDialogMode::Load => {
                            if ui.button("Cargar y Aplicar").clicked() {
                                let path = PathBuf::from(&self.profile_path_buf);
                                match FullProfile::load_from_file(&path) {
                                    Ok(prof) => {
                                        if let Some(ref prog) = prof.programming {
                                            self.programming_tab.load_programming_profile(
                                                prog.clone(),
                                                &self.button_monitor,
                                            );
                                        }
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
                                                        "Perfil '{}' aplicado ({} ejes stick, {} gases, {} botones programados)",
                                                        prof.name,
                                                        summary.stick_axes_updated,
                                                        summary.throttle_axes_updated,
                                                        prog_cnt
                                                    ),
                                                    false,
                                                ));
                                                self.show_profile_dialog = false;
                                            }
                                            Err(err) => {
                                                self.top_banner_message = Some((
                                                    format!("Error aplicando perfil: {err}"),
                                                    true,
                                                ));
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        self.top_banner_message = Some((
                                            format!("Error cargando archivo: {err}"),
                                            true,
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    if ui.button("Cancelar").clicked() {
                        self.show_profile_dialog = false;
                    }
                });
            });
    }
}

impl eframe::App for X56App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Redibujar a ~30 fps para suavidad del punto en vivo
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));

        egui::Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(4.0);
            self.render_top_bar(ui);
            ui.add_space(4.0);
            self.render_nav_tabs(ui);
            ui.add_space(4.0);

            if let Some((ref msg, is_err)) = self.top_banner_message {
                let col = if is_err {
                    Color32::from_rgb(255, 90, 90)
                } else {
                    Color32::from_rgb(100, 230, 100)
                };
                ui.colored_label(col, msg);
                ui.add_space(2.0);
            }
        });

        egui::CentralPanel::default().show(ui, |ui| match self.active_tab {
            ActiveTab::Curves => self.curves_tab.ui(ui, &self.poller),
            ActiveTab::Programming => self.programming_tab.ui(ui, &self.button_monitor),
            ActiveTab::Rgb => self.rgb_tab.ui(ui),
            ActiveTab::Calibrate => self.calibrate_tab.ui(ui),
            ActiveTab::Doctor => self.doctor_tab.ui(ui),
        });

        let ctx = ui.ctx().clone();
        self.render_profile_modal(&ctx);
    }
}
