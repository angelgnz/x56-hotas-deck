//! Axis response curves, deadband, live transfer HUD, and 2D gimbal telemetry tab.

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use x56_core::{AxisConfig, DeviceKind, X56Device, X56Manager};

use crate::curve_math::{CurveParameters, CurveType, LiveAxesState, LiveInputPoller};
use crate::theme::{self, PhysicalSpring};

/// Virtual UI axis IDs for the analog stick ministick and throttle mini-gimbal.
pub const AXIS_MINISTICK_X: u8 = 0x38;
pub const AXIS_MINISTICK_Y: u8 = 0x39;
pub const AXIS_THROTTLE_MINISTICK_X: u8 = 0x3a;
pub const AXIS_THROTTLE_MINISTICK_Y: u8 = 0x3b;

const DEADBAND_FILL: Color32 = Color32::from_rgba_unmultiplied_const(229, 72, 77, 22);
const DEADBAND_STROKE: Color32 = Color32::from_rgba_unmultiplied_const(229, 72, 77, 96);
const SATURATION_FILL: Color32 = Color32::from_rgba_unmultiplied_const(237, 237, 237, 12);
const CROSSHAIR_GUIDE: Color32 = Color32::from_rgba_unmultiplied_const(76, 195, 138, 100);
const VECTOR_LINE_STROKE: Color32 = Color32::from_rgba_unmultiplied_const(56, 189, 248, 120);

pub struct CurvesTab {
    pub selected_device: DeviceKind,
    pub selected_axis_id: u8,
    pub params: CurveParameters,
    pub hall_zero: u16,
    pub installed_spring: PhysicalSpring,
    pub eeprom_synced: bool,
    pub status_message: Option<(String, bool)>,
    pub stick_usb: Option<(String, u16, u16)>,
    pub throttle_usb: Option<(String, u16, u16)>,
    last_hw_scan: std::time::Instant,
}

impl Default for CurvesTab {
    fn default() -> Self {
        let mut tab = Self {
            selected_device: DeviceKind::Stick,
            selected_axis_id: 0x30,
            params: CurveParameters::default(),
            hall_zero: 0x8000,
            installed_spring: PhysicalSpring::Green47mm,
            eeprom_synced: false,
            status_message: None,
            stick_usb: None,
            throttle_usb: None,
            last_hw_scan: std::time::Instant::now(),
        };
        tab.read_from_hardware();
        tab
    }
}

impl CurvesTab {
    /// Refreshes the cached USB device descriptors using `X56Device::enumerate()`.
    fn refresh_usb_devices(&mut self) {
        let mut stick = None;
        let mut throttle = None;
        for desc in X56Device::enumerate() {
            let entry = (
                desc.path.to_string_lossy().into_owned(),
                desc.vendor_id,
                desc.product_id,
            );
            match desc.kind {
                DeviceKind::Stick if stick.is_none() => stick = Some(entry),
                DeviceKind::Throttle if throttle.is_none() => throttle = Some(entry),
                _ => {}
            }
        }
        self.stick_usb = stick;
        self.throttle_usb = throttle;
        self.last_hw_scan = std::time::Instant::now();
    }

    fn is_virtual_ministick(&self) -> bool {
        (self.selected_device == DeviceKind::Stick
            && matches!(self.selected_axis_id, AXIS_MINISTICK_X | AXIS_MINISTICK_Y))
            || (self.selected_device == DeviceKind::Throttle
                && matches!(
                    self.selected_axis_id,
                    AXIS_THROTTLE_MINISTICK_X | AXIS_THROTTLE_MINISTICK_Y
                ))
    }

    fn is_bipolar_axis(&self) -> bool {
        !(self.selected_device == DeviceKind::Throttle
            && matches!(self.selected_axis_id, 0x30 | 0x31))
    }

    fn localized_device_name(&self) -> String {
        match self.selected_device {
            DeviceKind::Stick => x56_core::t!("curves-stick-short"),
            DeviceKind::Throttle => x56_core::t!("curves-throttle-short"),
        }
    }

    fn axis_adc_spec(&self) -> String {
        match (self.selected_device, self.selected_axis_id) {
            (DeviceKind::Stick, 0x30 | 0x31) => x56_core::t!("curves-adc-16bit"),
            (DeviceKind::Stick, 0x32 | 0x35) => x56_core::t!("curves-adc-12bit"),
            (
                _,
                AXIS_MINISTICK_X
                | AXIS_MINISTICK_Y
                | AXIS_THROTTLE_MINISTICK_X
                | AXIS_THROTTLE_MINISTICK_Y,
            ) => x56_core::t!("curves-adc-8bit"),
            (DeviceKind::Throttle, _) => x56_core::t!("curves-adc-10bit"),
            _ => x56_core::t!("curves-adc-16bit"),
        }
    }

    /// Reads the active axis parameters from hardware EEPROM when connected.
    pub fn read_from_hardware(&mut self) {
        self.refresh_usb_devices();

        if self.is_virtual_ministick() {
            self.eeprom_synced = false;
            self.status_message = Some((x56_core::t!("curves-ministick-no-eeprom"), false));
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
            self.status_message = Some((x56_core::t!("curves-status-eeprom-synced"), false));
        } else {
            self.eeprom_synced = false;
            let dev_str = self.localized_device_name();
            self.status_message = Some((
                x56_core::t!("curves-status-sim-mode", "device" => &dev_str),
                false,
            ));
        }
    }

    /// Resets the active axis parameters to genuine 1:1 linear defaults.
    pub fn reset_current_axis(&mut self) {
        self.params = CurveParameters::linear();
        self.eeprom_synced = false;
        self.status_message = Some((x56_core::t!("curves-status-reset"), false));
    }

    /// Writes the active axis curve parameters to hardware EEPROM.
    pub fn apply_to_hardware(&mut self) {
        self.refresh_usb_devices();

        if self.is_virtual_ministick() {
            self.eeprom_synced = false;
            self.status_message = Some((x56_core::t!("curves-ministick-no-eeprom-apply"), true));
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
            self.status_message = Some((x56_core::t!("curves-params-out-of-range"), true));
            return;
        };

        if let Ok(dev) = X56Device::open(self.selected_device) {
            match dev.set_axis_config(&cfg) {
                Ok(_) => {
                    self.eeprom_synced = true;
                    let mut preview_notes = Vec::new();
                    if self.params.curve_type == CurveType::Bezier {
                        preview_notes.push(x56_core::t!("curves-feature-bezier"));
                    }
                    if self.params.invert {
                        preview_notes.push(x56_core::t!("curves-feature-invert"));
                    }
                    let base = x56_core::t!("curves-applied-success");
                    let msg = if preview_notes.is_empty() {
                        base
                    } else {
                        let sep = format!(" {} ", x56_core::t!("curves-join-and"));
                        let features = preview_notes.join(&sep);
                        x56_core::t!(
                            "curves-applied-preview-note",
                            "base" => &base,
                            "features" => &features
                        )
                    };
                    self.status_message = Some((msg, false));
                }
                Err(err) => {
                    self.eeprom_synced = false;
                    let err_str = err.to_string();
                    self.status_message =
                        Some((x56_core::t!("curves-apply-err", "err" => &err_str), true));
                }
            }
        } else {
            self.eeprom_synced = false;
            let dev_str = self.localized_device_name();
            self.status_message = Some((
                x56_core::t!("curves-no-usb-eeprom", "device" => &dev_str),
                true,
            ));
        }
    }

