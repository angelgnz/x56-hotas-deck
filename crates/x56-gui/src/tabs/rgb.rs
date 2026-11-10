//! Pestaña de control interactivo de iluminación LED RGB y preajustes.

use eframe::egui::{self, Color32, Ui};
use x56_core::{RgbColor, TargetDevice, X56Manager};

pub struct RgbTab {
    pub target: TargetDevice,
    pub color_rgba: [f32; 3], // 0.0 .. 1.0 para egui color picker
    pub status_message: Option<(String, bool)>,
}

impl Default for RgbTab {
    fn default() -> Self {
        Self {
            target: TargetDevice::All,
            color_rgba: [0.0, 1.0, 1.0], // Cyan de fábrica
            status_message: None,
        }
    }
}

impl RgbTab {
    pub fn current_rgb_color(&self) -> RgbColor {
        RgbColor::new(
            (self.color_rgba[0] * 255.0).round() as u8,
            (self.color_rgba[1] * 255.0).round() as u8,
            (self.color_rgba[2] * 255.0).round() as u8,
        )
    }

    pub fn apply_color(&mut self) {
        let color = self.current_rgb_color();
        let target = self.target;

        match X56Manager::set_rgb(target, color) {
            Ok(count) => {
                let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
                let target_desc = match target {
                    TargetDevice::All => "Ambos",
                    TargetDevice::Stick => "Stick",
                    TargetDevice::Throttle => "Throttle",
                };
                self.status_message = Some((
                    format!("Color RGB {hex} aplicado en {count} dispositivo(s) [{target_desc}]."),
                    false,
                ));
            }
            Err(err) => {
                self.status_message = Some((format!("Error aplicando color RGB: {err}"), true));
            }
        }
    }

    pub fn set_preset(&mut self, r: u8, g: u8, b: u8) {
        self.color_rgba = [(r as f32) / 255.0, (g as f32) / 255.0, (b as f32) / 255.0];
        self.apply_color();
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.heading("Control de Iluminación LED RGB");
        ui.label("Configura el color y brillo de la retroiluminación del Stick y Throttle.");

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Dispositivo destino:");
            ui.radio_value(
                &mut self.target,
                TargetDevice::All,
                "Ambos (Stick + Throttle)",
            );
            ui.radio_value(&mut self.target, TargetDevice::Stick, "Solo Stick");
            ui.radio_value(&mut self.target, TargetDevice::Throttle, "Solo Throttle");
        });

        ui.add_space(8.0);

        if let Some((ref msg, is_err)) = self.status_message {
            let col = if is_err {
                Color32::from_rgb(255, 90, 90)
            } else {
                Color32::from_rgb(100, 230, 100)
            };
            ui.colored_label(col, msg);
            ui.add_space(4.0);
        }

        ui.columns(2, |cols| {
            // Columna 1: Selector de color interactivo
            cols[0].group(|ui| {
                ui.heading("Selector de Color");
                ui.add_space(6.0);

                let mut col32 = Color32::from_rgb(
                    (self.color_rgba[0] * 255.0) as u8,
                    (self.color_rgba[1] * 255.0) as u8,
                    (self.color_rgba[2] * 255.0) as u8,
                );

                ui.color_edit_button_srgba(&mut col32);
                self.color_rgba = [
                    (col32.r() as f32) / 255.0,
                    (col32.g() as f32) / 255.0,
                    (col32.b() as f32) / 255.0,
                ];

                ui.add_space(8.0);
                let rgb = self.current_rgb_color();
                ui.label(format!("Rojo  (R): {:3}", rgb.r));
                ui.add(egui::Slider::new(&mut self.color_rgba[0], 0.0..=1.0).text("R"));
                ui.label(format!("Verde (G): {:3}", rgb.g));
                ui.add(egui::Slider::new(&mut self.color_rgba[1], 0.0..=1.0).text("G"));
                ui.label(format!("Azul  (B): {:3}", rgb.b));
                ui.add(egui::Slider::new(&mut self.color_rgba[2], 0.0..=1.0).text("B"));

                ui.add_space(8.0);
                ui.label(format!(
                    "Código Hex: #{:02x}{:02x}{:02x}",
                    rgb.r, rgb.g, rgb.b
                ));

                ui.add_space(10.0);
                if ui.button("💡 Aplicar Color al Hardware").clicked() {
                    self.apply_color();
                }
            });

            // Columna 2: Paleta de Preajustes
            cols[1].group(|ui| {
                ui.heading("Preajustes Tácticos");
                ui.label("Haz clic en cualquier preajuste para aplicarlo de inmediato:");

                ui.add_space(8.0);

                let presets = [
                    (
                        "Cyan Rhino (Fábrica)",
                        0x00,
                        0xff,
                        0xff,
                        Color32::from_rgb(0, 255, 255),
                    ),
                    (
                        "Ámbar Táctico / Naranja",
                        0xff,
                        0x66,
                        0x00,
                        Color32::from_rgb(255, 102, 0),
                    ),
                    (
                        "Rojo Combate Nocturno",
                        0xff,
                        0x00,
                        0x00,
                        Color32::from_rgb(255, 0, 0),
                    ),
                    (
                        "Verde Esmeralda",
                        0x00,
                        0xff,
                        0x44,
                        Color32::from_rgb(0, 255, 68),
                    ),
                    (
                        "Azul Espacial Profundo",
                        0x00,
                        0x44,
                        0xff,
                        Color32::from_rgb(0, 68, 255),
                    ),
                    (
                        "Magenta Neón",
                        0xff,
                        0x00,
                        0xbb,
                        Color32::from_rgb(255, 0, 187),
                    ),
                    (
                        "Blanco Puro",
                        0xff,
                        0xff,
                        0xff,
                        Color32::from_rgb(255, 255, 255),
                    ),
                    (
                        "Apagar Iluminación (Off)",
                        0x00,
                        0x00,
                        0x00,
                        Color32::from_rgb(40, 40, 40),
                    ),
                ];

                for (name, r, g, b, preview_col) in presets {
                    ui.horizontal(|ui| {
                        let (rect, _response) =
                            ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, preview_col);
                        if ui.button(name).clicked() {
                            self.set_preset(r, g, b);
                        }
                    });
                    ui.add_space(3.0);
                }
            });
        });
    }
}
