//! Pestaña de diagnóstico del sistema (`x56 doctor`) e integridad de permisos.

use eframe::egui::{self, Color32, Ui};
use x56_core::{CheckStatus, Doctor, DoctorReport};

use crate::icons;

pub struct DoctorTab {
    pub report: DoctorReport,
}

impl Default for DoctorTab {
    fn default() -> Self {
        Self {
            report: Doctor::run(),
        }
    }
}

impl DoctorTab {
    pub fn refresh(&mut self) {
        self.report = Doctor::run();
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.heading("Diagnóstico del Sistema e Integridad (`x56 doctor`)");
        ui.label(
            "Verifica que las reglas udev, permisos sin root y drivers del kernel estén listos.",
        );

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let refresh_lbl = format!("{} Re-analizar Sistema", icons::REFRESH_CW);
            if ui.button(refresh_lbl).clicked() {
                self.refresh();
            }

            let status_badge = if self.report.is_healthy() {
                (
                    Color32::from_rgb(40, 160, 40),
                    format!("{} SISTEMA 100% OPERATIVO", icons::CHECK),
                )
            } else if self.report.fail_count > 0 {
                (
                    Color32::from_rgb(200, 40, 40),
                    format!("{} SE REQUIEREN ACCIONES CORRECTIVAS", icons::X),
                )
            } else {
                (
                    Color32::from_rgb(200, 150, 20),
                    format!("{} ADVERTENCIAS DETECTADAS", icons::ALERT_TRIANGLE),
                )
            };

            ui.colored_label(status_badge.0, status_badge.1);
            ui.label(format!(
                "({} OK, {} advertencias, {} fallos)",
                self.report.ok_count, self.report.warn_count, self.report.fail_count
            ));
        });

        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for check in &self.report.checks {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let (badge_col, tag) = match check.status {
                            CheckStatus::Ok => (
                                Color32::from_rgb(80, 220, 80),
                                format!("{} OK", icons::CHECK),
                            ),
                            CheckStatus::Warn => (
                                Color32::from_rgb(255, 200, 50),
                                format!("{} WARN", icons::ALERT_TRIANGLE),
                            ),
                            CheckStatus::Fail => {
                                (Color32::from_rgb(255, 70, 70), format!("{} FAIL", icons::X))
                            }
                        };

                        ui.colored_label(badge_col, tag);
                        ui.label(format!(
                            "[{}] {}",
                            check.category.to_uppercase(),
                            check.title
                        ));
                    });

                    ui.label(format!("  {}", check.message));

                    if let Some(ref remedy) = check.remedy {
                        ui.colored_label(
                            Color32::from_rgb(255, 180, 80),
                            format!("  {} Solución: {}", icons::CORNER_DOWN_RIGHT, remedy),
                        );
                    }
                });
                ui.add_space(4.0);
            }
        });
    }
}