    /// Returns the normalized physical input and raw scaled ADC sample for the selected axis.
    fn current_live_input(&self, state: &LiveAxesState) -> (f64, u16) {
        match (self.selected_device, self.selected_axis_id) {
            (DeviceKind::Stick, 0x30) => (state.stick_x, state.raw_adc_x),
            (DeviceKind::Stick, 0x31) => (state.stick_y, state.raw_adc_y),
            (DeviceKind::Stick, 0x32 | 0x35) => {
                let adc12 = ((state.stick_z + 1.0) * 0.5 * 4095.0)
                    .round()
                    .clamp(0.0, 4095.0) as u16;
                (state.stick_z, adc12)
            }
            (DeviceKind::Stick, AXIS_MINISTICK_X) => {
                let adc8 = ((state.ministick_x + 1.0) * 0.5 * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u16;
                (state.ministick_x, adc8)
            }
            (DeviceKind::Stick, AXIS_MINISTICK_Y) => {
                let adc8 = ((state.ministick_y + 1.0) * 0.5 * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u16;
                (state.ministick_y, adc8)
            }
            (DeviceKind::Throttle, 0x30) => {
                let adc10 = (state.throttle_left * 1023.0).round().clamp(0.0, 1023.0) as u16;
                (state.throttle_left, adc10)
            }
            (DeviceKind::Throttle, 0x31) => {
                let adc10 = (state.throttle_right * 1023.0).round().clamp(0.0, 1023.0) as u16;
                (state.throttle_right, adc10)
            }
            (DeviceKind::Throttle, 0x32) => {
                let adc10 = ((state.rotary_1 + 1.0) * 0.5 * 1023.0)
                    .round()
                    .clamp(0.0, 1023.0) as u16;
                (state.rotary_1, adc10)
            }
            (DeviceKind::Throttle, 0x35) => {
                let adc10 = ((state.rotary_2 + 1.0) * 0.5 * 1023.0)
                    .round()
                    .clamp(0.0, 1023.0) as u16;
                (state.rotary_2, adc10)
            }
            (DeviceKind::Throttle, 0x36) => {
                let adc10 = ((state.rotary_4 + 1.0) * 0.5 * 1023.0)
                    .round()
                    .clamp(0.0, 1023.0) as u16;
                (state.rotary_4, adc10)
            }
            (DeviceKind::Throttle, 0x37) => {
                let adc10 = ((state.rotary_3 + 1.0) * 0.5 * 1023.0)
                    .round()
                    .clamp(0.0, 1023.0) as u16;
                (state.rotary_3, adc10)
            }
            (DeviceKind::Throttle, AXIS_THROTTLE_MINISTICK_X) => {
                let adc8 = ((state.throttle_ministick_x + 1.0) * 0.5 * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u16;
                (state.throttle_ministick_x, adc8)
            }
            (DeviceKind::Throttle, AXIS_THROTTLE_MINISTICK_Y) => {
                let adc8 = ((state.throttle_ministick_y + 1.0) * 0.5 * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u16;
                (state.throttle_ministick_y, adc8)
            }
            _ => (0.0, 0),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, poller: &LiveInputPoller, monitor: &x56_core::ButtonMonitor) {
        if self.last_hw_scan.elapsed() >= std::time::Duration::from_secs(2) {
            self.refresh_usb_devices();
        }

        let viewport_w = ui.available_width();
        ui.set_max_width(viewport_w);

        let live_state = poller.get_state();

        self.render_hardware_telemetry_bar(ui);
        ui.add_space(theme::SPACE_SM);

        self.render_axis_selection_subbar(ui);
        ui.add_space(theme::SPACE_MD);

        let two_col_mode = viewport_w >= 840.0;

        egui::ScrollArea::vertical()
            .id_salt("curves_body_scroll")
            .max_width(viewport_w)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let body_w = ui.available_width().min(viewport_w);
                ui.set_max_width(body_w);
                ui.set_width(body_w);

                if two_col_mode {
                    let gap = theme::SPACE_MD;
                    let avail_cols = (body_w - gap).max(400.0);
                    let left_width = (avail_cols * 0.56).floor();
                    let right_width = (avail_cols - left_width).floor();

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;

                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width, 10.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_max_width(left_width);
                                ui.set_width(left_width);
                                self.render_transfer_curve_section(ui, &live_state);
                                ui.add_space(theme::SPACE_MD);
                                self.render_telemetry_cards(ui, &live_state);
                                ui.add_space(theme::SPACE_MD);
                                self.render_gimbal_and_secondary_axes(ui, &live_state, monitor);
                            },
                        );

                        ui.allocate_ui_with_layout(
                            Vec2::new(right_width, 10.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_max_width(right_width);
                                ui.set_width(right_width);
                                self.render_curve_type_selector(ui);
                                ui.add_space(theme::SPACE_MD);
                                self.render_axis_parameters_card(ui);
                                ui.add_space(theme::SPACE_MD);
                                self.render_physical_springs_card(ui);
                            },
                        );
                    });
                } else {
                    self.render_transfer_curve_section(ui, &live_state);
                    ui.add_space(theme::SPACE_MD);
                    self.render_telemetry_cards(ui, &live_state);
                    ui.add_space(theme::SPACE_MD);
                    self.render_gimbal_and_secondary_axes(ui, &live_state, monitor);
                    ui.add_space(theme::SPACE_MD);
                    ui.separator();
                    ui.add_space(theme::SPACE_MD);
                    self.render_curve_type_selector(ui);
                    ui.add_space(theme::SPACE_MD);
                    self.render_axis_parameters_card(ui);
                    ui.add_space(theme::SPACE_MD);
                    self.render_physical_springs_card(ui);
                }
            });
    }

    /// Top device bar: segmented Stick / Throttle switcher separated cleanly from quiet hardware metadata.
    fn render_hardware_telemetry_bar(&mut self, ui: &mut Ui) {
        theme::card_frame()
            .inner_margin(Margin::symmetric(
                theme::SPACE_MD as i8,
                theme::SPACE_SM as i8,
            ))
            .show(ui, |ui| {
                let inner_w = ui.available_width().max(100.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_SM, theme::SPACE_SM);

                    ui.label(
                        RichText::new(x56_core::t!("curves-active-device"))
                            .color(theme::COLOR_TEXT_MUTED)
                            .size(theme::FONT_META),
                    );

                    let compact = inner_w < 580.0;
                    let is_stick = self.selected_device == DeviceKind::Stick;
                    let stick_lbl = if compact {
                        x56_core::t!("curves-stick-short")
                    } else {
                        x56_core::t!("curves-stick-rhino")
                    };
                    if theme::segmented_button(ui, &stick_lbl, is_stick).clicked() && !is_stick {
                        self.selected_device = DeviceKind::Stick;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    let is_throttle = self.selected_device == DeviceKind::Throttle;
                    let thr_lbl = if compact {
                        x56_core::t!("curves-throttle-short")
                    } else {
                        x56_core::t!("curves-throttle-rhino")
                    };
                    if theme::segmented_button(ui, &thr_lbl, is_throttle).clicked() && !is_throttle
                    {
                        self.selected_device = DeviceKind::Throttle;
                        self.selected_axis_id = 0x30;
                        self.read_from_hardware();
                    }

                    ui.add_space(theme::SPACE_SM);

                    let adc_val = self.axis_adc_spec();
                    theme::meta_kv(
                        ui,
                        &x56_core::t!("curves-adc-label"),
                        &adc_val,
                        theme::COLOR_TEXT_SECONDARY,
                    );

                    let active_usb = match self.selected_device {
                        DeviceKind::Stick => self.stick_usb.as_ref(),
                        DeviceKind::Throttle => self.throttle_usb.as_ref(),
                    };
                    let offline_lbl = x56_core::t!("header-offline");
                    let (node_str, node_col) = match active_usb {
                        Some((path, vid, pid)) => {
                            if inner_w >= 920.0 {
                                (
                                    format!("{path} · {vid:04X}:{pid:04X}"),
                                    theme::COLOR_TEXT_PRIMARY,
                                )
                            } else {
                                (path.clone(), theme::COLOR_TEXT_PRIMARY)
                            }
                        }
                        None => (offline_lbl, theme::COLOR_TEXT_MUTED),
                    };
                    theme::meta_kv(ui, &x56_core::t!("curves-node-label"), &node_str, node_col);

                    let (sync_label, sync_dot_col, sync_text_col) =
                        match (self.eeprom_synced, inner_w >= 520.0) {
                            (true, true) => (
                                x56_core::t!("curves-eeprom-synced"),
                                theme::COLOR_POSITIVE,
                                theme::COLOR_TEXT_SECONDARY,
                            ),
                            (true, false) => (
                                x56_core::t!("curves-synced-short"),
                                theme::COLOR_POSITIVE,
                                theme::COLOR_TEXT_SECONDARY,
                            ),
                            (false, true) => (
                                x56_core::t!("curves-eeprom-modified"),
                                theme::COLOR_WARNING,
                                theme::COLOR_WARNING,
                            ),
                            (false, false) => (
                                x56_core::t!("curves-modified-short"),
                                theme::COLOR_WARNING,
                                theme::COLOR_WARNING,
                            ),
                        };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_XS;
                        theme::status_dot(ui, sync_dot_col, 7.0);
                        ui.label(
                            RichText::new(sync_label)
                                .color(sync_text_col)
                                .size(theme::FONT_META),
                        );
                    });
                });
            });
    }

    /// Axis selection sub-bar: segmented buttons per device axis + quiet curve profile metadata.
    fn render_axis_selection_subbar(&mut self, ui: &mut Ui) {
        theme::card_frame()
            .inner_margin(Margin::symmetric(
                theme::SPACE_MD as i8,
                theme::SPACE_SM as i8,
            ))
            .show(ui, |ui| {
                let inner_w = ui.available_width().max(100.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_XS, theme::SPACE_XS);
                    ui.label(
                        RichText::new(x56_core::t!("curves-active-axis"))
                            .color(theme::COLOR_TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                    ui.add_space(theme::SPACE_XS);

                    let compact = inner_w < 580.0;
                    let axes: Vec<(u8, String)> = match (self.selected_device, compact) {
                        (DeviceKind::Stick, false) => vec![
                            (0x30, x56_core::t!("curves-axis-x")),
                            (0x31, x56_core::t!("curves-axis-y")),
                            (0x35, x56_core::t!("curves-axis-z")),
                            (AXIS_MINISTICK_X, x56_core::t!("curves-axis-mini-x")),
                            (AXIS_MINISTICK_Y, x56_core::t!("curves-axis-mini-y")),
                        ],
                        (DeviceKind::Stick, true) => vec![
                            (0x30, x56_core::t!("curves-axis-x-short")),
                            (0x31, x56_core::t!("curves-axis-y-short")),
                            (0x35, x56_core::t!("curves-axis-z-short")),
                            (AXIS_MINISTICK_X, x56_core::t!("curves-axis-mini-x-short")),
                            (AXIS_MINISTICK_Y, x56_core::t!("curves-axis-mini-y-short")),
                        ],
                        (DeviceKind::Throttle, false) => vec![
                            (0x30, x56_core::t!("curves-axis-tl")),
                            (0x31, x56_core::t!("curves-axis-tr")),
                            (0x32, x56_core::t!("curves-axis-r1")),
                            (0x35, x56_core::t!("curves-axis-r2")),
                            (0x37, x56_core::t!("curves-axis-r3")),
                            (0x36, x56_core::t!("curves-axis-r4")),
                            (
                                AXIS_THROTTLE_MINISTICK_X,
                                x56_core::t!("curves-axis-thr-mini-x"),
                            ),
                            (
                                AXIS_THROTTLE_MINISTICK_Y,
                                x56_core::t!("curves-axis-thr-mini-y"),
                            ),
                        ],
                        (DeviceKind::Throttle, true) => vec![
                            (0x30, x56_core::t!("curves-axis-tl-short")),
                            (0x31, x56_core::t!("curves-axis-tr-short")),
                            (0x32, x56_core::t!("curves-axis-r1-short")),
                            (0x35, x56_core::t!("curves-axis-r2-short")),
                            (0x37, x56_core::t!("curves-axis-r3-short")),
                            (0x36, x56_core::t!("curves-axis-r4-short")),
                            (
                                AXIS_THROTTLE_MINISTICK_X,
                                x56_core::t!("curves-axis-mini-x-short"),
                            ),
                            (
                                AXIS_THROTTLE_MINISTICK_Y,
                                x56_core::t!("curves-axis-mini-y-short"),
                            ),
                        ],
                    };

                    for (id, label) in axes {
                        let is_active = self.selected_axis_id == id;
                        if theme::segmented_button(ui, &label, is_active).clicked() && !is_active {
                            self.selected_axis_id = id;
                            self.read_from_hardware();
                        }
                    }

                    ui.add_space(theme::SPACE_SM);
                    ui.label(
                        RichText::new(self.params.curve_type.polynomial_badge())
                            .color(theme::COLOR_TEXT_MUTED)
                            .size(theme::FONT_META)
                            .monospace(),
                    );
                });
            });
    }

    /// Hero Moment 1: Transfer Response Curve inside `card_frame` -> `canvas_frame`.
    fn render_transfer_curve_section(&mut self, ui: &mut Ui, live_state: &LiveAxesState) {
        let (live_in, _raw_adc) = self.current_live_input(live_state);
        let live_out = self.params.evaluate(live_in);
        let is_bipolar = self.is_bipolar_axis();

        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(120.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            let title = if inner_w >= 340.0 {
                x56_core::t!("curves-transfer-title")
            } else {
                x56_core::t!("curves-transfer-short")
            };
            let live_readout = format!(
                "IN {:+5.1}%  OUT {:+5.1}%",
                live_in * 100.0,
                live_out * 100.0
            );

            ui.horizontal(|ui| {
                ui.set_width(inner_w);
                theme::section_heading(ui, &title);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&live_readout)
                            .color(theme::COLOR_TEXT_SECONDARY)
                            .size(theme::FONT_META)
                            .monospace(),
                    );
                });
            });

            ui.add_space(theme::SPACE_XS);
            ui.label(
                RichText::new(format!(
                    "{} {:.1}% · Expo {:.2} · Sat {:.1}%",
                    x56_core::t!("curves-deadband-short"),
                    self.params.deadband_pct(),
                    self.params.expo_factor(),
                    self.params.saturation_pct()
                ))
                .color(theme::COLOR_TEXT_MUTED)
                .size(theme::FONT_META)
                .monospace(),
            );

            ui.add_space(theme::SPACE_SM);

            theme::canvas_frame().show(ui, |ui| {
                let canvas_width = ui.available_width().max(100.0);
                let canvas_height = if canvas_width >= 360.0 {
                    (canvas_width * 0.72).clamp(280.0, 380.0)
                } else {
                    (canvas_width * 0.78).clamp(220.0, 320.0)
                };
                let (response, painter) =
                    ui.allocate_painter(Vec2::new(canvas_width, canvas_height), Sense::hover());
                let rect = response.rect;
                let painter = painter.with_clip_rect(rect);

                painter.rect_filled(rect, 0.0, theme::COLOR_SURFACE_LOWEST);

                let pad_left = 48.0;
                let pad_right = 24.0;
                let pad_top = 12.0;
                let pad_bottom = 24.0;
                let plot_rect = Rect::from_min_max(
                    Pos2::new(rect.min.x + pad_left, rect.min.y + pad_top),
                    Pos2::new(rect.max.x - pad_right, rect.max.y - pad_bottom),
                );

                let (x_min, x_max) = if is_bipolar { (-1.0, 1.0) } else { (0.0, 1.0) };
                let (y_min, y_max) = if is_bipolar {
                    (-1.0, 1.0)
                } else if self.params.invert {
                    (-1.0, 0.0)
                } else {
                    (0.0, 1.0)
                };

                let to_screen = |nx: f64, ny: f64| -> Pos2 {
                    let tx = ((nx - x_min) / (x_max - x_min)) as f32;
                    let ty = ((ny - y_min) / (y_max - y_min)) as f32;
                    let sx = plot_rect.min.x + tx * plot_rect.width();
                    let sy = plot_rect.max.y - ty * plot_rect.height();
                    Pos2::new(sx, sy)
                };

                // 1. Subtle 5% hairline gridlines and quiet 12px mono axis ticks
                let x_steps: &[f64] = if is_bipolar {
                    &[-1.0, -0.5, 0.0, 0.5, 1.0]
                } else {
                    &[0.0, 0.25, 0.5, 0.75, 1.0]
                };
                let y_steps: &[f64] = if is_bipolar {
                    &[-1.0, -0.5, 0.0, 0.5, 1.0]
                } else if self.params.invert {
                    &[-1.0, -0.75, -0.5, -0.25, 0.0]
                } else {
                    &[0.0, 0.25, 0.5, 0.75, 1.0]
                };

                let minor_stroke = Stroke::new(1.0, theme::COLOR_BORDER_SUBTLE);
                let major_stroke = Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE);

                if is_bipolar {
                    for &sub in &[-0.75, -0.25, 0.25, 0.75] {
                        let sx = to_screen(sub, 0.0).x;
                        let sy = to_screen(0.0, sub).y;
                        painter.line_segment(
                            [
                                Pos2::new(sx, plot_rect.min.y),
                                Pos2::new(sx, plot_rect.max.y),
                            ],
                            minor_stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(plot_rect.min.x, sy),
                                Pos2::new(plot_rect.max.x, sy),
                            ],
                            minor_stroke,
                        );
                    }
                }

                for &gx in x_steps {
                    let px = to_screen(gx, y_min);
                    let stroke = if gx == 0.0 && is_bipolar {
                        major_stroke
                    } else {
                        minor_stroke
                    };
                    painter.line_segment(
                        [
                            Pos2::new(px.x, plot_rect.min.y),
                            Pos2::new(px.x, plot_rect.max.y),
                        ],
                        stroke,
                    );
                    let lbl = format_tick_pct(gx);
                    painter.text(
                        Pos2::new(px.x, plot_rect.max.y + 12.0),
                        egui::Align2::CENTER_CENTER,
                        &lbl,
                        FontId::monospace(theme::FONT_META),
                        theme::COLOR_TEXT_MUTED,
                    );
                }

                for &gy in y_steps {
                    let py = to_screen(x_min, gy);
                    let stroke = if gy == 0.0 && is_bipolar {
                        major_stroke
                    } else {
                        minor_stroke
                    };
                    painter.line_segment(
                        [
                            Pos2::new(plot_rect.min.x, py.y),
                            Pos2::new(plot_rect.max.x, py.y),
                        ],
                        stroke,
                    );
                    let lbl = format_tick_pct(gy);
                    painter.text(
                        Pos2::new(plot_rect.min.x - 6.0, py.y),
                        egui::Align2::RIGHT_CENTER,
                        &lbl,
                        FontId::monospace(theme::FONT_META),
                        theme::COLOR_TEXT_MUTED,
                    );
                }

                // 2. Outer saturation region (only when sat_norm < 0.999)
                let sat_norm = ((self.params.xsat as f64) / 1000.0).clamp(0.1, 1.0);
                if sat_norm < 0.999 {
                    let sat_stroke = Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE);
                    if is_bipolar {
                        let left_sat_x = to_screen(-sat_norm, y_min).x;
                        let left_rect = Rect::from_min_max(
                            plot_rect.min,
                            Pos2::new(left_sat_x, plot_rect.max.y),
                        );
                        painter.rect_filled(left_rect, 0.0, SATURATION_FILL);
                        painter.line_segment(
                            [
                                Pos2::new(left_sat_x, plot_rect.min.y),
                                Pos2::new(left_sat_x, plot_rect.max.y),
                            ],
                            sat_stroke,
                        );
                    }
                    let right_sat_x = to_screen(sat_norm, y_min).x;
                    let right_rect =
                        Rect::from_min_max(Pos2::new(right_sat_x, plot_rect.min.y), plot_rect.max);
                    painter.rect_filled(right_rect, 0.0, SATURATION_FILL);
                    painter.line_segment(
                        [
                            Pos2::new(right_sat_x, plot_rect.min.y),
                            Pos2::new(right_sat_x, plot_rect.max.y),
                        ],
                        sat_stroke,
                    );
                }

                // 3. Deadband region (subtle fill + 1px hairline border)
                let dz_norm = ((self.params.deadband as f64) / 1000.0).clamp(0.0, 0.5);
                if dz_norm > 0.002 {
                    let dz_left_val = if is_bipolar { -dz_norm } else { 0.0 };
                    let dz_left = to_screen(dz_left_val, y_min).x;
                    let dz_right = to_screen(dz_norm, y_min).x;
                    let dz_rect = Rect::from_min_max(
                        Pos2::new(dz_left, plot_rect.min.y),
                        Pos2::new(dz_right, plot_rect.max.y),
                    );
                    painter.rect_filled(dz_rect, 0.0, DEADBAND_FILL);

                    let dz_border = Stroke::new(1.0, DEADBAND_STROKE);
                    if is_bipolar {
                        painter.line_segment(
                            [
                                Pos2::new(dz_left, plot_rect.min.y),
                                Pos2::new(dz_left, plot_rect.max.y),
                            ],
                            dz_border,
                        );
                    }
                    painter.line_segment(
                        [
                            Pos2::new(dz_right, plot_rect.min.y),
                            Pos2::new(dz_right, plot_rect.max.y),
                        ],
                        dz_border,
                    );
                }

                // 4. Crisp 1:1 dashed reference line in COLOR_TEXT_MUTED
                let (ref_start, ref_end) = if is_bipolar {
                    (to_screen(-1.0, -1.0), to_screen(1.0, 1.0))
                } else if self.params.invert {
                    (to_screen(0.0, 0.0), to_screen(1.0, -1.0))
                } else {
                    (to_screen(0.0, 0.0), to_screen(1.0, 1.0))
                };
                draw_dashed_line(
                    &painter,
                    ref_start,
                    ref_end,
                    Stroke::new(1.0, theme::COLOR_TEXT_MUTED),
                    6.0,
                    5.0,
                );

                // 5. Single clean 2.2px calibrated Instrument Cyan curve
                let plot_pts = self.params.generate_plot_points(is_bipolar, 140);
                let polyline: Vec<Pos2> = plot_pts
                    .into_iter()
                    .map(|[nx, ny]| to_screen(nx, ny))
                    .collect();
                painter.add(egui::Shape::line(
                    polyline,
                    Stroke::new(2.2, theme::COLOR_ACCENT),
                ));

                // 6. Live operating point: 1px crosshair guides + single 4px positive dot
                let clamped_in = live_in.clamp(x_min, x_max);
                let live_pos = to_screen(clamped_in, live_out.clamp(y_min, y_max));
                let guide_stroke = Stroke::new(1.0, CROSSHAIR_GUIDE);
                draw_dashed_line(
                    &painter,
                    Pos2::new(live_pos.x, plot_rect.min.y),
                    Pos2::new(live_pos.x, plot_rect.max.y),
                    guide_stroke,
                    4.0,
                    4.0,
                );
                draw_dashed_line(
                    &painter,
                    Pos2::new(plot_rect.min.x, live_pos.y),
                    Pos2::new(plot_rect.max.x, live_pos.y),
                    guide_stroke,
                    4.0,
                    4.0,
                );
                painter.circle_filled(live_pos, 4.0, theme::COLOR_POSITIVE);

                painter.rect_stroke(
                    plot_rect,
                    0.0,
                    Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
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
    ) {
        ui.set_max_width(col_w);
        theme::card_frame()
            .inner_margin(Margin::same(theme::SPACE_MD as i8))
            .show(ui, |ui| {
                let inner_w = ui.available_width().max(40.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);
                ui.set_min_height(64.0);

                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .color(theme::COLOR_TEXT_MUTED)
                            .size(theme::FONT_META),
                    )
                    .truncate(),
                );
                ui.add_space(theme::SPACE_XS);
                let figure_size = if inner_w >= 115.0 {
                    theme::FONT_FIGURE
                } else {
                    20.0
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(main_val)
                            .color(theme::COLOR_TEXT_PRIMARY)
                            .size(figure_size)
                            .monospace(),
                    )
                    .truncate(),
                );
                ui.add_space(theme::SPACE_XS);
                ui.add(
                    egui::Label::new(
                        RichText::new(sub_val)
                            .color(theme::COLOR_TEXT_SECONDARY)
                            .size(theme::FONT_META)
                            .monospace(),
                    )
                    .truncate(),
                );
            });
    }

    /// Four equal-width/equal-height telemetry stat cards in neutral primary text.
    fn render_telemetry_cards(&self, ui: &mut Ui, live_state: &LiveAxesState) {
        let (live_in, raw_adc) = self.current_live_input(live_state);
        let live_out = self.params.evaluate(live_in);
        let comp_rate = self.params.compression_rate(live_in);

        let avail_w = ui.available_width();
        let use_four_cols = avail_w >= 640.0;
        let col_w = if use_four_cols {
            ((avail_w - theme::SPACE_SM * 3.0) / 4.0).floor().max(80.0)
        } else if avail_w >= 280.0 {
            ((avail_w - theme::SPACE_SM) * 0.5).floor().max(80.0)
        } else {
            avail_w.max(80.0)
        };
        let is_wide_card = col_w >= 165.0;

        let title_0 = if is_wide_card {
            x56_core::t!("curves-stat-raw")
        } else {
            x56_core::t!("curves-stat-raw-short")
        };
        let title_1 = if is_wide_card {
            x56_core::t!("curves-stat-deflection")
        } else {
            x56_core::t!("curves-stat-deflection-short")
        };
        let title_2 = if is_wide_card {
            x56_core::t!("curves-stat-output")
        } else {
            x56_core::t!("curves-stat-output-short")
        };
        let title_3 = if is_wide_card {
            x56_core::t!("curves-stat-gain")
        } else {
            x56_core::t!("curves-stat-gain-short")
        };

        let in_pct = format!("{:+5.1}", live_in * 100.0);
        let out_pct = format!("{:+5.1}", live_out * 100.0);

        let cards: [(String, String, String); 4] = [
            (
                title_0,
                format!("0x{:04X}", raw_adc),
                format!("{raw_adc} LSB"),
            ),
            (
                title_1,
                format!("{:+.3}", live_in),
                x56_core::t!("curves-stat-input-pct", "val" => &in_pct),
            ),
            (
                title_2,
                format!("{:+.3}", live_out),
                x56_core::t!("curves-stat-output-pct", "val" => &out_pct),
            ),
            (
                title_3,
                format!("{:.2}x", comp_rate),
                x56_core::t!("curves-stat-gain-sub"),
            ),
        ];

        if use_four_cols {
            ui.columns(4, |cols| {
                for (idx, (title, main_val, sub_val)) in cards.iter().enumerate() {
                    let c_w = cols[idx].available_width().min(col_w);
                    Self::render_single_telemetry_card(
                        &mut cols[idx],
                        c_w,
                        title,
                        main_val,
                        sub_val,
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
                    &cards[0].0,
                    &cards[0].1,
                    &cards[0].2,
                );
                Self::render_single_telemetry_card(
                    &mut cols[1],
                    c1_w,
                    &cards[1].0,
                    &cards[1].1,
                    &cards[1].2,
                );
            });
            ui.add_space(theme::SPACE_SM);
            ui.columns(2, |cols| {
                let c0_w = cols[0].available_width().min(col_w);
                let c1_w = cols[1].available_width().min(col_w);
                Self::render_single_telemetry_card(
                    &mut cols[0],
                    c0_w,
                    &cards[2].0,
                    &cards[2].1,
                    &cards[2].2,
                );
                Self::render_single_telemetry_card(
                    &mut cols[1],
                    c1_w,
                    &cards[3].0,
                    &cards[3].1,
                    &cards[3].2,
                );
            });
        } else {
            for (title, main_val, sub_val) in &cards {
                Self::render_single_telemetry_card(ui, col_w, title, main_val, sub_val);
                ui.add_space(theme::SPACE_SM);
            }
        }
    }

    /// 2D Gimbal radar on `#0f0f10` with subtle 50%/100% rings, deadband circle, and single live dot.
    fn render_gimbal_radar(
        &self,
        ui: &mut Ui,
        box_size: f32,
        live_state: &LiveAxesState,
        is_pushed: Option<bool>,
    ) {
        ui.vertical_centered(|ui| {
            let (resp, painter) =
                ui.allocate_painter(Vec2::new(box_size, box_size), Sense::hover());
            let r = resp.rect;

            painter.rect_filled(r, theme::RADIUS_CONTROL as f32, theme::COLOR_SURFACE_LOWEST);
            painter.rect_stroke(
                r,
                theme::RADIUS_CONTROL as f32,
                Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                StrokeKind::Inside,
            );

            let center = r.center();
            let half = (box_size * 0.5) - 10.0;

            painter.circle_stroke(center, half, Stroke::new(1.0, theme::COLOR_GRID_HUD));
            painter.circle_stroke(center, half * 0.5, Stroke::new(1.0, theme::COLOR_GRID_HUD));

            painter.line_segment(
                [
                    Pos2::new(r.min.x + 8.0, center.y),
                    Pos2::new(r.max.x - 8.0, center.y),
                ],
                Stroke::new(1.0, theme::COLOR_BORDER_SUBTLE),
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, r.min.y + 8.0),
                    Pos2::new(center.x, r.max.y - 8.0),
                ],
                Stroke::new(1.0, theme::COLOR_BORDER_SUBTLE),
            );

            let dz_norm = (self.params.deadband as f32) / 1000.0;
            if dz_norm > 0.002 {
                let dz_radius = (dz_norm * half).clamp(3.0, half * 0.6);
                painter.circle_filled(center, dz_radius, DEADBAND_FILL);
                painter.circle_stroke(center, dz_radius, Stroke::new(1.0, DEADBAND_STROKE));
            }

            let (gx, gy, vect_mag, vect_deg) = if self.selected_device == DeviceKind::Throttle {
                let (mag, deg) = live_state.throttle_gimbal_vector();
                (
                    live_state.throttle_ministick_x as f32,
                    live_state.throttle_ministick_y as f32,
                    mag,
                    deg,
                )
            } else {
                let (mag, deg) = live_state.gimbal_vector();
                (
                    live_state.stick_x as f32,
                    live_state.stick_y as f32,
                    mag,
                    deg,
                )
            };

            let dot_pos = Pos2::new(center.x + gx * half, center.y - gy * half);

            painter.line_segment([center, dot_pos], Stroke::new(1.0, VECTOR_LINE_STROKE));

            let dot_col = if is_pushed == Some(true) {
                theme::COLOR_POSITIVE
            } else {
                theme::COLOR_ACCENT
            };
            painter.circle_filled(dot_pos, 4.5, dot_col);

            ui.add_space(theme::SPACE_SM);
            ui.label(
                RichText::new(format!("X {:+.3}  Y {:+.3}", gx, gy))
                    .color(theme::COLOR_TEXT_PRIMARY)
                    .size(theme::FONT_META)
                    .monospace(),
            );
            ui.label(
                RichText::new(format!("Mag {:.3}  {:03.0}°", vect_mag, vect_deg))
                    .color(theme::COLOR_TEXT_SECONDARY)
                    .size(theme::FONT_META)
                    .monospace(),
            );

            if let Some(pushed) = is_pushed {
                ui.add_space(theme::SPACE_XS);
                let push_state = if pushed {
                    x56_core::t!("curves-push-active")
                } else {
                    x56_core::t!("curves-push-idle")
                };
                let push_col = if pushed {
                    theme::COLOR_POSITIVE
                } else {
                    theme::COLOR_TEXT_MUTED
                };
                theme::meta_kv(
                    ui,
                    &x56_core::t!("curves-push-label"),
                    &push_state,
                    push_col,
                );
            }
        });
    }

    /// Secondary axis meters for Throttle: Dual Throttle L/R, Mini-Gimbal, and Rotaries R1..R4.
    fn render_throttle_secondary_meters(
        &self,
        ui: &mut Ui,
        live_state: &LiveAxesState,
        container_w: f32,
        is_pushed: bool,
    ) {
        ui.set_max_width(container_w);
        ui.set_width(container_w);

        ui.vertical(|ui| {
            ui.set_max_width(container_w);

            let thr_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-dual-throttle")
            } else {
                x56_core::t!("curves-dual-throttle-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.label(
                    RichText::new(thr_lbl)
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_SM);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-tl-short"),
                    live_state.throttle_left,
                    false,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-tr-short"),
                    live_state.throttle_right,
                    false,
                    theme::COLOR_ACCENT,
                );
            });

            ui.add_space(theme::SPACE_SM);

            let ministick_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-mini-gimbal-title")
            } else {
                x56_core::t!("curves-mini-gimbal-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal(|ui| {
                    ui.set_width(inner_w);
                    ui.label(
                        RichText::new(ministick_lbl)
                            .color(theme::COLOR_TEXT_PRIMARY)
                            .size(theme::FONT_SMALL),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let push_state = if is_pushed {
                            x56_core::t!("curves-push-active")
                        } else {
                            x56_core::t!("curves-push-idle")
                        };
                        let push_col = if is_pushed {
                            theme::COLOR_POSITIVE
                        } else {
                            theme::COLOR_TEXT_MUTED
                        };
                        theme::meta_kv(
                            ui,
                            &x56_core::t!("curves-push-label"),
                            &push_state,
                            push_col,
                        );
                    });
                });
                ui.add_space(theme::SPACE_SM);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-mini-x-short"),
                    live_state.throttle_ministick_x,
                    true,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-mini-y-short"),
                    live_state.throttle_ministick_y,
                    true,
                    theme::COLOR_ACCENT,
                );
            });

            ui.add_space(theme::SPACE_SM);

            let rot_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-rotaries-title")
            } else {
                x56_core::t!("curves-rotaries-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.label(
                    RichText::new(rot_lbl)
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_SM);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-r1-short"),
                    live_state.rotary_1,
                    true,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-r2-short"),
                    live_state.rotary_2,
                    true,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-r3-short"),
                    live_state.rotary_3,
                    true,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-r4-short"),
                    live_state.rotary_4,
                    true,
                    theme::COLOR_ACCENT,
                );
            });
        });
    }

    /// Secondary axis meters for Stick: Yaw Z, Analog Ministick, and POV Hat 1.
    fn render_secondary_meters(&self, ui: &mut Ui, live_state: &LiveAxesState, container_w: f32) {
        ui.set_max_width(container_w);
        ui.set_width(container_w);

        ui.vertical(|ui| {
            ui.set_max_width(container_w);

            let rudder_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-yaw-title")
            } else {
                x56_core::t!("curves-yaw-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.horizontal(|ui| {
                    ui.set_width(inner_w);
                    ui.label(
                        RichText::new(rudder_lbl)
                            .color(theme::COLOR_TEXT_PRIMARY)
                            .size(theme::FONT_SMALL),
                    );
                    if inner_w >= 250.0 {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(x56_core::t!("curves-yaw-bipolar"))
                                    .color(theme::COLOR_TEXT_MUTED)
                                    .size(theme::FONT_META),
                            );
                        });
                    }
                });
                ui.add_space(theme::SPACE_SM);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-z-short"),
                    live_state.stick_z,
                    true,
                    theme::COLOR_ACCENT,
                );
            });

            ui.add_space(theme::SPACE_SM);

            let ministick_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-ministick-title")
            } else {
                x56_core::t!("curves-ministick-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.label(
                    RichText::new(ministick_lbl)
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_SM);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-mini-x-short"),
                    live_state.ministick_x,
                    true,
                    theme::COLOR_ACCENT,
                );
                ui.add_space(theme::SPACE_XS);
                render_aligned_axis_bar(
                    ui,
                    &x56_core::t!("curves-axis-mini-y-short"),
                    live_state.ministick_y,
                    true,
                    theme::COLOR_ACCENT,
                );
            });

            ui.add_space(theme::SPACE_SM);

            let pov_lbl = if container_w >= 260.0 {
                x56_core::t!("curves-pov-title")
            } else {
                x56_core::t!("curves-pov-short")
            };
            theme::subcard_frame().show(ui, |ui| {
                let inner_w = ui.available_width().max(80.0);
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                ui.label(
                    RichText::new(pov_lbl)
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_XS);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_MD, theme::SPACE_XS);
                    theme::meta_kv(
                        ui,
                        &x56_core::t!("curves-pov-mode"),
                        &x56_core::t!("curves-pov-8way"),
                        theme::COLOR_TEXT_SECONDARY,
                    );
                    let pov_centered = live_state.pov_x == 0 && live_state.pov_y == 0;
                    let pov_state_str = if pov_centered {
                        x56_core::t!("curves-pov-center")
                    } else {
                        live_state.pov_direction_label().to_owned()
                    };
                    let pov_col = if pov_centered {
                        theme::COLOR_TEXT_MUTED
                    } else {
                        theme::COLOR_POSITIVE
                    };
                    theme::meta_kv(
                        ui,
                        &x56_core::t!("curves-pov-state"),
                        &pov_state_str,
                        pov_col,
                    );
                });
            });
        });
    }

    /// 2D Gimbal Monitor & Secondary Axes container with genuine hardware connection status.
    fn render_gimbal_and_secondary_axes(
        &self,
        ui: &mut Ui,
        live_state: &LiveAxesState,
        monitor: &x56_core::ButtonMonitor,
    ) {
        let snapshot = monitor.snapshot();
        let is_thr_pushed = snapshot.pressed_buttons.contains("thr_ministick_push");
        let is_connected = match self.selected_device {
            DeviceKind::Stick => live_state.stick_connected,
            DeviceKind::Throttle => live_state.throttle_connected,
        };

        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(120.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            let title = match (self.selected_device, inner_w >= 430.0) {
                (DeviceKind::Throttle, true) => x56_core::t!("curves-monitor-thr"),
                (DeviceKind::Throttle, false) => x56_core::t!("curves-monitor-thr-short"),
                (DeviceKind::Stick, true) => x56_core::t!("curves-monitor-stick"),
                (DeviceKind::Stick, false) => x56_core::t!("curves-monitor-stick-short"),
            };

            ui.horizontal(|ui| {
                ui.set_width(inner_w);
                theme::section_heading(ui, &title);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (dot_col, status_txt, txt_col) = if is_connected {
                        (
                            theme::COLOR_POSITIVE,
                            format!("{} · 50 Hz", x56_core::t!("curves-live-badge")),
                            theme::COLOR_TEXT_SECONDARY,
                        )
                    } else {
                        (
                            theme::COLOR_TEXT_MUTED,
                            x56_core::t!("header-offline"),
                            theme::COLOR_TEXT_MUTED,
                        )
                    };
                    ui.label(
                        RichText::new(status_txt)
                            .color(txt_col)
                            .size(theme::FONT_META)
                            .monospace(),
                    );
                    theme::status_dot(ui, dot_col, 7.0);
                });
            });

            ui.add_space(theme::SPACE_MD);

            let push_opt = if self.selected_device == DeviceKind::Throttle {
                Some(is_thr_pushed)
            } else {
                None
            };

            if inner_w >= 440.0 {
                let gap = theme::SPACE_MD;
                let box_size = (inner_w * 0.36).clamp(160.0, 188.0).floor();
                let right_w = (inner_w - gap - box_size).max(180.0).floor();

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        Vec2::new(box_size, 10.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.set_max_width(box_size);
                            ui.set_width(box_size);
                            self.render_gimbal_radar(ui, box_size, live_state, push_opt);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, 10.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            if self.selected_device == DeviceKind::Throttle {
                                self.render_throttle_secondary_meters(
                                    ui,
                                    live_state,
                                    right_w,
                                    is_thr_pushed,
                                );
                            } else {
                                self.render_secondary_meters(ui, live_state, right_w);
                            }
                        },
                    );
                });
            } else {
                let box_size = (inner_w - 16.0).clamp(150.0, 188.0).floor();
                self.render_gimbal_radar(ui, box_size, live_state, push_opt);
                ui.add_space(theme::SPACE_MD);
                if self.selected_device == DeviceKind::Throttle {
                    self.render_throttle_secondary_meters(ui, live_state, inner_w, is_thr_pushed);
                } else {
                    self.render_secondary_meters(ui, live_state, inner_w);
                }
            }
        });
    }

    /// Curve profile selector: clean 2x2 segmented button grid + quiet mono badge text.
    fn render_curve_type_selector(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            theme::section_heading(ui, &x56_core::t!("curves-type-heading"));
            ui.add_space(theme::SPACE_SM);

            if inner_w >= 240.0 {
                let col_w = ((inner_w - theme::SPACE_SM) * 0.5).floor().max(80.0);
                ui.columns(2, |cols| {
                    cols[0].set_max_width(col_w);
                    cols[1].set_max_width(col_w);
                    for (idx, ct) in CurveType::ALL.iter().enumerate() {
                        let col_idx = idx % 2;
                        let is_active = self.params.curve_type == *ct;
                        let lbl = ct.label();
                        cols[col_idx].vertical_centered_justified(|ui| {
                            ui.set_max_width(col_w);
                            if theme::segmented_button(ui, &lbl, is_active).clicked() {
                                self.params.set_curve_type(*ct);
                                self.eeprom_synced = false;
                            }
                        });
                        if idx < 2 {
                            cols[col_idx].add_space(theme::SPACE_SM);
                        }
                    }
                });
            } else {
                for ct in CurveType::ALL {
                    let is_active = self.params.curve_type == ct;
                    let lbl = ct.label();
                    ui.vertical_centered_justified(|ui| {
                        ui.set_max_width(inner_w);
                        if theme::segmented_button(ui, &lbl, is_active).clicked() {
                            self.params.set_curve_type(ct);
                            self.eeprom_synced = false;
                        }
                    });
                    ui.add_space(theme::SPACE_XS);
                }
            }

            ui.add_space(theme::SPACE_SM);
            ui.label(
                RichText::new(self.params.curve_type.polynomial_badge())
                    .color(theme::COLOR_TEXT_MUTED)
                    .size(theme::FONT_META)
                    .monospace(),
            );
        });
    }

    /// Axis parameters card: Gestalt spacing, tabular mono values, invert checkbox, and action buttons.
    fn render_axis_parameters_card(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);
            ui.spacing_mut().slider_width = inner_w;

            theme::section_heading(ui, &x56_core::t!("curves-params-heading"));
            ui.add_space(theme::SPACE_MD);

            let compact = inner_w < 260.0;

            // 1. Center deadband (0.0..=25.0%)
            let mut db_pct = self.params.deadband_pct();
            let db_lbl = if compact {
                x56_core::t!("curves-deadband-short")
            } else {
                x56_core::t!("curves-deadband")
            };
            render_param_header(ui, inner_w, &db_lbl, &format!("{:.1}%", db_pct));
            ui.add_space(theme::SPACE_XS);
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

            ui.add_space(theme::SPACE_MD);

            // 2. Curvature / expo (1.00..=3.00)
            let mut expo = self.params.expo_factor();
            render_param_header(
                ui,
                inner_w,
                &x56_core::t!("curves-curvature"),
                &format!("{:.2}", expo),
            );
            ui.add_space(theme::SPACE_XS);
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

            ui.add_space(theme::SPACE_MD);

            // 3. Outer saturation (70.0..=100.0%)
            let mut sat_pct = self.params.saturation_pct();
            render_param_header(
                ui,
                inner_w,
                &x56_core::t!("curves-saturation-outer"),
                &format!("{:.1}%", sat_pct),
            );
            ui.add_space(theme::SPACE_XS);
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

            // Conditional Bezier control points
            if self.params.curve_type == CurveType::Bezier {
                ui.add_space(theme::SPACE_MD);
                ui.label(
                    RichText::new(x56_core::t!("curves-bezier-nodes"))
                        .color(theme::COLOR_TEXT_SECONDARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_XS);

                if inner_w >= 260.0 {
                    let sub_col_w = ((inner_w - theme::SPACE_SM) * 0.5).floor().max(60.0);
                    ui.columns(2, |cols| {
                        cols[0].set_max_width(sub_col_w);
                        cols[0].spacing_mut().slider_width = sub_col_w;
                        render_param_header(
                            &mut cols[0],
                            sub_col_w,
                            "P1",
                            &format!("{:.2}", self.params.bezier_p1[1]),
                        );
                        cols[0].add_space(theme::SPACE_XS);
                        if cols[0]
                            .add(
                                egui::Slider::new(&mut self.params.bezier_p1[1], 0.0..=1.0)
                                    .step_by(0.01)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            self.eeprom_synced = false;
                        }

                        cols[1].set_max_width(sub_col_w);
                        cols[1].spacing_mut().slider_width = sub_col_w;
                        render_param_header(
                            &mut cols[1],
                            sub_col_w,
                            "P2",
                            &format!("{:.2}", self.params.bezier_p2[1]),
                        );
                        cols[1].add_space(theme::SPACE_XS);
                        if cols[1]
                            .add(
                                egui::Slider::new(&mut self.params.bezier_p2[1], 0.0..=1.0)
                                    .step_by(0.01)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            self.eeprom_synced = false;
                        }
                    });
                } else {
                    render_param_header(
                        ui,
                        inner_w,
                        "P1",
                        &format!("{:.2}", self.params.bezier_p1[1]),
                    );
                    ui.add_space(theme::SPACE_XS);
                    if ui
                        .add(
                            egui::Slider::new(&mut self.params.bezier_p1[1], 0.0..=1.0)
                                .step_by(0.01)
                                .show_value(false),
                        )
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                    ui.add_space(theme::SPACE_SM);
                    render_param_header(
                        ui,
                        inner_w,
                        "P2",
                        &format!("{:.2}", self.params.bezier_p2[1]),
                    );
                    ui.add_space(theme::SPACE_XS);
                    if ui
                        .add(
                            egui::Slider::new(&mut self.params.bezier_p2[1], 0.0..=1.0)
                                .step_by(0.01)
                                .show_value(false),
                        )
                        .changed()
                    {
                        self.eeprom_synced = false;
                    }
                }
            }

            ui.add_space(theme::SPACE_MD);
            ui.separator();
            ui.add_space(theme::SPACE_SM);

            if ui
                .checkbox(
                    &mut self.params.invert,
                    RichText::new(x56_core::t!("curves-invert-preview"))
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_SMALL),
                )
                .changed()
            {
                self.eeprom_synced = false;
            }

            ui.add_space(theme::SPACE_MD);
            ui.separator();
            ui.add_space(theme::SPACE_SM);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_SM, theme::SPACE_SM);

                if theme::secondary_button(ui, &x56_core::t!("curves-apply")).clicked() {
                    self.apply_to_hardware();
                }

                if theme::ghost_button(ui, &x56_core::t!("curves-reset-btn")).clicked() {
                    self.reset_current_axis();
                }

                if theme::ghost_button(ui, &x56_core::t!("curves-read-btn")).clicked() {
                    self.read_from_hardware();
                }
            });
        });
    }

    /// Physical springs selector: uniform 40px single-hit-target rows with 8px color dots.
    fn render_physical_springs_card(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(100.0);
            ui.set_max_width(inner_w);
            ui.set_width(inner_w);

            theme::section_heading(ui, &x56_core::t!("curves-springs-heading"));
            ui.add_space(theme::SPACE_SM);

            let springs = [
                PhysicalSpring::Red52mm,
                PhysicalSpring::Green47mm,
                PhysicalSpring::Yellow42mm,
                PhysicalSpring::Blue36mm,
            ];

            for (idx, spring) in springs.into_iter().enumerate() {
                if idx > 0 {
                    ui.add_space(theme::SPACE_XS);
                }
                let is_installed = self.installed_spring == spring;
                let row_size = Vec2::new(inner_w, 40.0);
                let (rect, response) = ui.allocate_exact_size(row_size, Sense::click());

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    let bg_col = if is_installed || response.hovered() {
                        theme::COLOR_SURFACE_HIGH
                    } else {
                        theme::COLOR_SURFACE_CONTAINER
                    };
                    let border_col = if is_installed || response.hovered() {
                        theme::COLOR_BORDER_STRONG
                    } else {
                        theme::COLOR_BORDER_HAIRLINE
                    };

                    painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), bg_col);
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(theme::RADIUS_CONTROL),
                        Stroke::new(1.0, border_col),
                        StrokeKind::Inside,
                    );

                    let dot_center = Pos2::new(rect.min.x + theme::SPACE_MD + 4.0, rect.center().y);
                    painter.circle_filled(dot_center, 4.0, spring.color());

                    let name_galley = painter.layout_no_wrap(
                        spring.localized_label(),
                        FontId::proportional(theme::FONT_BODY),
                        theme::COLOR_TEXT_PRIMARY,
                    );
                    let desc_galley = painter.layout_no_wrap(
                        spring.localized_desc(),
                        FontId::proportional(theme::FONT_SMALL),
                        theme::COLOR_TEXT_SECONDARY,
                    );

                    let text_x = dot_center.x + 4.0 + theme::SPACE_SM;
                    let name_pos = Pos2::new(text_x, rect.center().y - name_galley.size().y * 0.5);
                    let desc_x = text_x + name_galley.size().x + theme::SPACE_SM;
                    let desc_pos = Pos2::new(desc_x, rect.center().y - desc_galley.size().y * 0.5);

                    let installed_galley = if is_installed {
                        Some(painter.layout_no_wrap(
                            x56_core::t!("curves-spring-installed"),
                            FontId::proportional(theme::FONT_META),
                            theme::COLOR_POSITIVE,
                        ))
                    } else {
                        None
                    };

                    let right_reserved = installed_galley
                        .as_ref()
                        .map(|g| g.size().x + theme::SPACE_MD + 14.0)
                        .unwrap_or(theme::SPACE_MD);

                    painter.galley(name_pos, name_galley, theme::COLOR_TEXT_PRIMARY);
                    if desc_pos.x + desc_galley.size().x <= rect.max.x - right_reserved {
                        painter.galley(desc_pos, desc_galley, theme::COLOR_TEXT_SECONDARY);
                    }

                    if let Some(inst_g) = installed_galley {
                        let inst_pos = Pos2::new(
                            rect.max.x - theme::SPACE_MD - inst_g.size().x,
                            rect.center().y - inst_g.size().y * 0.5,
                        );
                        let badge_dot = Pos2::new(inst_pos.x - 8.0, rect.center().y);
                        painter.circle_filled(badge_dot, 3.0, theme::COLOR_POSITIVE);
                        painter.galley(inst_pos, inst_g, theme::COLOR_POSITIVE);
                    }
                }

                if response.clicked() {
                    self.installed_spring = spring;
                }
            }
        });
    }
}

