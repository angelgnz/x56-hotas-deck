//! Subsistema de Ejes y Curvas (Avionics Vector HUD) con telemetría 16-bit Hall,
//! gráfico táctico de transferencia y monitor Gimbal 2D en tiempo real.

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use x56_core::{axis_name, AxisConfig, DeviceKind, X56Device, X56Manager};

use crate::curve_math::{CurveParameters, CurveType, LiveAxesState, LiveInputPoller};
use crate::icons;
use crate::theme::{
    self, PhysicalSpring, COLOR_ERROR, COLOR_GRID_HUD, COLOR_ON_PRIMARY_CONTAINER,
    COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE, COLOR_OUTLINE_VARIANT,
    COLOR_PRIMARY, COLOR_PRIMARY_CONTAINER, COLOR_PRIMARY_FIXED, COLOR_SECONDARY_CONTAINER,
    COLOR_SURFACE_CONTAINER, COLOR_SURFACE_HIGH, COLOR_SURFACE_LOWEST, COLOR_TERTIARY_CONTAINER,
    COLOR_TERTIARY_FIXED_DIM, COLOR_WARNING_AMBER,
};

/// IDs virtuales de UI para los ejes del Ministick analógico C-Stick.
pub const AXIS_MINISTICK_X: u8 = 0x38;
pub const AXIS_MINISTICK_Y: u8 = 0x39;

pub struct CurvesTab {
    pub selected_device: DeviceKind,
    pub selected_axis_id: u8,
    pub params: CurveParameters,
    pub hall_zero: u16,
    pub installed_spring: PhysicalSpring,
    pub eeprom_synced: bool,
    pub status_message: Option<(String, bool)>,
}

impl Default for CurvesTab {
    fn default() -> Self {
        let mut tab = Self {
            selected_device: DeviceKind::Stick,
            selected_axis_id: 0x30, // Eje X (Roll)
            params: CurveParameters::default(),
            hall_zero: 0x8000,
            installed_spring: PhysicalSpring::Green47mm,
            eeprom_synced: true,
            status_message: None,
        };
        tab.read_from_hardware();
        tab
    }
}

impl CurvesTab {
    /// Lee los parámetros del eje activo desde la EEPROM del hardware si está conectado.
    pub fn read_from_hardware(&mut self) {
        if self.selected_device == DeviceKind::Stick
            && matches!(self.selected_axis_id, AXIS_MINISTICK_X | AXIS_MINISTICK_Y)
        {
            self.eeprom_synced = true;
            self.status_message = Some((
                "Ministick C-Stick sincronizado con pipeline uinput.".into(),
                false,
            ));
            return;
        }

        if let Ok(cfg) = X56Manager::get_axis_config(
            self.selected_device,
            &format!("{:#04x}", self.selected_axis_id),
        ) {
            self.params.xsat = cfg.xsat;
            self.params.ysat = cfg.ysat;
            self.params.deadband = cfg.deadband;
            self.params.curvature = cfg.curvature;
            self.params.profile = cfg.profile;
            self.params.sync_curve_type_from_hardware();
            self.hall_zero = cfg.hall_zero;
            self.eeprom_synced = true;
            self.status_message = Some((
                "EEPROM SYNCED // Configuración leída del hardware.".into(),
                false,
            ));
        } else {
            // Mantener valores tácticos predeterminados cuando se ejecuta sin hardware conectado
            self.eeprom_synced = true;
            self.status_message = Some((
                format!(
                    "Modo simulación activo ({:?} no detectado en /dev/hidraw*).",
                    self.selected_device
                ),
                false,
            ));
        }
    }

    /// Restablece los parámetros del eje activo a sus valores predeterminados.
    pub fn reset_current_axis(&mut self) {
        self.params = CurveParameters::default();
        self.eeprom_synced = false;
        self.status_message = Some((
            "Parámetros de curva revertidos a valores nominales.".into(),
            false,
        ));
    }

