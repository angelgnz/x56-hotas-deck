//! Pestaña del editor interactivo de curvas de respuesta y visualización en tiempo real.

use eframe::egui::{self, Color32, Ui};
use egui_plot::{Line, Plot, PlotPoints, Points};
use x56_core::{axis_name, AxisConfig, DeviceKind, X56Device, X56Manager};
use x56_proto::CurveProfile;

use crate::curve_math::{CurveParameters, LiveInputPoller};

pub struct CurvesTab {
    pub selected_device: DeviceKind,
    pub selected_axis_id: u8,
    pub params: CurveParameters,
    pub hall_zero: u16,
    pub status_message: Option<(String, bool)>, // (mensaje, es_error)
}

impl Default for CurvesTab {
    fn default() -> Self {
        let mut tab = Self {
            selected_device: DeviceKind::Stick,
            selected_axis_id: 0x30, // X Roll
            params: CurveParameters::default(),
            hall_zero: 0,
            status_message: None,
        };
        tab.read_from_hardware();
        tab
    }
}

impl CurvesTab {
    pub fn read_from_hardware(&mut self) {
        if let Ok(cfg) = X56Manager::get_axis_config(
            self.selected_device,
            &format!("{:#04x}", self.selected_axis_id),
        ) {
            self.params.xsat = cfg.xsat;
            self.params.ysat = cfg.ysat;
            self.params.deadband = cfg.deadband;
            self.params.curvature = cfg.curvature;
            self.params.profile = cfg.profile;
            self.hall_zero = cfg.hall_zero;
            self.status_message = Some(("Configuración leída del hardware.".into(), false));
        } else {
            self.status_message = Some((
                format!(
                    "No se pudo leer del {:?} (¿dispositivo desconectado?).",
                    self.selected_device
                ),
                true,
            ));
        }
    }