fn format_tick_pct(norm: f64) -> String {
    let pct = (norm * 100.0).round() as i32;
    if pct == 0 {
        "0%".to_owned()
    } else {
        format!("{pct:+}%")
    }
}

fn render_param_header(ui: &mut Ui, row_w: f32, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.set_max_width(row_w);
        ui.set_width(row_w);
        ui.label(
            RichText::new(label)
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .color(theme::COLOR_TEXT_PRIMARY)
                    .size(theme::FONT_SMALL)
                    .monospace(),
            );
        });
    });
}

/// Draws a dashed line segment between `p0` and `p1`.
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

/// Compact horizontal axis meter with fixed 68px label and 56px tabular % columns.
fn render_aligned_axis_bar(
    ui: &mut Ui,
    label: &str,
    value: f64,
    is_bipolar: bool,
    fill_col: Color32,
) {
    const LABEL_COL_W: f32 = 68.0;
    const PCT_COL_W: f32 = 56.0;

    let row_w = ui.available_width().max(LABEL_COL_W + PCT_COL_W + 32.0);
    ui.horizontal(|ui| {
        ui.set_max_width(row_w);
        ui.set_width(row_w);
        ui.spacing_mut().item_spacing.x = theme::SPACE_SM;

        ui.allocate_ui_with_layout(
            Vec2::new(LABEL_COL_W, 16.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(label)
                            .color(theme::COLOR_TEXT_MUTED)
                            .size(theme::FONT_META),
                    )
                    .truncate(),
                );
            },
        );

        let bar_w = (row_w - LABEL_COL_W - PCT_COL_W - theme::SPACE_SM * 2.0).max(24.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 8.0), Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, theme::COLOR_SURFACE_HIGHEST);

            if is_bipolar {
                let mid_x = rect.center().x;
                let v = (value as f32).clamp(-1.0, 1.0);
                let active_x = mid_x + v * (rect.width() * 0.5);
                let fill_rect = Rect::from_min_max(
                    Pos2::new(mid_x.min(active_x), rect.min.y),
                    Pos2::new(mid_x.max(active_x), rect.max.y),
                );
                painter.rect_filled(fill_rect, 2.0, fill_col);
                painter.line_segment(
                    [Pos2::new(mid_x, rect.min.y), Pos2::new(mid_x, rect.max.y)],
                    Stroke::new(1.0, theme::COLOR_TEXT_SECONDARY),
                );
            } else {
                let v = (value as f32).clamp(0.0, 1.0);
                let fill_w = rect.width() * v;
                let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
                painter.rect_filled(fill_rect, 2.0, fill_col);
            }
        }

        let pct_text = if is_bipolar {
            format!("{:+5.1}%", value * 100.0)
        } else {
            format!("{:5.1}%", value * 100.0)
        };
        ui.allocate_ui_with_layout(
            Vec2::new(PCT_COL_W, 16.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.label(
                    RichText::new(pct_text)
                        .color(theme::COLOR_TEXT_SECONDARY)
                        .size(theme::FONT_META)
                        .monospace(),
                );
            },
        );
    });
}
