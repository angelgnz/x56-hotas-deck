//! Pestaña del asistente de calibración de sensores de efecto Hall.

use eframe::egui::{self, Color32, Ui};
use x56_core::{Calibrator, HallCalibrationResult};

pub struct CalibrateTab {
    pub samples: usize,
    pub dry_run: bool,
    pub results: Option<Vec<HallCalibrationResult>>,
    pub is_sampling: bool,
    pub status_message: Option<(String, bool)>,
}

impl Default for CalibrateTab {
    fn default() -> Self {
        Self {
            samples: 60,
            dry_run: true,
            results: None,
            is_sampling: false,
            status_message: None,
        }
    }
}

impl CalibrateTab {
    pub fn execute_calibration(&mut self, dry_run: bool) {
        self.is_sampling = true;
        match Calibrator::calibrate_stick(self.samples, dry_run) {
            Ok(res) => {
                let msg = if dry_run {
                    "Muestreo completado en modo simulación (ningún dato escrito en EEPROM).".into()
                } else {
                    "¡Calibración física guardada exitosamente en la EEPROM del Stick!".into()
                };
                self.status_message = Some((msg, false));
                self.results = Some(res);
            }
            Err(err) => {
                self.status_message = Some((format!("Error durante la calibración: {err}"), true));
            }
        }
        self.is_sampling = false;
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.heading("Asistente de Calibración de Sensores de Efecto Hall");
        ui.label(
            "Recalibra el punto cero magnético de los ejes X (Roll) e Y (Pitch) de la palanca.",
        );

        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label("⚠️ Instrucciones de Seguridad:");
            ui.label(
                "1. Suelta la palanca por completo y permite que el resorte la centre libremente.",
            );
            ui.label("2. No toques la base ni el vástago durante los 2 segundos de muestreo.");
            ui.label(
                "3. Primero realiza un muestreo en modo simulación para revisar las desviaciones.",
            );
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label("Número de muestras:");
            ui.add(egui::Slider::new(&mut self.samples, 20..=200).text("paquetes"));
            ui.separator();
            ui.checkbox(&mut self.dry_run, "Modo Simulación (Dry-Run)");
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("🔬 Muestrear Posición Central").clicked() {
                self.execute_calibration(true);
            }

            let apply_btn = ui.add_enabled(
                !self.dry_run,
                egui::Button::new("💾 Grabar Calibración en la EEPROM"),
            );
            if apply_btn.clicked() {
                self.execute_calibration(false);
            }
        });

        ui.add_space(10.0);

        if let Some((ref msg, is_err)) = self.status_message {
            let col = if is_err {
                Color32::from_rgb(255, 90, 90)
            } else {
                Color32::from_rgb(100, 230, 100)
            };
            ui.colored_label(col, msg);
            ui.add_space(6.0);
        }

        if let Some(ref res) = self.results {
            ui.group(|ui| {
                ui.heading("Resultados del Muestreo de Centro:");
                ui.add_space(6.0);

                for r in res {
                    ui.label(format!("• {}:", r.axis_name));
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "  Punto cero anterior: {:#06x} ({})",
                            r.old_hall_zero, r.old_hall_zero
                        ));
                        ui.separator();
                        ui.label(format!(
                            "  Punto cero muestreado: {:#06x} ({})",
                            r.sampled_hall_zero, r.sampled_hall_zero
                        ));
                    });

                    let drift_color = if r.delta.abs() < 500 {
                        Color32::from_rgb(100, 230, 100) // Verde: excelente
                    } else if r.delta.abs() < 2500 {
                        Color32::from_rgb(255, 200, 50) // Amarillo: desvío moderado
                    } else {
                        Color32::from_rgb(255, 100, 100) // Rojo: desvío notable
                    };

                    ui.colored_label(
                        drift_color,
                        format!(
                            "  Ajuste necesario (Delta): {:+} | Estado: {}",
                            r.delta,
                            if r.applied {
                                "APLICADO"
                            } else {
                                "PENDIENTE (dry-run)"
                            }
                        ),
                    );
                    ui.add_space(4.0);
                }
            });
        }
    }
}
