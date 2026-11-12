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
        ui.heading(x56_core::t!("doc-title"));
        ui.label(x56_core::t!("doc-subtitle"));

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let refresh_lbl = format!("{} {}", icons::REFRESH_CW, x56_core::t!("doc-refresh"));
            if ui.button(refresh_lbl).clicked() {
                self.refresh();
            }

            let status_badge = if self.report.is_healthy() {
                (
                    Color32::from_rgb(40, 160, 40),
                    format!("{} {}", icons::CHECK, x56_core::t!("doc-status-ok")),
                )
            } else if self.report.fail_count > 0 {
                (
                    Color32::from_rgb(200, 40, 40),
                    format!("{} {}", icons::X, x56_core::t!("doc-status-fail")),
                )
            } else {
                (
                    Color32::from_rgb(200, 150, 20),
                    format!("{} {}", icons::ALERT_TRIANGLE, x56_core::t!("doc-status-warn")),
                )
            };

            ui.colored_label(status_badge.0, status_badge.1);
            let ok_str = self.report.ok_count.to_string();
            let warn_str = self.report.warn_count.to_string();
            let fail_str = self.report.fail_count.to_string();
            ui.label(x56_core::t!("doc-summary", "ok" => &ok_str, "warn" => &warn_str, "fail" => &fail_str));
        });

        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for check in &self.report.checks {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let (badge_col, tag) = match check.status {
                            CheckStatus::Ok => (
                                Color32::from_rgb(80, 220, 80),
                                format!("{} {}", icons::CHECK, x56_core::t!("doc-pass")),
                            ),
                            CheckStatus::Warn => (
                                Color32::from_rgb(255, 200, 50),
                                format!("{} {}", icons::ALERT_TRIANGLE, x56_core::t!("doc-warn")),
                            ),
                            CheckStatus::Fail => (
                                Color32::from_rgb(255, 70, 70),
                                format!("{} {}", icons::X, x56_core::t!("doc-fail")),
                            ),
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
