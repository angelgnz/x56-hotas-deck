//! Pestaña de diagnóstico del sistema (`x56 doctor`) e integridad de permisos.

use eframe::egui::{self, Color32, Ui};
use x56_core::{CheckStatus, Doctor, DoctorReport};

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
            if ui.button("🔄 Re-analizar Sistema").clicked() {
                self.refresh();
            }

            let status_badge = if self.report.is_healthy() {
                (Color32::from_rgb(40, 160, 40), "✓ SISTEMA 100% OPERATIVO")
            } else if self.report.fail_count > 0 {
                (
                    Color32::from_rgb(200, 40, 40),
                    "✗ SE REQUIEREN ACCIONES CORRECTIVAS",
                )
            } else {
                (Color32::from_rgb(200, 150, 20), "! ADVERTENCIAS DETECTADAS")
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
                            CheckStatus::Ok => (Color32::from_rgb(80, 220, 80), "[OK]"),
                            CheckStatus::Warn => (Color32::from_rgb(255, 200, 50), "[WARN]"),
                            CheckStatus::Fail => (Color32::from_rgb(255, 70, 70), "[FAIL]"),
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
                            format!("  ↳ Solución: {}", remedy),
                        );
                    }
                });
                ui.add_space(4.0);
            }
        });
    }
}
