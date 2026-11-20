//! Hall-effect sensor neutral calibration wizard tab.

use eframe::egui::{self, RichText, Ui};
use x56_core::{Calibrator, HallCalibrationResult};

use crate::icons;
use crate::theme;

pub struct CalibrateTab {
    pub samples: usize,
    pub dry_run: bool,
    pub results: Option<Vec<HallCalibrationResult>>,
    pub status_message: Option<(String, bool)>,
}

impl Default for CalibrateTab {
    fn default() -> Self {
        Self {
            samples: 60,
            dry_run: true,
            results: None,
            status_message: None,
        }
    }
}

impl CalibrateTab {
    pub fn execute_calibration(&mut self, dry_run: bool) {
        match Calibrator::calibrate_stick(self.samples, dry_run) {
            Ok(res) => {
                let msg = if dry_run {
                    x56_core::t!("cal-sim-complete")
                } else {
                    x56_core::t!("cal-success-eeprom")
                };
                self.status_message = Some((msg, false));
                self.results = Some(res);
            }
            Err(err) => {
                let err_str = err.to_string();
                self.status_message = Some((x56_core::t!("cal-error", "err" => &err_str), true));
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.set_min_width(ui.available_width());

        theme::section_heading(ui, &x56_core::t!("cal-title"));
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(x56_core::t!("cal-subtitle"))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );

        ui.add_space(theme::SPACE_MD);

        theme::subcard_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icons::ALERT_TRIANGLE)
                        .color(theme::COLOR_WARNING)
                        .size(theme::FONT_BODY),
                );
                ui.label(
                    RichText::new(x56_core::t!("cal-instructions"))
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_BODY),
                );
            });

            ui.add_space(theme::SPACE_XS);

            let ms = (self.samples as u64 * 2).to_string();
            for step in [
                x56_core::t!("cal-step1"),
                x56_core::t!("cal-step2", "ms" => &ms),
                x56_core::t!("cal-step3"),
            ] {
                ui.label(
                    RichText::new(step)
                        .color(theme::COLOR_TEXT_SECONDARY)
                        .size(theme::FONT_SMALL),
                );
            }
        });

        ui.add_space(theme::SPACE_MD);

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(x56_core::t!("cal-samples-label"))
                    .color(theme::COLOR_TEXT_SECONDARY)
                    .size(theme::FONT_SMALL),
            );
            let unit = x56_core::t!("cal-samples-unit");
            ui.add(egui::Slider::new(&mut self.samples, 20..=200).text(unit));

            ui.add_space(theme::SPACE_MD);

            ui.checkbox(
                &mut self.dry_run,
                RichText::new(x56_core::t!("cal-dry-run"))
                    .color(theme::COLOR_TEXT_SECONDARY)
                    .size(theme::FONT_SMALL),
            );
        });

        ui.add_space(theme::SPACE_MD);

        ui.horizontal_wrapped(|ui| {
            if theme::secondary_button(ui, &x56_core::t!("cal-sample-btn")).clicked() {
                self.execute_calibration(true);
            }

            ui.add_enabled_ui(!self.dry_run, |ui| {
                if theme::primary_button(ui, &x56_core::t!("cal-write-btn")).clicked() {
                    self.execute_calibration(false);
                }
            });
        });

        if let Some((ref msg, is_err)) = self.status_message {
            ui.add_space(theme::SPACE_MD);
            let col = if is_err {
                theme::COLOR_NEGATIVE
            } else {
                theme::COLOR_POSITIVE
            };
            ui.horizontal(|ui| {
                theme::status_dot(ui, col, 8.0);
                ui.label(RichText::new(msg).color(col).size(theme::FONT_SMALL));
            });
        }

        if let Some(ref res) = self.results {
            ui.add_space(theme::SPACE_MD);
            theme::subcard_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                theme::section_heading(ui, &x56_core::t!("cal-results-heading"));
                ui.add_space(theme::SPACE_SM);

                for (idx, r) in res.iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(theme::SPACE_SM);
                    }
                    ui.label(
                        RichText::new(&r.axis_name)
                            .color(theme::COLOR_TEXT_PRIMARY)
                            .size(theme::FONT_BODY),
                    );
                    ui.add_space(theme::SPACE_XS);

                    let prev_hex = format!("0x{:04X}", r.old_hall_zero);
                    let prev_dec = r.old_hall_zero.to_string();
                    let sampled_hex = format!("0x{:04X}", r.sampled_hall_zero);
                    let sampled_dec = r.sampled_hall_zero.to_string();

                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(x56_core::t!(
                                "cal-prev-zero",
                                "hex" => &prev_hex,
                                "dec" => &prev_dec
                            ))
                            .color(theme::COLOR_TEXT_SECONDARY)
                            .size(theme::FONT_META)
                            .monospace(),
                        );
                        ui.add_space(theme::SPACE_MD);
                        ui.label(
                            RichText::new(x56_core::t!(
                                "cal-sampled-zero",
                                "hex" => &sampled_hex,
                                "dec" => &sampled_dec
                            ))
                            .color(theme::COLOR_TEXT_SECONDARY)
                            .size(theme::FONT_META)
                            .monospace(),
                        );
                    });

                    let drift_color = if r.delta.abs() < 500 {
                        theme::COLOR_POSITIVE
                    } else if r.delta.abs() < 2500 {
                        theme::COLOR_WARNING
                    } else {
                        theme::COLOR_NEGATIVE
                    };

                    let delta_str = format!("{:+}", r.delta);
                    let status_str = if r.applied {
                        x56_core::t!("cal-status-applied")
                    } else {
                        x56_core::t!("cal-status-pending")
                    };

                    ui.label(
                        RichText::new(x56_core::t!(
                            "cal-delta-status",
                            "delta" => &delta_str,
                            "status" => &status_str
                        ))
                        .color(drift_color)
                        .size(theme::FONT_META)
                        .monospace(),
                    );
                }
            });
        }
    }
}
