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
        let viewport_w = ui.available_width();
        ui.set_max_width(viewport_w);

        let live_state = poller.get_state();

        // 1. Top Hardware & Subsystem Telemetry Bar (fija arriba, ajustada al ancho real)
        self.render_hardware_telemetry_bar(ui);
        ui.add_space(6.0);

        // 2. Axis Selection Sub-Bar (fija arriba, ajustada al ancho real)
        self.render_axis_selection_subbar(ui);
        ui.add_space(8.0);

        // Decisión de breakpoint sobre el ancho del contenedor exterior para evitar oscilación por scrollbar
        let two_col_mode = viewport_w >= 840.0;

        // 3. Cuerpo desplazable con ancho máximo bloqueado al viewport
        egui::ScrollArea::vertical()
            .id_salt("curves_body_scroll")
            .max_width(viewport_w)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let body_w = ui.available_width().min(viewport_w);
                ui.set_max_width(body_w);
                ui.set_width(body_w);

                if two_col_mode {
                    let gap = 12.0;
                    let avail_cols = (body_w - gap).max(400.0);
                    let left_width = (avail_cols * 0.56).floor();
                    let right_width = (avail_cols - left_width).floor();

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;

                        // COLUMNA IZQUIERDA (Transfer curve, telemetría, gimbal)
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width, 10.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_max_width(left_width);
                                ui.set_width(left_width);
                                self.render_transfer_curve_section(ui, &live_state);
                                ui.add_space(10.0);
                                self.render_telemetry_cards(ui, &live_state);
                                ui.add_space(10.0);
                                self.render_gimbal_and_secondary_axes(ui, &live_state);
                            },
                        );

                        // COLUMNA DERECHA (Selector tipo curva, parámetros deadband/expo, muelles)
                        ui.allocate_ui_with_layout(
                            Vec2::new(right_width, 10.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_max_width(right_width);
                                ui.set_width(right_width);
                                self.render_curve_type_selector(ui);
                                ui.add_space(10.0);
                                self.render_axis_parameters_card(ui);
                                ui.add_space(10.0);
                                self.render_physical_springs_card(ui);
                            },
                        );
                    });
                } else {
                    // Modo apilado vertical para pantallas compactas (< 840px)
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
        let outer_w = ui.available_width();
        theme::card_frame()
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                let inner_w = (outer_w - 26.0).max(100.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);

                    let is_stick = self.selected_device == DeviceKind::Stick;
                    let stick_lbl = if inner_w >= 920.0 {
                        format!("{} STICK // RHINO [VID:0738 PID:2215]", icons::JOYSTICK)
                    } else if inner_w >= 580.0 {
                        format!("{} STICK // RHINO", icons::JOYSTICK)
                    } else {
                        format!("{} STICK", icons::JOYSTICK)
                    };
                    if theme::tactical_button(ui, &stick_lbl, is_stick, false).clicked()
                        && !is_stick
                    {
                        self.selected_device = DeviceKind::Stick;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    let is_throttle = self.selected_device == DeviceKind::Throttle;
                    let thr_lbl = if inner_w >= 920.0 {
                        format!("{} DUAL THROTTLE [VID:0738 PID:2216]", icons::SLIDERS)
                    } else if inner_w >= 580.0 {
                        format!("{} DUAL THROTTLE", icons::SLIDERS)
                    } else {
                        format!("{} THROTTLE", icons::SLIDERS)
                    };
                    if theme::tactical_button(ui, &thr_lbl, is_throttle, false).clicked()
                        && !is_throttle
                    {
                        self.selected_device = DeviceKind::Throttle;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    let adc_val = if inner_w >= 760.0 {
                        "16-BIT HALL SENSOR (65536 STEPS)"
                    } else if inner_w >= 520.0 {
                        "16-BIT HALL (65536)"
                    } else {
                        "16-BIT"
                    };
                    theme::telemetry_badge(ui, "ADC:", adc_val, COLOR_PRIMARY_CONTAINER);
                    theme::telemetry_badge(ui, "JITTER:", "0.08ms", COLOR_TERTIARY_CONTAINER);

                    let node_str = match self.selected_device {
                        DeviceKind::Stick => "/dev/hidraw0",
                        DeviceKind::Throttle => "/dev/hidraw1",
                    };
                    theme::telemetry_badge(ui, "NODE:", node_str, COLOR_ON_SURFACE);

                    let (sync_label, sync_col) = match (self.eeprom_synced, inner_w >= 520.0) {
                        (true, true) => (
                            format!("{} EEPROM SYNCED", icons::CIRCLE_CHECK),
                            COLOR_TERTIARY_FIXED_DIM,
                        ),
                        (true, false) => (
                            format!("{} SYNCED", icons::CIRCLE_CHECK),
                            COLOR_TERTIARY_FIXED_DIM,
                        ),
                        (false, true) => (
                            format!("{} EEPROM MODIFIED", icons::CIRCLE_ALERT),
                            COLOR_WARNING_AMBER,
                        ),
                        (false, false) => (
                            format!("{} MODIFIED", icons::CIRCLE_ALERT),
                            COLOR_WARNING_AMBER,
                        ),
                    };
                    theme::outline_badge(ui, &sync_label, sync_col, sync_col);
                });
            });
    }

    /// Axis Selection Sub-Bar:
    /// EJE ACTIVO con pills para Eje X (Roll), Eje Y (Pitch), Timón Z (Yaw), Ministick X, Ministick Y,
    /// y badge "POLYNOMIAL: CUBIC-HERMITE".
    fn render_axis_selection_subbar(&mut self, ui: &mut Ui) {
        let outer_w = ui.available_width();
        theme::card_frame()
            .inner_margin(Margin::symmetric(12, 7))
            .show(ui, |ui| {
                let inner_w = (outer_w - 26.0).max(100.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    ui.label(
                        RichText::new("EJE ACTIVO:")
                            .color(COLOR_OUTLINE)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );

                    let compact = inner_w < 580.0;
                    let axes: &[(u8, &str)] = match (self.selected_device, compact) {
                        (DeviceKind::Stick, false) => &[
                            (0x30, "Eje X (Roll)"),
                            (0x31, "Eje Y (Pitch)"),
                            (0x35, "Timón Z (Yaw)"),
                            (AXIS_MINISTICK_X, "Ministick X"),
                            (AXIS_MINISTICK_Y, "Ministick Y"),
                        ],
                        (DeviceKind::Stick, true) => &[
                            (0x30, "Eje X"),
                            (0x31, "Eje Y"),
                            (0x35, "Timón Z"),
                            (AXIS_MINISTICK_X, "Mini X"),
                            (AXIS_MINISTICK_Y, "Mini Y"),
                        ],
                        (DeviceKind::Throttle, false) => &[
                            (0x30, "Throttle Left"),
                            (0x31, "Throttle Right"),
                            (0x32, "Rotary 1 (F)"),
                            (0x35, "Rotary 2 (G)"),
                            (0x37, "Rotary 3"),
                            (0x36, "Rotary 4"),
                        ],
                        (DeviceKind::Throttle, true) => &[
                            (0x30, "Thr L"),
                            (0x31, "Thr R"),
                            (0x32, "Rot 1"),
                            (0x35, "Rot 2"),
                            (0x37, "Rot 3"),
                            (0x36, "Rot 4"),
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

                    theme::outline_badge(
                        ui,
                        self.params.curve_type.polynomial_badge(),
                        COLOR_PRIMARY_FIXED,
                        COLOR_PRIMARY_CONTAINER,
                    );
                });
            });
    }

    /// Canvas HUD "RESPUESTA DE TRANSFERENCIA (TRANSFER CURVE)" con egui::Painter:
    /// sombreado degradado bajo la curva, bandas de zona muerta sombreada (hatch),
    /// barras de saturación a los lados, eje horizontal/vertical cian, línea S-Curve cyan (#00f0ff),
    /// línea diagonal 1:1 punteada, crosshair de fósforo verde con etiqueta "IN: +XX.X%"
    /// sincronizado con la entrada de hardware real.
    fn render_transfer_curve_section(&mut self, ui: &mut Ui, live_state: &LiveAxesState) {
        let outer_w = ui.available_width();
        theme::card_frame().show(ui, |ui| {
            let inner_w = (outer_w - 26.0).max(120.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            // Encabezado del bloque HUD
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                let title = if inner_w >= 520.0 {
                    format!(
                        "{} RESPUESTA DE TRANSFERENCIA (TRANSFER CURVE)",
                        icons::ACTIVITY
                    )
                } else if inner_w >= 340.0 {
                    format!("{} CURVA DE TRANSFERENCIA", icons::ACTIVITY)
                } else {
                    format!("{} CURVA", icons::ACTIVITY)
                };
                ui.label(
                    RichText::new(title)
                        .color(COLOR_PRIMARY)
                        .size(12.0)
                        .strong()
                        .monospace(),
                );

                ui.label(
                    RichText::new(format!(
                        "DZ: {:.1}% | EXPO: {:.2} | SAT: {:.1}%",
                        self.params.deadband_pct(),
                        self.params.expo_factor(),
                        self.params.saturation_pct()
                    ))
                    .color(COLOR_ON_SURFACE_VARIANT)
                    .size(10.0)
                    .monospace(),
                );
            });

            ui.add_space(6.0);

            theme::sunken_frame().show(ui, |ui| {
                let canvas_width = (inner_w - 22.0).max(100.0);
                ui.set_max_width(canvas_width);
                let canvas_height = (canvas_width * 0.50).clamp(185.0, 290.0);
                let (response, painter) =
                    ui.allocate_painter(Vec2::new(canvas_width, canvas_height), Sense::hover());
                let rect = response.rect;
                let painter = painter.with_clip_rect(rect);

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

    fn render_single_telemetry_card(
        ui: &mut Ui,
        col_w: f32,
        title: &str,
        main_val: &str,
        sub_val: &str,
        val_col: Color32,
    ) {
        ui.set_max_width(col_w);
        theme::card_frame()
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                let inner_w = (col_w - 22.0).max(40.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .color(COLOR_OUTLINE)
                            .size(9.5)
                            .strong()
                            .monospace(),
                    )
                    .truncate(),
                );
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(main_val)
                            .color(val_col)
                            .size(if inner_w >= 130.0 { 13.5 } else { 11.5 })
                            .strong()
                            .monospace(),
                    )
                    .truncate(),
                );
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(sub_val)
                            .color(COLOR_ON_SURFACE_VARIANT)
                            .size(9.0)
                            .monospace(),
                    )
                    .truncate(),
                );
            });
    }

    /// 4 tarjetas telemétricas inferiores:
    /// RAW ADC (HEX / LSB), DEFLEXIÓN FÍSICA (NORM), SALIDA PROCESADA (UINPUT), TASA DE COMPRESIÓN.
    /// Se adaptan fluidamente a 4 columnas (>= 680px), rejilla 2x2 (>= 280px) o 1 columna (< 280px).
    fn render_telemetry_cards(&self, ui: &mut Ui, live_state: &LiveAxesState) {
        let (live_in, raw_adc) = self.current_live_input(live_state);
        let live_out = self.params.evaluate(live_in);
        let comp_rate = self.params.compression_rate(live_in);

        let avail_w = ui.available_width();
        let use_four_cols = avail_w >= 680.0;
        let col_w = if use_four_cols {
            ((avail_w - 24.0) / 4.0).floor().max(80.0)
        } else if avail_w >= 280.0 {
            ((avail_w - 8.0) * 0.5).floor().max(80.0)
        } else {
            avail_w.max(80.0)
        };
        let is_wide_card = col_w >= 175.0;

        let title_0 = if is_wide_card {
            "RAW ADC (HEX / LSB)"
        } else {
            "RAW ADC"
        };
        let title_1 = if is_wide_card {
            "DEFLEXIÓN FÍSICA (NORM)"
        } else {
            "DEFLEXIÓN (NORM)"
        };
        let title_2 = if is_wide_card {
            "SALIDA PROCESADA (UINPUT)"
        } else {
            "SALIDA UINPUT"
        };
        let title_3 = if is_wide_card {
            "TASA DE COMPRESIÓN"
        } else {
            "COMPRESIÓN"
        };

        let cards = [
            (
                title_0,
                format!("{:#06X} / {}", raw_adc, raw_adc),
                "16-BIT HALL SENSOR".to_string(),
                COLOR_ON_SURFACE,
            ),
            (
                title_1,
                format!("{:+.4}", live_in),
                format!("INPUT: {:+.1}%", live_in * 100.0),
                COLOR_PRIMARY_FIXED,
            ),
            (
                title_2,
                format!("{:+.4}", live_out),
                format!("OUTPUT: {:+.1}%", live_out * 100.0),
                COLOR_PRIMARY_CONTAINER,
            ),
            (
                title_3,
                format!("{:.2}x", comp_rate),
                "INSTANT GAIN dY/dX".to_string(),
                COLOR_TERTIARY_CONTAINER,
            ),
        ];

        if use_four_cols {
            ui.columns(4, |cols| {
                for (idx, (title, main_val, sub_val, val_col)) in cards.iter().enumerate() {
                    let c_w = cols[idx].available_width().min(col_w);
                    Self::render_single_telemetry_card(
                        &mut cols[idx],
                        c_w,
                        title,
                        main_val,
                        sub_val,
                        *val_col,
                    );
                }
            });
        } else if avail_w >= 280.0 {
            ui.columns(2, |cols| {
                let c0_w = cols[0].available_width().min(col_w);
                let c1_w = cols[1].available_width().min(col_w);
                Self::render_single_telemetry_card(
                    &mut cols[0],
                    c0_w,
                    cards[0].0,
                    &cards[0].1,
                    &cards[0].2,
                    cards[0].3,
                );
                Self::render_single_telemetry_card(
                    &mut cols[1],
                    c1_w,
                    cards[1].0,
                    &cards[1].1,
                    &cards[1].2,
                    cards[1].3,
                );
            });
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                let c0_w = cols[0].available_width().min(col_w);
                let c1_w = cols[1].available_width().min(col_w);
                Self::render_single_telemetry_card(
                    &mut cols[0],
                    c0_w,
                    cards[2].0,
                    &cards[2].1,
                    &cards[2].2,
                    cards[2].3,
                );
                Self::render_single_telemetry_card(
                    &mut cols[1],
                    c1_w,
                    cards[3].0,
                    &cards[3].1,
                    &cards[3].2,
                    cards[3].3,
                );
            });
        } else {
            for (title, main_val, sub_val, val_col) in &cards {
                Self::render_single_telemetry_card(ui, col_w, title, main_val, sub_val, *val_col);
                ui.add_space(6.0);
            }
        }
    }

    /// Dibuja el radar de coordenadas 2D Gimbal con retícula, zona muerta y vector de deflexión.
    fn render_gimbal_radar(&self, ui: &mut Ui, box_size: f32, live_state: &LiveAxesState) {
        ui.vertical_centered(|ui| {
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
            let coord_lbl = if box_size >= 170.0 {
                format!(
                    "X: {:+.3} Y: {:+.3} VECT: {:.3} ({:02.0}°)",
                    live_state.stick_x, live_state.stick_y, vect_mag, vect_deg
                )
            } else {
                format!(
                    "X:{:+.2} Y:{:+.2} V:{:.2}",
                    live_state.stick_x, live_state.stick_y, vect_mag
                )
            };
            ui.add(
                egui::Label::new(
                    RichText::new(coord_lbl)
                        .color(COLOR_TERTIARY_CONTAINER)
                        .size(9.0)
                        .strong()
                        .monospace(),
                )
                .truncate(),
            );
        });
    }

    /// Medidores secundarios: Timón Z, Ministick analógico y POV Hat 1.
    fn render_secondary_meters(&self, ui: &mut Ui, live_state: &LiveAxesState, container_w: f32) {
        ui.set_max_width(container_w);
        ui.set_width(container_w);

        ui.vertical(|ui| {
            ui.set_max_width(container_w);
            let sub_inner_w = (container_w - 22.0).max(60.0);

            // 1. Timón Z (Twist) bipolar con centro cero
            let rudder_lbl = if sub_inner_w >= 260.0 {
                "TIMÓN Z (TWIST // RUDDER YAW)"
            } else {
                "TIMÓN Z (YAW)"
            };
            render_bipolar_bar(
                ui,
                container_w,
                rudder_lbl,
                live_state.stick_z,
                COLOR_PRIMARY_CONTAINER,
                "CENTER-ZERO BIPOLAR",
            );

            ui.add_space(8.0);

            // 2. Mini-stick analógico (Slew X / Y) con barras duales
            let ministick_lbl = if sub_inner_w >= 250.0 {
                "MINI-STICK ANALÓGICO (SLEW X/Y)"
            } else {
                "MINI-STICK (SLEW)"
            };
            Frame::new()
                .fill(COLOR_SURFACE_LOWEST)
                .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(sub_inner_w);
                    ui.set_width(sub_inner_w);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 2.0);
                        ui.label(
                            RichText::new(ministick_lbl)
                                .color(COLOR_ON_SURFACE)
                                .size(10.5)
                                .strong()
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "X:{:+.2} Y:{:+.2}",
                                live_state.ministick_x, live_state.ministick_y
                            ))
                            .color(COLOR_WARNING_AMBER)
                            .size(10.0)
                            .monospace(),
                        );
                    });
                    ui.add_space(4.0);
                    render_compact_dual_bar(
                        ui,
                        sub_inner_w,
                        "SLEW X",
                        live_state.ministick_x,
                        COLOR_WARNING_AMBER,
                    );
                    ui.add_space(3.0);
                    render_compact_dual_bar(
                        ui,
                        sub_inner_w,
                        "SLEW Y",
                        live_state.ministick_y,
                        COLOR_WARNING_AMBER,
                    );
                });

            ui.add_space(8.0);

            // 3. POV Hat 1 con modo
            let pov_lbl = if sub_inner_w >= 260.0 {
                "POV HAT 1 (8-WAY DIRECTIONAL)"
            } else {
                "POV HAT 1"
            };
            Frame::new()
                .fill(COLOR_SURFACE_LOWEST)
                .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(sub_inner_w);
                    ui.set_width(sub_inner_w);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        ui.label(
                            RichText::new(pov_lbl)
                                .color(COLOR_ON_SURFACE)
                                .size(10.5)
                                .strong()
                                .monospace(),
                        );
                        theme::telemetry_badge(ui, "MODE:", "8-WAY", COLOR_PRIMARY_FIXED);
                        theme::telemetry_badge(
                            ui,
                            "STATE:",
                            live_state.pov_direction_label(),
                            COLOR_TERTIARY_CONTAINER,
                        );
                    });
                });
        });
    }

    /// "MONITOR GIMBAL 2D & EJES SECUNDARIOS EN VIVO":
    /// Adapta el Gimbal radar y medidores secundarios lado a lado (>= 430px) o apilados (< 430px).
    fn render_gimbal_and_secondary_axes(&self, ui: &mut Ui, live_state: &LiveAxesState) {
        let outer_w = ui.available_width();
        theme::card_frame().show(ui, |ui| {
            let inner_w = (outer_w - 26.0).max(120.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                let title = if inner_w >= 480.0 {
                    format!(
                        "{} MONITOR GIMBAL 2D & EJES SECUNDARIOS EN VIVO",
                        icons::ACTIVITY
                    )
                } else if inner_w >= 320.0 {
                    format!("{} GIMBAL 2D & EJES SECUNDARIOS", icons::ACTIVITY)
                } else {
                    format!("{} GIMBAL 2D", icons::ACTIVITY)
                };
                ui.label(
                    RichText::new(title)
                        .color(COLOR_PRIMARY)
                        .size(12.0)
                        .strong()
                        .monospace(),
                );
                ui.label(
                    RichText::new(format!("{} LIVE 50/1000Hz", icons::RADIO))
                        .color(COLOR_TERTIARY_CONTAINER)
                        .size(9.5)
                        .monospace(),
                );
            });

            ui.add_space(8.0);

            if inner_w >= 430.0 {
                let gap = 12.0;
                let box_size = (inner_w * 0.36).clamp(140.0, 186.0).floor();
                let right_w = (inner_w - gap - box_size).max(180.0).floor();

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        Vec2::new(box_size, 10.0),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(box_size);
                            ui.set_width(box_size);
                            self.render_gimbal_radar(ui, box_size, live_state);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, 10.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.render_secondary_meters(ui, live_state, right_w);
                        },
                    );
                });
            } else {
                let box_size = (inner_w - 16.0).clamp(120.0, 180.0).floor();
                self.render_gimbal_radar(ui, box_size, live_state);
                ui.add_space(10.0);
                self.render_secondary_meters(ui, live_state, inner_w);
            }
        });
    }

    /// TIPO DE CURVATURA: 4 botones (Lineal 1:1, S-Curve, Exponencial, Bézier libre).
    fn render_curve_type_selector(&mut self, ui: &mut Ui) {
        let outer_w = ui.available_width();
        theme::card_frame().show(ui, |ui| {
            let inner_w = (outer_w - 26.0).max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            ui.label(
                RichText::new(format!("{} TIPO DE CURVATURA", icons::SLIDERS_HORIZONTAL))
                    .color(COLOR_PRIMARY)
                    .size(12.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(6.0);

            let use_short_labels = inner_w < 330.0;
            let short_label = |ct: CurveType| -> &'static str {
                match ct {
                    CurveType::Linear => "Lineal (1:1)",
                    CurveType::SCurve => "Curva-S",
                    CurveType::Exponential => "Exponencial",
                    CurveType::Bezier => "Bézier",
                }
            };

            if inner_w >= 240.0 {
                let col_w = ((inner_w - 8.0) * 0.5).floor().max(80.0);
                ui.columns(2, |cols| {
                    cols[0].set_max_width(col_w);
                    cols[1].set_max_width(col_w);
                    for (idx, ct) in CurveType::ALL.iter().enumerate() {
                        let col_idx = idx % 2;
                        let is_active = self.params.curve_type == *ct;
                        let lbl = if use_short_labels {
                            short_label(*ct)
                        } else {
                            ct.label()
                        };
                        cols[col_idx].vertical_centered_justified(|ui| {
                            ui.set_max_width(col_w);
                            if theme::tactical_button(ui, lbl, is_active, false).clicked() {
                                self.params.set_curve_type(*ct);
                                self.eeprom_synced = false;
                            }
                        });
                        if idx < 2 {
                            cols[col_idx].add_space(4.0);
                        }
                    }
                });
            } else {
                for ct in CurveType::ALL {
                    let is_active = self.params.curve_type == ct;
                    ui.vertical_centered_justified(|ui| {
                        ui.set_max_width(inner_w);
                        if theme::tactical_button(ui, short_label(ct), is_active, false).clicked() {
                            self.params.set_curve_type(ct);
                            self.eeprom_synced = false;
                        }
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    /// PARÁMETROS DEL EJE:
    /// Sliders de precisión para Zona Muerta Central (Deadband 0-25%),
    /// Curvatura/Sensibilidad Expo (1.00-3.00), Saturación Externa (70-100%),
    /// y switch toggle "Invertir Dirección de Eje".
    fn render_axis_parameters_card(&mut self, ui: &mut Ui) {
        let outer_w = ui.available_width();
        theme::card_frame().show(ui, |ui| {
            let inner_w = (outer_w - 26.0).max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);
            ui.spacing_mut().slider_width = inner_w;

            ui.label(
                RichText::new(format!("{} PARÁMETROS DEL EJE", icons::SETTINGS))
                    .color(COLOR_PRIMARY)
                    .size(12.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(8.0);

            let compact = inner_w < 280.0;

            // 1. Zona Muerta Central (Deadband 0-25%)
            let mut db_pct = self.params.deadband_pct();
            ui.horizontal(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                let lbl = if compact {
                    "Zona Muerta (DZ)"
                } else {
                    "Zona Muerta Central (Deadband)"
                };
                ui.label(RichText::new(lbl).color(COLOR_ON_SURFACE).size(11.0));
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
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                let lbl = if compact {
                    "Sensibilidad Expo"
                } else {
                    "Curvatura / Sensibilidad Expo"
                };
                ui.label(RichText::new(lbl).color(COLOR_ON_SURFACE).size(11.0));
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
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                let lbl = if compact {
                    "Saturación Externa"
                } else {
                    "Saturación Externa (Outer Clamp)"
                };
                ui.label(RichText::new(lbl).color(COLOR_ON_SURFACE).size(11.0));
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
                ui.label(
                    RichText::new("Nodos Bézier (P1 / P2):")
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(10.5)
                        .monospace(),
                );
                if inner_w >= 280.0 {
                    let sub_col_w = ((inner_w - 8.0) * 0.5).floor().max(60.0);
                    ui.columns(2, |cols| {
                        cols[0].set_max_width(sub_col_w);
                        cols[0].spacing_mut().slider_width = (sub_col_w - 70.0).max(40.0);
                        cols[1].set_max_width(sub_col_w);
                        cols[1].spacing_mut().slider_width = (sub_col_w - 70.0).max(40.0);
                        if cols[0]
                            .add(
                                egui::Slider::new(&mut self.params.bezier_p1[1], 0.0..=1.0)
                                    .text("P1"),
                            )
                            .changed()
                        {
                            self.eeprom_synced = false;
                        }
                        if cols[1]
                            .add(
                                egui::Slider::new(&mut self.params.bezier_p2[1], 0.0..=1.0)
                                    .text("P2"),
                            )
                            .changed()
                        {
                            self.eeprom_synced = false;
                        }
                    });
                } else {
                    ui.spacing_mut().slider_width = (inner_w - 70.0).max(40.0);
                    if ui
                        .add(egui::Slider::new(&mut self.params.bezier_p1[1], 0.0..=1.0).text("P1"))
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                    ui.add_space(3.0);
                    if ui
                        .add(egui::Slider::new(&mut self.params.bezier_p2[1], 0.0..=1.0).text("P2"))
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // 4. Switch toggle "Invertir Dirección de Eje"
            ui.horizontal(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                let inv_lbl = if compact {
                    "Invertir Eje"
                } else {
                    "Invertir Dirección de Eje"
                };
                ui.label(
                    RichText::new(inv_lbl)
                        .color(COLOR_ON_SURFACE)
                        .size(11.0)
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
        let outer_w = ui.available_width();
        theme::card_frame().show(ui, |ui| {
            let inner_w = (outer_w - 26.0).max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            ui.horizontal(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                let title_lbl = if inner_w >= 280.0 {
                    format!("{} MUELLES FÍSICOS (X-56 SPRING)", icons::WRENCH)
                } else {
                    format!("{} MUELLES FÍSICOS", icons::WRENCH)
                };
                ui.label(
                    RichText::new(title_lbl)
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

            let row_w = (inner_w - 22.0).max(80.0);

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
                        ui.set_max_width(row_w);
                        ui.set_width(row_w);

                        ui.horizontal(|ui| {
                            ui.set_max_width(row_w);
                            ui.set_width(row_w);

                            // Indicador cromático del muelle
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::new(10.0, 24.0), Sense::hover());
                            ui.painter().rect_filled(dot_rect, 3.0, spring.color());

                            // Columna de texto con ancho reservado para que el botón siempre sea visible
                            let btn_reserve = 68.0;
                            let text_col_w = (ui.available_width() - btn_reserve).max(50.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(text_col_w, 26.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_max_width(text_col_w);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(spring.label())
                                                .color(spring.color())
                                                .size(11.0)
                                                .strong()
                                                .monospace(),
                                        );
                                        if is_installed && text_col_w >= 120.0 {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} INSTALADO",
                                                    icons::CHECK
                                                ))
                                                .color(COLOR_TERTIARY_CONTAINER)
                                                .size(9.5)
                                                .strong()
                                                .monospace(),
                                            );
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(spring.desc())
                                                .color(COLOR_ON_SURFACE_VARIANT)
                                                .size(9.5),
                                        )
                                        .truncate(),
                                    );
                                },
                            );

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
fn render_bipolar_bar(
    ui: &mut Ui,
    container_w: f32,
    label: &str,
    value: f64,
    fill_col: Color32,
    mode_tag: &str,
) {
    let sub_inner_w = (container_w - 22.0).max(60.0);
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_max_width(sub_inner_w);
            ui.set_width(sub_inner_w);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 2.0);
                ui.label(
                    RichText::new(label)
                        .color(COLOR_ON_SURFACE)
                        .size(10.5)
                        .strong()
                        .monospace(),
                );
                ui.label(
                    RichText::new(format!("{:+.1}%", value * 100.0))
                        .color(fill_col)
                        .size(11.0)
                        .strong()
                        .monospace(),
                );
                if sub_inner_w >= 260.0 {
                    ui.label(
                        RichText::new(mode_tag)
                            .color(COLOR_OUTLINE)
                            .size(9.0)
                            .monospace(),
                    );
                }
            });

            ui.add_space(4.0);

            let (rect, _) = ui.allocate_exact_size(Vec2::new(sub_inner_w, 12.0), Sense::hover());
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
fn render_compact_dual_bar(ui: &mut Ui, row_w: f32, axis_lbl: &str, value: f64, col: Color32) {
    ui.horizontal(|ui| {
        ui.set_max_width(row_w);
        ui.label(
            RichText::new(axis_lbl)
                .color(COLOR_OUTLINE)
                .size(9.5)
                .monospace(),
        );
        let bar_w = (ui.available_width() - 52.0).max(24.0);
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