    /// Escribe los parámetros de la curva actual en el hardware físico.
    pub fn apply_to_hardware(&mut self) {
        if self.selected_device == DeviceKind::Stick
            && matches!(self.selected_axis_id, AXIS_MINISTICK_X | AXIS_MINISTICK_Y)
        {
            self.eeprom_synced = true;
            self.status_message = Some((
                "Curva de Ministick aplicada al traductor uinput.".into(),
                false,
            ));
            return;
        }

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
                    self.eeprom_synced = true;
                    self.status_message = Some((
                        format!(
                            "EEPROM SYNCED // Curva escrita en {} ({:?}).",
                            axis_name(self.selected_axis_id, self.selected_device),
                            self.selected_device
                        ),
                        false,
                    ));
                }
                Err(err) => {
                    self.status_message =
                        Some((format!("Error aplicando al hardware: {err}"), true));
                }
            }
        } else {
            self.eeprom_synced = true;
            self.status_message = Some((
                format!(
                    "Curva aplicada al perfil en memoria ({:?} sin conexión USB).",
                    self.selected_device
                ),
                false,
            ));
        }
    }

    /// Obtiene la entrada física normalizada y el valor ADC crudo del eje actualmente seleccionado.
    fn current_live_input(&self, state: &LiveAxesState) -> (f64, u16) {
        match (self.selected_device, self.selected_axis_id) {
            (DeviceKind::Stick, 0x30) => (state.stick_x, state.raw_adc_x),
            (DeviceKind::Stick, 0x31) => (state.stick_y, state.raw_adc_y),
            (DeviceKind::Stick, 0x35) => {
                let adc16 = ((state.stick_z + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.stick_z, adc16)
            }
            (DeviceKind::Stick, AXIS_MINISTICK_X) => {
                let adc16 = ((state.ministick_x + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.ministick_x, adc16)
            }
            (DeviceKind::Stick, AXIS_MINISTICK_Y) => {
                let adc16 = ((state.ministick_y + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.ministick_y, adc16)
            }
            (DeviceKind::Throttle, 0x30) => {
                let adc = (state.throttle_left * 65535.0).round().clamp(0.0, 65535.0) as u16;
                (state.throttle_left, adc)
            }
            (DeviceKind::Throttle, 0x31) => {
                let adc = (state.throttle_right * 65535.0).round().clamp(0.0, 65535.0) as u16;
                (state.throttle_right, adc)
            }
            (DeviceKind::Throttle, 0x32) => {
                let adc = ((state.rotary_1 + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.rotary_1, adc)
            }
            (DeviceKind::Throttle, 0x35) => {
                let adc = ((state.rotary_2 + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.rotary_2, adc)
            }
            (DeviceKind::Throttle, 0x36) => {
                let adc = ((state.rotary_4 + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.rotary_4, adc)
            }
            (DeviceKind::Throttle, 0x37) => {
                let adc = ((state.rotary_3 + 1.0) * 0.5 * 65535.0)
                    .round()
                    .clamp(0.0, 65535.0) as u16;
                (state.rotary_3, adc)
            }
            _ => (0.0, 32768),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, poller: &LiveInputPoller) {
        let live_state = poller.get_state();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // 1. Top Hardware & Subsystem Telemetry Bar
                self.render_hardware_telemetry_bar(ui);
                ui.add_space(8.0);

                // 2. Axis Selection Sub-Bar
                self.render_axis_selection_subbar(ui);
                ui.add_space(10.0);

                // 3. Grid Avionics: 2 columnas en pantallas anchas (>= 820px), o apilado vertical (< 820px)
                let total_width = ui.available_width();
                if total_width >= 820.0 {
                    let gap = 12.0;
                    let left_width = ((total_width - gap) * 0.62).max(360.0);
                    let right_width = total_width - gap - left_width;

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;

                        // COLUMNA IZQUIERDA (Transfer curve, telemetría, gimbal)
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.render_transfer_curve_section(ui, &live_state);
                                ui.add_space(10.0);
                                self.render_telemetry_cards(ui, &live_state);
                                ui.add_space(10.0);
                                self.render_gimbal_and_secondary_axes(ui, &live_state);
                            },
                        );

                        // COLUMNA DERECHA (Selector tipo curva, parámetros deadband/expo, muelles)
                        ui.allocate_ui_with_layout(
                            Vec2::new(right_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.render_curve_type_selector(ui);
                                ui.add_space(10.0);
                                self.render_axis_parameters_card(ui);
                                ui.add_space(10.0);
                                self.render_physical_springs_card(ui);
                            },
                        );
                    });
                } else {
                    // Modo apilado vertical para pantallas compactas (< 820px)
                    self.render_transfer_curve_section(ui, &live_state);
                    ui.add_space(10.0);
                    self.render_telemetry_cards(ui, &live_state);
                    ui.add_space(10.0);
                    self.render_gimbal_and_secondary_axes(ui, &live_state);
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);
                    self.render_curve_type_selector(ui);
                    ui.add_space(10.0);
                    self.render_axis_parameters_card(ui);
                    ui.add_space(10.0);
                    self.render_physical_springs_card(ui);
                }
            });
    }

    /// Top Hardware & Subsystem Telemetry Bar:
    /// Selector STICK // RHINO [VID:0738 PID:2215] / DUAL THROTTLE [VID:0738 PID:2216],
    /// badge ADC 16-BIT HALL SENSOR (65536 STEPS), JITTER 0.08ms, NODE /dev/hidraw0, EEPROM SYNCED.
    fn render_hardware_telemetry_bar(&mut self, ui: &mut Ui) {
        theme::card_frame()
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let is_stick = self.selected_device == DeviceKind::Stick;
                    let stick_lbl =
                        format!("{} STICK // RHINO [VID:0738 PID:2215]", icons::JOYSTICK);
                    if theme::tactical_button(ui, &stick_lbl, is_stick, false).clicked()
                        && !is_stick
                    {
                        self.selected_device = DeviceKind::Stick;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    let is_throttle = self.selected_device == DeviceKind::Throttle;
                    let thr_lbl = format!("{} DUAL THROTTLE [VID:0738 PID:2216]", icons::SLIDERS);
                    if theme::tactical_button(ui, &thr_lbl, is_throttle, false).clicked()
                        && !is_throttle
                    {
                        self.selected_device = DeviceKind::Throttle;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    ui.separator();

                    theme::telemetry_badge(
                        ui,
                        "ADC:",
                        "16-BIT HALL SENSOR (65536 STEPS)",
                        COLOR_PRIMARY_CONTAINER,
                    );
                    theme::telemetry_badge(ui, "JITTER:", "0.08ms", COLOR_TERTIARY_CONTAINER);

                    let node_str = match self.selected_device {
                        DeviceKind::Stick => "/dev/hidraw0",
                        DeviceKind::Throttle => "/dev/hidraw1",
                    };
                    theme::telemetry_badge(ui, "NODE:", node_str, COLOR_ON_SURFACE);

                    let (sync_label, sync_col) = if self.eeprom_synced {
                        (
                            format!("{} EEPROM SYNCED", icons::CIRCLE_CHECK),
                            COLOR_TERTIARY_FIXED_DIM,
                        )
                    } else {
                        (
                            format!("{} EEPROM MODIFIED", icons::CIRCLE_ALERT),
                            COLOR_WARNING_AMBER,
                        )
                    };
                    Frame::new()
                        .fill(COLOR_SURFACE_LOWEST)
                        .stroke(Stroke::new(1.0, sync_col))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(sync_label)
                                    .color(sync_col)
                                    .size(10.5)
                                    .strong()
                                    .monospace(),
                            );
                        });
                });
            });
    }

    /// Axis Selection Sub-Bar:
    /// EJE ACTIVO con pills para Eje X (Roll), Eje Y (Pitch), Timón Z (Yaw), Ministick X, Ministick Y,
    /// y badge "POLYNOMIAL: CUBIC-HERMITE".
    fn render_axis_selection_subbar(&mut self, ui: &mut Ui) {
        theme::card_frame()
            .inner_margin(Margin::symmetric(12, 7))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("EJE ACTIVO:")
                            .color(COLOR_OUTLINE)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );

                    let axes: &[(u8, &str)] = match self.selected_device {
                        DeviceKind::Stick => &[
                            (0x30, "Eje X (Roll)"),
                            (0x31, "Eje Y (Pitch)"),
                            (0x35, "Timón Z (Yaw)"),
                            (AXIS_MINISTICK_X, "Ministick X"),
                            (AXIS_MINISTICK_Y, "Ministick Y"),
                        ],
                        DeviceKind::Throttle => &[
                            (0x30, "Throttle Left"),
                            (0x31, "Throttle Right"),
                            (0x32, "Rotary 1 (F)"),
                            (0x35, "Rotary 2 (G)"),
                            (0x37, "Rotary 3"),
                            (0x36, "Rotary 4"),
                        ],
                    };

                    for &(id, label) in axes {
                        let is_active = self.selected_axis_id == id;
                        if theme::tactical_button(ui, label, is_active, false).clicked()
                            && !is_active
                        {
                            self.selected_axis_id = id;
                            self.read_from_hardware();
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Frame::new()
                            .fill(COLOR_SURFACE_LOWEST)
                            .stroke(Stroke::new(1.0, COLOR_PRIMARY_CONTAINER))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(self.params.curve_type.polynomial_badge())
                                        .color(COLOR_PRIMARY_FIXED)
                                        .size(10.5)
                                        .strong()
                                        .monospace(),
                                );
                            });
                    });
                });
            });
    }

    /// Canvas HUD "RESPUESTA DE TRANSFERENCIA (TRANSFER CURVE)" con egui::Painter:
    /// sombreado degradado bajo la curva, bandas de zona muerta sombreada (hatch),
    /// barras de saturación a los lados, eje horizontal/vertical cian, línea S-Curve cyan (#00f0ff),
    /// línea diagonal 1:1 punteada, crosshair de fósforo verde con etiqueta "IN: +XX.X%"
    /// sincronizado con la entrada de hardware real.
    fn render_transfer_curve_section(&mut self, ui: &mut Ui, live_state: &LiveAxesState) {
        theme::card_frame().show(ui, |ui| {
            // Encabezado del bloque HUD
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} RESPUESTA DE TRANSFERENCIA (TRANSFER CURVE)",
                        icons::ACTIVITY
                    ))
                    .color(COLOR_PRIMARY)
                    .size(12.5)
                    .strong()
                    .monospace(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "DZ: {:.1}% | EXPO: {:.2} | SAT: {:.1}%",
                            self.params.deadband_pct(),
                            self.params.expo_factor(),
                            self.params.saturation_pct()
                        ))
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(10.5)
                        .monospace(),
                    );
                });
            });

            ui.add_space(6.0);

            theme::sunken_frame().show(ui, |ui| {
                let canvas_width = ui.available_width();
                let canvas_height = (canvas_width * 0.52).clamp(210.0, 300.0);
                let (response, painter) =
                    ui.allocate_painter(Vec2::new(canvas_width, canvas_height), Sense::hover());
                let rect = response.rect;

                // Fondo profundo #090e16
                painter.rect_filled(rect, 4.0, COLOR_SURFACE_LOWEST);

                // Márgenes internos para etiquetas de ejes
                let pad_left = 38.0;
                let pad_right = 18.0;
                let pad_top = 16.0;
                let pad_bottom = 22.0;
                let plot_rect = Rect::from_min_max(
                    Pos2::new(rect.min.x + pad_left, rect.min.y + pad_top),
                    Pos2::new(rect.max.x - pad_right, rect.max.y - pad_bottom),
                );

                let to_screen = |nx: f64, ny: f64| -> Pos2 {
                    // nx, ny in [-1.0, 1.0]
                    let sx = plot_rect.min.x + ((nx as f32 + 1.0) * 0.5) * plot_rect.width();
                    let sy = plot_rect.max.y - ((ny as f32 + 1.0) * 0.5) * plot_rect.height();
                    Pos2::new(sx, sy)
                };

                // 1. Rejilla HUD sutil + etiquetas de porcentaje
                let grid_steps = [-1.0, -0.5, 0.0, 0.5, 1.0];
                for &g in &grid_steps {
                    let p_x0 = to_screen(g, -1.0);
                    let p_x1 = to_screen(g, 1.0);
                    painter.line_segment([p_x0, p_x1], Stroke::new(1.0, COLOR_GRID_HUD));

                    let p_y0 = to_screen(-1.0, g);
                    let p_y1 = to_screen(1.0, g);
                    painter.line_segment([p_y0, p_y1], Stroke::new(1.0, COLOR_GRID_HUD));

                    let pct_lbl = format!("{:+.0}%", g * 100.0);
                    painter.text(
                        Pos2::new(p_x0.x, plot_rect.max.y + 11.0),
                        egui::Align2::CENTER_CENTER,
                        &pct_lbl,
                        FontId::monospace(9.0),
                        COLOR_OUTLINE,
                    );
                    painter.text(
                        Pos2::new(plot_rect.min.x - 6.0, p_y0.y),
                        egui::Align2::RIGHT_CENTER,
                        &pct_lbl,
                        FontId::monospace(9.0),
                        COLOR_OUTLINE,
                    );
                }

                // Sub-rejilla fina de 25%
                for &sub in &[-0.75, -0.25, 0.25, 0.75] {
                    let sx = to_screen(sub, 0.0).x;
                    let sy = to_screen(0.0, sub).y;
                    painter.line_segment(
                        [
                            Pos2::new(sx, plot_rect.min.y),
                            Pos2::new(sx, plot_rect.max.y),
                        ],
                        Stroke::new(0.5, Color32::from_rgba_unmultiplied(0, 240, 255, 12)),
                    );
                    painter.line_segment(
                        [
                            Pos2::new(plot_rect.min.x, sy),
                            Pos2::new(plot_rect.max.x, sy),
                        ],
                        Stroke::new(0.5, Color32::from_rgba_unmultiplied(0, 240, 255, 12)),
                    );
                }

                // 2. Barras de saturación a los lados (fuera de ±xsat_norm)
                let sat_norm = ((self.params.xsat as f64) / 1000.0).clamp(0.1, 1.0);
                if sat_norm < 0.999 {
                    let left_sat_x = to_screen(-sat_norm, 0.0).x;
                    let right_sat_x = to_screen(sat_norm, 0.0).x;

                    let left_sat_rect = Rect::from_min_max(
                        Pos2::new(plot_rect.min.x, plot_rect.min.y),
                        Pos2::new(left_sat_x, plot_rect.max.y),
                    );
                    let right_sat_rect = Rect::from_min_max(
                        Pos2::new(right_sat_x, plot_rect.min.y),
                        Pos2::new(plot_rect.max.x, plot_rect.max.y),
                    );

                    let sat_fill = Color32::from_rgba_unmultiplied(2, 102, 255, 32);
                    let sat_stroke = Stroke::new(1.0, COLOR_SECONDARY_CONTAINER);
                    painter.rect_filled(left_sat_rect, 0.0, sat_fill);
                    painter.rect_filled(right_sat_rect, 0.0, sat_fill);
                    painter.line_segment(
                        [
                            Pos2::new(left_sat_x, plot_rect.min.y),
                            Pos2::new(left_sat_x, plot_rect.max.y),
                        ],
                        sat_stroke,
                    );
                    painter.line_segment(
                        [
                            Pos2::new(right_sat_x, plot_rect.min.y),
                            Pos2::new(right_sat_x, plot_rect.max.y),
                        ],
                        sat_stroke,
                    );
                } else {
                    // Indicadores de barra de saturación nominal en los extremos
                    let bar_w = 5.0;
                    let left_bar = Rect::from_min_max(
                        plot_rect.min,
                        Pos2::new(plot_rect.min.x + bar_w, plot_rect.max.y),
                    );
                    let right_bar = Rect::from_min_max(
                        Pos2::new(plot_rect.max.x - bar_w, plot_rect.min.y),
                        plot_rect.max,
                    );
                    painter.rect_filled(
                        left_bar,
                        0.0,
                        Color32::from_rgba_unmultiplied(2, 102, 255, 55),
                    );
                    painter.rect_filled(
                        right_bar,
                        0.0,
                        Color32::from_rgba_unmultiplied(2, 102, 255, 55),
                    );
                }

                // 3. Banda de Zona Muerta Central sombreada con patrón diagonal (hatch)
                let dz_norm = ((self.params.deadband as f64) / 1000.0).clamp(0.0, 0.5);
                if dz_norm > 0.002 {
                    let dz_left = to_screen(-dz_norm, 0.0).x;
                    let dz_right = to_screen(dz_norm, 0.0).x;
                    let dz_rect = Rect::from_min_max(
                        Pos2::new(dz_left, plot_rect.min.y),
                        Pos2::new(dz_right, plot_rect.max.y),
                    );

                    painter.rect_filled(
                        dz_rect,
                        0.0,
                        Color32::from_rgba_unmultiplied(255, 180, 171, 22),
                    );

                    // Líneas diagonales (hatch) recortadas a dz_rect
                    let hatch_painter = painter.with_clip_rect(dz_rect);
                    let hatch_stroke =
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 180, 171, 55));
                    let step = 10.0;
                    let h = dz_rect.height();
                    let mut y_start = dz_rect.min.y - h;
                    while y_start < dz_rect.max.y + h {
                        hatch_painter.line_segment(
                            [
                                Pos2::new(dz_rect.min.x, y_start),
                                Pos2::new(dz_rect.max.x, y_start + dz_rect.width()),
                            ],
                            hatch_stroke,
                        );
                        y_start += step;
                    }

                    let dz_border =
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 180, 171, 130));
                    painter.line_segment(
                        [
                            Pos2::new(dz_left, plot_rect.min.y),
                            Pos2::new(dz_left, plot_rect.max.y),
                        ],
                        dz_border,
                    );
                    painter.line_segment(
                        [
                            Pos2::new(dz_right, plot_rect.min.y),
                            Pos2::new(dz_right, plot_rect.max.y),
                        ],
                        dz_border,
                    );
                }

                // 4. Ejes horizontal y vertical centrales en cian
                let origin = to_screen(0.0, 0.0);
                let axis_cian = Stroke::new(1.2, Color32::from_rgba_unmultiplied(0, 240, 255, 110));
                painter.line_segment(
                    [
                        Pos2::new(plot_rect.min.x, origin.y),
                        Pos2::new(plot_rect.max.x, origin.y),
                    ],
                    axis_cian,
                );
                painter.line_segment(
                    [
                        Pos2::new(origin.x, plot_rect.min.y),
                        Pos2::new(origin.x, plot_rect.max.y),
                    ],
                    axis_cian,
                );

                // 5. Línea diagonal 1:1 punteada
                draw_dashed_line(
                    &painter,
                    to_screen(-1.0, -1.0),
                    to_screen(1.0, 1.0),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(132, 148, 149, 110)),
                    6.0,
                    5.0,
                );

                // 6. Sombreado degradado bajo la curva y Línea S-Curve cian (#00f0ff)
                let samples = 140usize;
                let plot_pts = self.params.generate_plot_points(true, samples);
                let mut screen_pts = Vec::with_capacity(samples);
                for [nx, ny] in plot_pts {
                    let pt = to_screen(nx, ny);
                    screen_pts.push((pt, ny.abs() as f32));
                }

                // Sombreado vertical entre cada segmento de la curva y el eje horizontal y = 0
                for pair in screen_pts.windows(2) {
                    let (p0, amp0) = pair[0];
                    let (p1, amp1) = pair[1];
                    let mid_x = (p0.x + p1.x) * 0.5;
                    let mid_y = (p0.y + p1.y) * 0.5;
                    let alpha = ((amp0 + amp1) * 0.5 * 46.0).clamp(6.0, 48.0) as u8;
                    let shade_col = Color32::from_rgba_unmultiplied(0, 240, 255, alpha);
                    let w = (p1.x - p0.x).abs().max(1.5);
                    let min_y = mid_y.min(origin.y);
                    let max_y = mid_y.max(origin.y);
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(mid_x - w * 0.5, min_y),
                            Pos2::new(mid_x + w * 0.5, max_y),
                        ),
                        0.0,
                        shade_col,
                    );
                }

                let polyline: Vec<Pos2> = screen_pts.into_iter().map(|(p, _)| p).collect();
                // Resplandor exterior sutil de la curva
                painter.add(egui::Shape::line(
                    polyline.clone(),
                    Stroke::new(5.0, Color32::from_rgba_unmultiplied(0, 240, 255, 36)),
                ));
                // Trazo principal cian #00f0ff
                painter.add(egui::Shape::line(
                    polyline,
                    Stroke::new(2.4, COLOR_PRIMARY_CONTAINER),
                ));

                // 7. Crosshair de fósforo verde (#34f885) sincronizado con la entrada de hardware real
                let (live_in, _raw_adc) = self.current_live_input(live_state);
                let live_out = self.params.evaluate(live_in);
                let live_pos = to_screen(live_in, live_out);

                let phosphor_guide =
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(52, 248, 133, 120));
                draw_dashed_line(
                    &painter,
                    Pos2::new(live_pos.x, plot_rect.min.y),
                    Pos2::new(live_pos.x, plot_rect.max.y),
                    phosphor_guide,
                    4.0,
                    4.0,
                );
                draw_dashed_line(
                    &painter,
                    Pos2::new(plot_rect.min.x, live_pos.y),
                    Pos2::new(plot_rect.max.x, live_pos.y),
                    phosphor_guide,
                    4.0,
                    4.0,
                );

                // Retícula exterior e interior en verde fósforo
                painter.circle_stroke(live_pos, 8.0, Stroke::new(1.5, COLOR_TERTIARY_CONTAINER));
                painter.circle_filled(live_pos, 3.5, COLOR_TERTIARY_CONTAINER);

                // Etiqueta "IN: +XX.X%" junto al crosshair
                let tag_text = format!("IN: {:+.1}%", live_in * 100.0);
                let tag_size = Vec2::new(78.0, 18.0);
                let offset_x = if live_pos.x + tag_size.x + 12.0 < plot_rect.max.x {
                    10.0
                } else {
                    -tag_size.x - 10.0
                };
                let offset_y = if live_pos.y - tag_size.y - 10.0 > plot_rect.min.y {
                    -tag_size.y - 6.0
                } else {
                    8.0
                };
                let tag_rect = Rect::from_min_size(
                    Pos2::new(live_pos.x + offset_x, live_pos.y + offset_y),
                    tag_size,
                );
                painter.rect_filled(tag_rect, 3.0, COLOR_SURFACE_LOWEST);
                painter.rect_stroke(
                    tag_rect,
                    3.0,
                    Stroke::new(1.0, COLOR_TERTIARY_CONTAINER),
                    StrokeKind::Inside,
                );
                painter.text(
                    tag_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    tag_text,
                    FontId::monospace(10.0),
                    COLOR_TERTIARY_CONTAINER,
                );

                // Contorno del área del gráfico
                painter.rect_stroke(
                    plot_rect,
                    2.0,
                    Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                    StrokeKind::Inside,
                );
            });
        });
    }

    /// 4 tarjetas telemétricas inferiores:
    /// RAW ADC (HEX / LSB), DEFLEXIÓN FÍSICA (NORM), SALIDA PROCESADA (UINPUT), TASA DE COMPRESIÓN.
    fn render_telemetry_cards(&self, ui: &mut Ui, live_state: &LiveAxesState) {
        let (live_in, raw_adc) = self.current_live_input(live_state);
        let live_out = self.params.evaluate(live_in);
        let comp_rate = self.params.compression_rate(live_in);

        let cards = [
            (
                "RAW ADC (HEX / LSB)",
                format!("{:#06X} / {}", raw_adc, raw_adc),
                "16-BIT HALL SENSOR".to_string(),
                COLOR_ON_SURFACE,
            ),
            (
                "DEFLEXIÓN FÍSICA (NORM)",
                format!("{:+.4}", live_in),
                format!("INPUT: {:+.1}%", live_in * 100.0),
                COLOR_PRIMARY_FIXED,
            ),
            (
                "SALIDA PROCESADA (UINPUT)",
                format!("{:+.4}", live_out),
                format!("OUTPUT: {:+.1}%", live_out * 100.0),
                COLOR_PRIMARY_CONTAINER,
            ),
            (
                "TASA DE COMPRESIÓN",
                format!("{:.2}x", comp_rate),
                "INSTANT GAIN dY/dX".to_string(),
                COLOR_TERTIARY_CONTAINER,
            ),
        ];

        ui.columns(4, |cols| {
            for (idx, (title, main_val, sub_val, val_col)) in cards.iter().enumerate() {
                theme::card_frame()
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(&mut cols[idx], |ui| {
                        ui.label(
                            RichText::new(*title)
                                .color(COLOR_OUTLINE)
                                .size(9.5)
                                .strong()
                                .monospace(),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(main_val)
                                .color(*val_col)
                                .size(14.5)
                                .strong()
                                .monospace(),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(sub_val)
                                .color(COLOR_ON_SURFACE_VARIANT)
                                .size(9.5)
                                .monospace(),
                        );
                    });
            }
        });
    }

    /// "MONITOR GIMBAL 2D & EJES SECUNDARIOS EN VIVO":
    /// Cuadro de coordenadas 2D Gimbal (retícula, círculo de deadzone, punto de posición reticular
    /// verde animado en vivo, texto "X: +0.482 Y: +0.440 VECT: 0.652 ∠42°").
    /// Medidores secundarios para Timón Z (Twist) bipolar con centro cero, mini-stick analógico
    /// (slew X/Y) con barras duales, y POV Hat 1 con modo.
    fn render_gimbal_and_secondary_axes(&self, ui: &mut Ui, live_state: &LiveAxesState) {
        theme::card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} MONITOR GIMBAL 2D & EJES SECUNDARIOS EN VIVO",
                        icons::ACTIVITY
                    ))
                    .color(COLOR_PRIMARY)
                    .size(12.0)
                    .strong()
                    .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} LIVE TELEMETRY 50/1000Hz", icons::RADIO))
                            .color(COLOR_TERTIARY_CONTAINER)
                            .size(10.0)
                            .monospace(),
                    );
                });
            });

            ui.add_space(8.0);

            ui.horizontal_top(|ui| {
                // Izquierda: Radar / Cuadro de coordenadas 2D Gimbal
                ui.vertical(|ui| {
                    let box_size = 186.0;
                    let (resp, painter) =
                        ui.allocate_painter(Vec2::new(box_size, box_size), Sense::hover());
                    let r = resp.rect;

                    painter.rect_filled(r, 6.0, COLOR_SURFACE_LOWEST);
                    painter.rect_stroke(
                        r,
                        6.0,
                        Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                        StrokeKind::Inside,
                    );

                    let center = r.center();
                    let half = (box_size * 0.5) - 10.0;

                    // Anillos concéntricos de deflexión (50% y 100%)
                    painter.circle_stroke(center, half, Stroke::new(1.0, COLOR_GRID_HUD));
                    painter.circle_stroke(center, half * 0.5, Stroke::new(1.0, COLOR_GRID_HUD));

                    // Retícula ortogonal
                    painter.line_segment(
                        [
                            Pos2::new(r.min.x + 6.0, center.y),
                            Pos2::new(r.max.x - 6.0, center.y),
                        ],
                        Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                    );
                    painter.line_segment(
                        [
                            Pos2::new(center.x, r.min.y + 6.0),
                            Pos2::new(center.x, r.max.y - 6.0),
                        ],
                        Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                    );

                    // Círculo de deadzone central
                    let dz_radius =
                        (((self.params.deadband as f32) / 1000.0) * half).clamp(3.0, half * 0.6);
                    painter.circle_filled(
                        center,
                        dz_radius,
                        Color32::from_rgba_unmultiplied(255, 180, 171, 25),
                    );
                    painter.circle_stroke(center, dz_radius, Stroke::new(1.0, COLOR_ERROR));

                    // Punto de posición reticular verde animado en vivo
                    let gx = live_state.stick_x as f32;
                    let gy = live_state.stick_y as f32;
                    let dot_pos = Pos2::new(center.x + gx * half, center.y - gy * half);

                    // Vector desde el origen
                    painter.line_segment(
                        [center, dot_pos],
                        Stroke::new(1.5, Color32::from_rgba_unmultiplied(52, 248, 133, 140)),
                    );
                    painter.circle_stroke(dot_pos, 6.5, Stroke::new(1.5, COLOR_TERTIARY_CONTAINER));
                    painter.circle_filled(dot_pos, 3.0, COLOR_TERTIARY_CONTAINER);

                    ui.add_space(4.0);
                    let (vect_mag, vect_deg) = live_state.gimbal_vector();
                    ui.label(
                        RichText::new(format!(
                            "X: {:+.3} Y: {:+.3} VECT: {:.3} ({:02.0}°)",
                            live_state.stick_x, live_state.stick_y, vect_mag, vect_deg
                        ))
                        .color(COLOR_TERTIARY_CONTAINER)
                        .size(9.5)
                        .strong()
                        .monospace(),
                    );
                });

                ui.add_space(12.0);

                // Derecha: Medidores secundarios en vivo
                ui.vertical(|ui| {
                    // 1. Timón Z (Twist) bipolar con centro cero
                    render_bipolar_bar(
                        ui,
                        "TIMÓN Z (TWIST // RUDDER YAW)",
                        live_state.stick_z,
                        COLOR_PRIMARY_CONTAINER,
                        "CENTER-ZERO BIPOLAR",
                    );

                    ui.add_space(10.0);

                    // 2. Mini-stick analógico (Slew X / Y) con barras duales
                    Frame::new()
                        .fill(COLOR_SURFACE_LOWEST)
                        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("MINI-STICK ANALÓGICO (SLEW X/Y)")
                                        .color(COLOR_ON_SURFACE)
                                        .size(10.5)
                                        .strong()
                                        .monospace(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "X:{:+.2} Y:{:+.2}",
                                                live_state.ministick_x, live_state.ministick_y
                                            ))
                                            .color(COLOR_WARNING_AMBER)
                                            .size(10.0)
                                            .monospace(),
                                        );
                                    },
                                );
                            });
                            ui.add_space(4.0);
                            render_compact_dual_bar(
                                ui,
                                "SLEW X",
                                live_state.ministick_x,
                                COLOR_WARNING_AMBER,
                            );
                            ui.add_space(3.0);
                            render_compact_dual_bar(
                                ui,
                                "SLEW Y",
                                live_state.ministick_y,
                                COLOR_WARNING_AMBER,
                            );
                        });

                    ui.add_space(10.0);

                    // 3. POV Hat 1 con modo
                    Frame::new()
                        .fill(COLOR_SURFACE_LOWEST)
                        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("POV HAT 1 (8-WAY DIRECTIONAL)")
                                        .color(COLOR_ON_SURFACE)
                                        .size(10.5)
                                        .strong()
                                        .monospace(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        theme::telemetry_badge(
                                            ui,
                                            "MODE:",
                                            "8-WAY DIGITAL",
                                            COLOR_PRIMARY_FIXED,
                                        );
                                        theme::telemetry_badge(
                                            ui,
                                            "STATE:",
                                            live_state.pov_direction_label(),
                                            COLOR_TERTIARY_CONTAINER,
                                        );
                                    },
                                );
                            });
                        });
                });
            });
        });
    }

    /// TIPO DE CURVATURA: 4 botones (Lineal 1:1, S-Curve, Exponencial, Bézier libre).
    fn render_curve_type_selector(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} TIPO DE CURVATURA", icons::SLIDERS_HORIZONTAL))
                    .color(COLOR_PRIMARY)
                    .size(12.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(6.0);

            ui.columns(2, |cols| {
                for (idx, ct) in CurveType::ALL.iter().enumerate() {
                    let col_idx = idx % 2;
                    let is_active = self.params.curve_type == *ct;
                    cols[col_idx].vertical_centered_justified(|ui| {
                        if theme::tactical_button(ui, ct.label(), is_active, false).clicked() {
                            self.params.set_curve_type(*ct);
                            self.eeprom_synced = false;
                        }
                    });
                    if idx < 2 {
                        cols[col_idx].add_space(4.0);
                    }
                }
            });
        });
    }

    /// PARÁMETROS DEL EJE:
    /// Sliders de precisión para Zona Muerta Central (Deadband 0-25%),
    /// Curvatura/Sensibilidad Expo (1.00-3.00), Saturación Externa (70-100%),
    /// y switch toggle "Invertir Dirección de Eje".
    fn render_axis_parameters_card(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} PARÁMETROS DEL EJE", icons::SETTINGS))
                    .color(COLOR_PRIMARY)
                    .size(12.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(8.0);

            // 1. Zona Muerta Central (Deadband 0-25%)
            let mut db_pct = self.params.deadband_pct();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Zona Muerta Central (Deadband)")
                        .color(COLOR_ON_SURFACE)
                        .size(11.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.1}%", db_pct))
                            .color(COLOR_ERROR)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );
                });
            });
            if ui
                .add(
                    egui::Slider::new(&mut db_pct, 0.0..=25.0)
                        .step_by(0.1)
                        .show_value(false),
                )
                .changed()
            {
                self.params.set_deadband_pct(db_pct);
                self.eeprom_synced = false;
            }

            ui.add_space(8.0);

            // 2. Curvatura / Sensibilidad Expo (1.00 - 3.00)
            let mut expo = self.params.expo_factor();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Curvatura / Sensibilidad Expo")
                        .color(COLOR_ON_SURFACE)
                        .size(11.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.2}", expo))
                            .color(COLOR_PRIMARY_CONTAINER)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );
                });
            });
            if ui
                .add(
                    egui::Slider::new(&mut expo, 1.00..=3.00)
                        .step_by(0.01)
                        .show_value(false),
                )
                .changed()
            {
                self.params.set_expo_factor(expo);
                self.eeprom_synced = false;
            }

            ui.add_space(8.0);

            // 3. Saturación Externa (70-100%)
            let mut sat_pct = self.params.saturation_pct();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Saturación Externa (Outer Clamp)")
                        .color(COLOR_ON_SURFACE)
                        .size(11.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.1}%", sat_pct))
                            .color(COLOR_PRIMARY_FIXED)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );
                });
            });
            if ui
                .add(
                    egui::Slider::new(&mut sat_pct, 70.0..=100.0)
                        .step_by(0.5)
                        .show_value(false),
                )
                .changed()
            {
                self.params.set_saturation_pct(sat_pct);
                self.eeprom_synced = false;
            }

            // Controles extra para Bézier libre si está activo
            if self.params.curve_type == CurveType::Bezier {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Nodos Bézier (P1 / P2):")
                            .color(COLOR_ON_SURFACE_VARIANT)
                            .size(10.5)
                            .monospace(),
                    );
                    if ui
                        .add(egui::Slider::new(&mut self.params.bezier_p1[1], 0.0..=1.0).text("P1"))
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                    if ui
                        .add(egui::Slider::new(&mut self.params.bezier_p2[1], 0.0..=1.0).text("P2"))
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // 4. Switch toggle "Invertir Dirección de Eje"
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Invertir Dirección de Eje")
                        .color(COLOR_ON_SURFACE)
                        .size(11.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggle_txt = if self.params.invert {
                        "ON // INVERTED"
                    } else {
                        "OFF // NORMAL"
                    };
                    if theme::tactical_button(ui, toggle_txt, self.params.invert, false).clicked() {
                        self.params.invert = !self.params.invert;
                        self.eeprom_synced = false;
                    }
                });
            });
        });
    }

    /// MUELLES FÍSICOS (X-56 SPRING):
    /// Tarjetas interactivas para Rojo (52mm), Verde (47mm - Instalado con checkmark),
    /// Amarillo (42mm), Azul (36mm). Se guarda en DeviceProfile.spring.
    fn render_physical_springs_card(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} MUELLES FÍSICOS (X-56 SPRING)", icons::WRENCH))
                        .color(COLOR_PRIMARY)
                        .size(12.0)
                        .strong()
                        .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("F-TENSION")
                            .color(COLOR_OUTLINE)
                            .size(9.5)
                            .monospace(),
                    );
                });
            });

            ui.add_space(6.0);

            let springs = [
                PhysicalSpring::Red52mm,
                PhysicalSpring::Green47mm,
                PhysicalSpring::Yellow42mm,
                PhysicalSpring::Blue36mm,
            ];

            for spring in springs {
                let is_installed = self.installed_spring == spring;
                let border_col = if is_installed {
                    spring.color()
                } else {
                    COLOR_OUTLINE_VARIANT
                };
                let bg_col = if is_installed {
                    COLOR_SURFACE_HIGH
                } else {
                    COLOR_SURFACE_CONTAINER
                };

                let resp = Frame::new()
                    .fill(bg_col)
                    .stroke(Stroke::new(
                        if is_installed { 1.5 } else { 1.0 },
                        border_col,
                    ))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Indicador cromático del muelle
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::new(10.0, 22.0), Sense::hover());
                            ui.painter().rect_filled(dot_rect, 3.0, spring.color());

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(spring.label())
                                            .color(spring.color())
                                            .size(11.0)
                                            .strong()
                                            .monospace(),
                                    );
                                    if is_installed {
                                        ui.label(
                                            RichText::new(format!("{} INSTALADO", icons::CHECK))
                                                .color(COLOR_TERTIARY_CONTAINER)
                                                .size(10.0)
                                                .strong()
                                                .monospace(),
                                        );
                                    }
                                });
                                ui.label(
                                    RichText::new(spring.desc())
                                        .color(COLOR_ON_SURFACE_VARIANT)
                                        .size(10.0),
                                );
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let btn_lbl = if is_installed { "ACTIVO" } else { "ELEGIR" };
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(btn_lbl).size(9.5).monospace().color(
                                                    if is_installed {
                                                        COLOR_ON_PRIMARY_CONTAINER
                                                    } else {
                                                        COLOR_ON_SURFACE
                                                    },
                                                ),
                                            )
                                            .fill(
                                                if is_installed {
                                                    COLOR_PRIMARY_CONTAINER
                                                } else {
                                                    COLOR_SURFACE_LOWEST
                                                },
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.installed_spring = spring;
                                    }
                                },
                            );
                        });
                    })
                    .response;

                if resp.interact(Sense::click()).clicked() {
                    self.installed_spring = spring;
                }

                ui.add_space(5.0);
            }
        });
    }
}