    pub fn apply_to_hardware(&mut self) {
        let Ok(cfg) = AxisConfig::new(
            self.selected_axis_id,
            self.params.xsat,
            self.params.ysat,
            self.params.deadband,
            self.params.curvature,
            self.params.profile,
            self.hall_zero,
        ) else {
            self.status_message = Some(("Parámetros fuera de rango permitido.".into(), true));
            return;
        };

        if let Ok(dev) = X56Device::open(self.selected_device) {
            match dev.set_axis_config(&cfg) {
                Ok(_) => {
                    self.status_message = Some((
                        format!(
                            "Ajustes aplicados con éxito para {} ({:?}).",
                            axis_name(self.selected_axis_id, self.selected_device),
                            self.selected_device
                        ),
                        false,
                    ));
                }
                Err(err) => {
                    self.status_message = Some((format!("Error aplicando al hardware: {err}"), true));
                }
            }
        } else {
            self.status_message = Some((
                format!("Error abriendo {:?} en Interfaz 2.", self.selected_device),
                true,
            ));
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, poller: &LiveInputPoller) {
        ui.horizontal(|ui| {
            ui.label("Dispositivo:");
            let stick_selected = self.selected_device == DeviceKind::Stick;
            if ui
                .selectable_label(stick_selected, "🕹️ Palanca (Stick)")
                .clicked()
                && !stick_selected
            {
                self.selected_device = DeviceKind::Stick;
                self.selected_axis_id = 0x30;
                self.read_from_hardware();
            }

            let throttle_selected = self.selected_device == DeviceKind::Throttle;
            if ui
                .selectable_label(throttle_selected, "🚀 Gases (Throttle)")
                .clicked()
                && !throttle_selected
            {
                self.selected_device = DeviceKind::Throttle;
                self.selected_axis_id = 0x30;
                self.read_from_hardware();
            }

            ui.separator();

            ui.label("Eje:");
            let axes: &[(u8, &str)] = match self.selected_device {
                DeviceKind::Stick => &[
                    (0x30, "X (Roll / Alabeo)"),
                    (0x31, "Y (Pitch / Cabeceo)"),
                    (0x35, "Z (Twist / Guiñada)"),
                ],
                DeviceKind::Throttle => &[
                    (0x30, "Throttle Left"),
                    (0x31, "Throttle Right"),
                    (0x32, "Rotary 1"),
                    (0x35, "Rotary 2"),
                    (0x36, "Rotary 4"),
                    (0x37, "Rotary 3"),
                ],
            };

            for &(id, label) in axes {
                let is_sel = self.selected_axis_id == id;
                if ui.selectable_label(is_sel, label).clicked() && !is_sel {
                    self.selected_axis_id = id;
                    self.read_from_hardware();
                }
            }
        });

        ui.separator();

        if let Some((ref msg, is_err)) = self.status_message {
            let color = if is_err {
                Color32::from_rgb(255, 90, 90)
            } else {
                Color32::from_rgb(100, 230, 100)
            };
            ui.colored_label(color, msg);
        }

        ui.columns(2, |cols| {
            // Columna izquierda: Controles y sliders
            cols[0].vertical(|ui| {
                ui.group(|ui| {
                    ui.heading("Parámetros de Respuesta");

                    ui.add_space(6.0);
                    ui.label("Curvatura (Curvature):");
                    ui.add(
                        egui::Slider::new(&mut self.params.curvature, 0..=1000)
                            .text("/ 1000 (500 = Lineal)"),
                    );

                    ui.add_space(4.0);
                    ui.label("Zona Muerta (Deadband):");
                    ui.add(
                        egui::Slider::new(&mut self.params.deadband, 0..=1000)
                            .text("/ 1000"),
                    );

                    ui.add_space(4.0);
                    ui.label("Saturación X (Input Max):");
                    ui.add(
                        egui::Slider::new(&mut self.params.xsat, 0..=1000)
                            .text("/ 1000"),
                    );

                    ui.add_space(4.0);
                    ui.label("Saturación Y (Output Max):");
                    ui.add(
                        egui::Slider::new(&mut self.params.ysat, 0..=1000)
                            .text("/ 1000"),
                    );

                    ui.add_space(8.0);
                    ui.label("Perfil de Curva:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.params.profile, CurveProfile::S, "S (Simétrica)");
                        ui.radio_value(&mut self.params.profile, CurveProfile::J, "J (Progresiva)");
                    });

                    if self.hall_zero > 0 {
                        ui.add_space(4.0);
                        ui.label(format!("Calibración Hall EEPROM: {:#06x} ({})", self.hall_zero, self.hall_zero));
                    }
                });

                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label("Presets Rápidos:");
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Lineal 1:1").clicked() {
                            self.params.curvature = 500;
                            self.params.deadband = 0;
                            self.params.xsat = 1000;
                            self.params.ysat = 1000;
                            self.params.profile = CurveProfile::S;
                        }
                        if ui.button("Combate Fino").clicked() {
                            self.params.curvature = 600;
                            self.params.deadband = 30;
                            self.params.xsat = 1000;
                            self.params.ysat = 1000;
                            self.params.profile = CurveProfile::S;
                        }
                        if ui.button("Gases Progresivos").clicked() {
                            self.params.curvature = 480;
                            self.params.deadband = 0;
                            self.params.xsat = 1000;
                            self.params.ysat = 1000;
                            self.params.profile = CurveProfile::J;
                        }
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Aplicar al Hardware").clicked() {
                        self.apply_to_hardware();
                    }
                    if ui.button("🔄 Releer del Hardware").clicked() {
                        self.read_from_hardware();
                    }
                });
            });

            // Columna derecha: Gráfico interactivo egui_plot
            cols[1].vertical(|ui| {
                ui.heading("Gráfico de Respuesta Entrada / Salida");

                let is_bipolar = match self.selected_device {
                    DeviceKind::Stick => true,
                    DeviceKind::Throttle => {
                        // Left y Right son unidireccionales 0..1, rotatorios bipolares
                        !matches!(self.selected_axis_id, 0x30 | 0x31)
                    }
                };

                // Puntos de la curva configurada
                let curve_pts = self.params.generate_plot_points(is_bipolar, 120);
                let plot_line = Line::new("Respuesta Configurada", PlotPoints::new(curve_pts))
                    .color(Color32::from_rgb(0, 230, 255))
                    .width(2.5);

                // Línea de referencia 1:1
                let ref_pts = if is_bipolar {
                    vec![[-1.0, -1.0], [1.0, 1.0]]
                } else {
                    vec![[0.0, 0.0], [1.0, 1.0]]
                };
                let ref_line = Line::new("Referencia 1:1", PlotPoints::new(ref_pts))
                    .color(Color32::from_rgba_unmultiplied(180, 180, 180, 90))
                    .style(egui_plot::LineStyle::Dashed { length: 8.0 });

                // Lectura en vivo del stick para dibujar el punto físico actual
                let live_state = poller.get_state();
                let live_in = match (self.selected_device, self.selected_axis_id) {
                    (DeviceKind::Stick, 0x30) => live_state.stick_x,
                    (DeviceKind::Stick, 0x31) => live_state.stick_y,
                    (DeviceKind::Stick, 0x35) => live_state.stick_z,
                    _ => 0.0,
                };
                let live_out = self.params.evaluate(live_in);

                let live_dot = Points::new("Entrada Física en Vivo", PlotPoints::new(vec![[live_in, live_out]]))
                    .color(Color32::from_rgb(255, 60, 60))
                    .radius(7.0);

                let plot = Plot::new("curve_plot")
                    .data_aspect(1.0)
                    .allow_zoom(false)
                    .allow_drag(false)
                    .show_grid(true)
                    .include_x(if is_bipolar { -1.05 } else { -0.05 })
                    .include_x(1.05)
                    .include_y(if is_bipolar { -1.05 } else { -0.05 })
                    .include_y(1.05);

                plot.show(ui, |plot_ui| {
                    plot_ui.line(ref_line);
                    plot_ui.line(plot_line);
                    plot_ui.points(live_dot);
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(format!("Entrada física (Raw) : {:+.3}", live_in));
                    ui.separator();
                    ui.label(format!("Salida filtrada (Curved): {:+.3}", live_out));
                });
            });
        });
    }
}
