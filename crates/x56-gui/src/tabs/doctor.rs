//! System diagnostics and permissions integrity tab (`x56 doctor`).

use eframe::egui::{self, RichText, Ui};
use x56_core::{CheckStatus, Doctor, DoctorReport};

use crate::theme;

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
        ui.set_min_width(ui.available_width());

        ui.horizontal(|ui| {
            theme::section_heading(ui, &x56_core::t!("doc-title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::secondary_button(ui, &x56_core::t!("doc-refresh")).clicked() {
                    self.refresh();
                }
            });
        });

        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(x56_core::t!("doc-subtitle"))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );

        ui.add_space(theme::SPACE_MD);

        let (status_color, status_text) = if self.report.is_healthy() {
            (theme::COLOR_POSITIVE, x56_core::t!("doc-status-ok"))
        } else if self.report.fail_count > 0 {
            (theme::COLOR_NEGATIVE, x56_core::t!("doc-status-fail"))
        } else {
            (theme::COLOR_WARNING, x56_core::t!("doc-status-warn"))
        };

        let ok_str = self.report.ok_count.to_string();
        let warn_str = self.report.warn_count.to_string();
        let fail_str = self.report.fail_count.to_string();
        let summary_text =
            x56_core::t!("doc-summary", "ok" => &ok_str, "warn" => &warn_str, "fail" => &fail_str);

        ui.horizontal_wrapped(|ui| {
            theme::status_dot(ui, status_color, 8.0);
            ui.label(
                RichText::new(status_text)
                    .color(theme::COLOR_TEXT_PRIMARY)
                    .size(theme::FONT_SMALL),
            );
            ui.label(
                RichText::new(summary_text)
                    .color(theme::COLOR_TEXT_MUTED)
                    .size(theme::FONT_META)
                    .monospace(),
            );
        });

        ui.add_space(theme::SPACE_MD);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            for check in &self.report.checks {
                theme::subcard_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());

                    let (dot_col, tag) = match check.status {
                        CheckStatus::Ok => (theme::COLOR_POSITIVE, x56_core::t!("doc-pass")),
                        CheckStatus::Warn => (theme::COLOR_WARNING, x56_core::t!("doc-warn")),
                        CheckStatus::Fail => (theme::COLOR_NEGATIVE, x56_core::t!("doc-fail")),
                    };

                    ui.horizontal(|ui| {
                        theme::status_dot(ui, dot_col, 8.0);
                        ui.label(
                            RichText::new(tag)
                                .color(dot_col)
                                .size(theme::FONT_META)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(format!("{} · {}", check.category, check.title))
                                .color(theme::COLOR_TEXT_PRIMARY)
                                .size(theme::FONT_BODY),
                        );
                    });

                    ui.add_space(theme::SPACE_XS);
                    ui.indent(("doc_msg", &check.category, &check.title), |ui| {
                        ui.label(
                            RichText::new(&check.message)
                                .color(theme::COLOR_TEXT_SECONDARY)
                                .size(theme::FONT_SMALL),
                        );

                        if let Some(ref remedy) = check.remedy {
                            ui.add_space(theme::SPACE_XS);
                            ui.label(
                                RichText::new(x56_core::t!("doc-remedy", "remedy" => remedy))
                                    .color(theme::COLOR_WARNING)
                                    .size(theme::FONT_SMALL),
                            );
                        }
                    });
                });
                ui.add_space(theme::SPACE_SM);
            }
        });
    }
}