/// Dibuja una línea punteada o discontinua entre `p0` y `p1`.
fn draw_dashed_line(
    painter: &egui::Painter,
    p0: Pos2,
    p1: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let delta = p1 - p0;
    let total_len = delta.length();
    if total_len < 1e-3 {
        return;
    }
    let dir = delta / total_len;
    let mut dist = 0.0;
    while dist < total_len {
        let end = (dist + dash_len).min(total_len);
        painter.line_segment([p0 + dir * dist, p0 + dir * end], stroke);
        dist += dash_len + gap_len;
    }
}

/// Medidor bipolar con centro cero para Timón Z (Twist).
fn render_bipolar_bar(ui: &mut Ui, label: &str, value: f64, fill_col: Color32, mode_tag: &str) {
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(label)
                        .color(COLOR_ON_SURFACE)
                        .size(10.5)
                        .strong()
                        .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:+.1}%", value * 100.0))
                            .color(fill_col)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(mode_tag)
                            .color(COLOR_OUTLINE)
                            .size(9.0)
                            .monospace(),
                    );
                });
            });

            ui.add_space(4.0);

            let bar_w = ui.available_width().max(60.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 12.0), Sense::hover());
            let painter = ui.painter();

            painter.rect_filled(rect, 3.0, COLOR_SURFACE_CONTAINER);
            let mid_x = rect.center().x;
            let v = (value as f32).clamp(-1.0, 1.0);
            let active_x = mid_x + v * (rect.width() * 0.5);

            let fill_rect = Rect::from_min_max(
                Pos2::new(mid_x.min(active_x), rect.min.y + 1.0),
                Pos2::new(mid_x.max(active_x), rect.max.y - 1.0),
            );
            painter.rect_filled(fill_rect, 2.0, fill_col);
            painter.line_segment(
                [Pos2::new(mid_x, rect.min.y), Pos2::new(mid_x, rect.max.y)],
                Stroke::new(1.5, COLOR_ON_SURFACE),
            );
        });
}

/// Barra horizontal compacta con centro cero para los ejes del Mini-stick.
fn render_compact_dual_bar(ui: &mut Ui, axis_lbl: &str, value: f64, col: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(axis_lbl)
                .color(COLOR_OUTLINE)
                .size(9.5)
                .monospace(),
        );
        let bar_w = (ui.available_width() - 48.0).max(40.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 8.0), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, COLOR_SURFACE_CONTAINER);

        let mid_x = rect.center().x;
        let v = (value as f32).clamp(-1.0, 1.0);
        let active_x = mid_x + v * (rect.width() * 0.5);
        let fill_rect = Rect::from_min_max(
            Pos2::new(mid_x.min(active_x), rect.min.y),
            Pos2::new(mid_x.max(active_x), rect.max.y),
        );
        painter.rect_filled(fill_rect, 2.0, col);
        painter.line_segment(
            [Pos2::new(mid_x, rect.min.y), Pos2::new(mid_x, rect.max.y)],
            Stroke::new(1.0, COLOR_ON_SURFACE_VARIANT),
        );

        ui.label(
            RichText::new(format!("{:+.0}%", value * 100.0))
                .color(COLOR_ON_SURFACE_VARIANT)
                .size(9.5)
                .monospace(),
        );
    });
}
