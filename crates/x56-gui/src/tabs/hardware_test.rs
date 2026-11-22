//! Hardware verification and live input test tab for X56 Stick and Throttle.
//!
//! Renders interactive 2D input schematics for both devices alongside live axis
//! instrumentation: 2D gimbal radars for Stick X/Y (with integrated Z-twist perimeter
//! arc) and both ministicks, dual vertical throttle bars, and 4 circular rotary gauges.

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};
use x56_core::{
    catalog_for_device, ButtonMonitor, DeviceKind, LiveButtonsSnapshot, ProgrammingProfile,
};

use crate::curve_math::{LiveAxesState, LiveInputPoller};
use crate::icons;
use crate::tabs::programming::{
    control_meta_for, get_device_schematic_nodes, localized_node_name_by_id, InteractionMap,
    ProgrammingTab, SchematicNode,
};
use crate::theme::{
    self, COLOR_ACCENT, COLOR_BORDER_HAIRLINE, COLOR_BORDER_SUBTLE, COLOR_GRID_HUD, COLOR_POSITIVE,
    COLOR_SURFACE_HIGH, COLOR_SURFACE_LOWEST, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY, COLOR_WARNING, FONT_META, FONT_SMALL, RADIUS_CONTROL, SPACE_MD, SPACE_SM,
    SPACE_XS,
};

const AXIS_MOVE_THRESHOLD: f64 = 0.08;
const TWIST_ARC_HALF_SPAN_RAD: f32 = 3.0 * FRAC_PI_4; // 135 deg -> 270 deg total sweep
const VECTOR_LINE_STROKE: Color32 = Color32::from_rgba_unmultiplied_const(56, 189, 248, 120);

/// All 13 analog axis identifiers tracked during a hardware verification session.
pub const ALL_TRACKED_AXES: [&str; 13] = [
    "stick_x",
    "stick_y",
    "stick_z",
    "ministick_x",
    "ministick_y",
    "throttle_left",
    "throttle_right",
    "throttle_ministick_x",
    "throttle_ministick_y",
    "rotary_1",
    "rotary_2",
    "rotary_3",
    "rotary_4",
];

/// Device filter for the hardware test workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestViewFilter {
    #[default]
    Both,
    StickOnly,
    ThrottleOnly,
}

impl TestViewFilter {
    pub fn label(self) -> String {
        match self {
            Self::Both => x56_core::t!("hwtest-filter-both"),
            Self::StickOnly => x56_core::t!("hwtest-filter-stick"),
            Self::ThrottleOnly => x56_core::t!("hwtest-filter-throttle"),
        }
    }
}

/// Interactive hardware test module state (buttons, axes, ministicks, throttles, rotaries).
pub struct HardwareTestTab {
    pub view_filter: TestViewFilter,
    pub tested_buttons: HashSet<String>,
    pub tested_axes: HashSet<&'static str>,
    pub baseline_axes: Option<LiveAxesState>,
    pub last_pressed_id: Option<String>,
    pub hovered_stick_id: Option<String>,
    pub hovered_throttle_id: Option<String>,
    prev_pressed_buttons: HashSet<String>,
}

impl Default for HardwareTestTab {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareTestTab {
    /// Creates a new empty hardware test session.
    pub fn new() -> Self {
        Self {
            view_filter: TestViewFilter::Both,
            tested_buttons: HashSet::new(),
            tested_axes: HashSet::new(),
            baseline_axes: None,
            last_pressed_id: None,
            hovered_stick_id: None,
            hovered_throttle_id: None,
            prev_pressed_buttons: HashSet::new(),
        }
    }

    /// Clears all tested buttons, tested axes, and the last-detected button state.
    pub fn reset_test(&mut self) {
        self.tested_buttons.clear();
        self.tested_axes.clear();
        self.baseline_axes = None;
        self.last_pressed_id = None;
        self.prev_pressed_buttons.clear();
    }

    /// Updates tested button and axis checklists from the current live telemetry frame.
    pub fn update_session_tracking(
        &mut self,
        live_axes: &LiveAxesState,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let mut newly_pressed: Vec<&String> = snapshot
            .pressed_buttons
            .iter()
            .filter(|id| !self.prev_pressed_buttons.contains(*id))
            .collect();
        newly_pressed.sort();

        if let Some(first_new) = newly_pressed.first() {
            self.last_pressed_id = Some((*first_new).clone());
        } else if self.last_pressed_id.is_none() && !snapshot.pressed_buttons.is_empty() {
            let mut sorted: Vec<&String> = snapshot.pressed_buttons.iter().collect();
            sorted.sort();
            if let Some(first) = sorted.first() {
                self.last_pressed_id = Some((*first).clone());
            }
        }

        for id in &snapshot.pressed_buttons {
            self.tested_buttons.insert(id.clone());
        }
        self.prev_pressed_buttons = snapshot.pressed_buttons.clone();

        match &mut self.baseline_axes {
            None => {
                self.baseline_axes = Some(*live_axes);
            }
            Some(base) => {
                // Refresh resting baseline when a physical device connects mid-session.
                if !base.stick_connected && live_axes.stick_connected {
                    base.stick_connected = true;
                    base.stick_x = live_axes.stick_x;
                    base.stick_y = live_axes.stick_y;
                    base.stick_z = live_axes.stick_z;
                    base.ministick_x = live_axes.ministick_x;
                    base.ministick_y = live_axes.ministick_y;
                }
                if !base.throttle_connected && live_axes.throttle_connected {
                    base.throttle_connected = true;
                    base.throttle_left = live_axes.throttle_left;
                    base.throttle_right = live_axes.throttle_right;
                    base.throttle_ministick_x = live_axes.throttle_ministick_x;
                    base.throttle_ministick_y = live_axes.throttle_ministick_y;
                    base.rotary_1 = live_axes.rotary_1;
                    base.rotary_2 = live_axes.rotary_2;
                    base.rotary_3 = live_axes.rotary_3;
                    base.rotary_4 = live_axes.rotary_4;
                }

                let pairs: [(&'static str, f64, f64); 13] = [
                    ("stick_x", live_axes.stick_x, base.stick_x),
                    ("stick_y", live_axes.stick_y, base.stick_y),
                    ("stick_z", live_axes.stick_z, base.stick_z),
                    ("ministick_x", live_axes.ministick_x, base.ministick_x),
                    ("ministick_y", live_axes.ministick_y, base.ministick_y),
                    ("throttle_left", live_axes.throttle_left, base.throttle_left),
                    (
                        "throttle_right",
                        live_axes.throttle_right,
                        base.throttle_right,
                    ),
                    (
                        "throttle_ministick_x",
                        live_axes.throttle_ministick_x,
                        base.throttle_ministick_x,
                    ),
                    (
                        "throttle_ministick_y",
                        live_axes.throttle_ministick_y,
                        base.throttle_ministick_y,
                    ),
                    ("rotary_1", live_axes.rotary_1, base.rotary_1),
                    ("rotary_2", live_axes.rotary_2, base.rotary_2),
                    ("rotary_3", live_axes.rotary_3, base.rotary_3),
                    ("rotary_4", live_axes.rotary_4, base.rotary_4),
                ];

                for (name, cur, prev) in pairs {
                    if (cur - prev).abs() > AXIS_MOVE_THRESHOLD {
                        self.tested_axes.insert(name);
                    }
                }
            }
        }
    }

    /// Returns `(tested_count, total_count)` for Stick or Throttle buttons.
    pub fn button_progress(&self, kind: DeviceKind) -> (usize, usize) {
        let catalog = catalog_for_device(kind);
        let tested = catalog
            .iter()
            .filter(|b| self.tested_buttons.contains(&b.id))
            .count();
        (tested, catalog.len())
    }

    /// Returns `(tested_count, total_count)` across all 13 analog axes.
    pub fn axes_progress(&self) -> (usize, usize) {
        let tested = ALL_TRACKED_AXES
            .iter()
            .filter(|id| self.tested_axes.contains(*id))
            .count();
        (tested, ALL_TRACKED_AXES.len())
    }

    /// Renders the complete Hardware Test subsystem view.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        poller: &LiveInputPoller,
        monitor: &ButtonMonitor,
        profile: &ProgrammingProfile,
    ) {
        let live_axes = poller.get_state();
        let snapshot = monitor.snapshot();
        self.update_session_tracking(&live_axes, &snapshot);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.render_header_bar(ui, &snapshot);
                ui.add_space(SPACE_MD);

                let avail_w = ui.available_width();
                match self.view_filter {
                    TestViewFilter::StickOnly => {
                        self.render_stick_column(ui, &live_axes, &snapshot, profile);
                    }
                    TestViewFilter::ThrottleOnly => {
                        self.render_throttle_column(ui, &live_axes, &snapshot, profile);
                    }
                    TestViewFilter::Both => {
                        if avail_w >= 920.0 {
                            let gap = SPACE_MD;
                            let col_w = ((avail_w - gap) * 0.5).floor();
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                ui.allocate_ui_with_layout(
                                    Vec2::new(col_w, 10.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_max_width(col_w);
                                        ui.set_width(col_w);
                                        self.render_throttle_column(
                                            ui, &live_axes, &snapshot, profile,
                                        );
                                    },
                                );
                                ui.allocate_ui_with_layout(
                                    Vec2::new(col_w, 10.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.set_max_width(col_w);
                                        ui.set_width(col_w);
                                        self.render_stick_column(
                                            ui, &live_axes, &snapshot, profile,
                                        );
                                    },
                                );
                            });
                        } else {
                            self.render_throttle_column(ui, &live_axes, &snapshot, profile);
                            ui.add_space(SPACE_MD);
                            self.render_stick_column(ui, &live_axes, &snapshot, profile);
                        }
                    }
                }
            });
    }

    fn render_header_bar(&mut self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_SM, SPACE_SM);

                theme::section_heading(ui, &x56_core::t!("hwtest-title"));

                ui.add_space(SPACE_SM);
                for filter in [
                    TestViewFilter::Both,
                    TestViewFilter::ThrottleOnly,
                    TestViewFilter::StickOnly,
                ] {
                    if theme::segmented_button(ui, &filter.label(), self.view_filter == filter)
                        .clicked()
                    {
                        self.view_filter = filter;
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let reset_lbl =
                        format!("{} {}", icons::ROTATE_CCW, x56_core::t!("hwtest-reset"));
                    if theme::secondary_button(ui, &reset_lbl)
                        .on_hover_text(x56_core::t!("hwtest-reset-tooltip"))
                        .clicked()
                    {
                        self.reset_test();
                    }

                    ui.add_space(SPACE_SM);

                    let sld_val = if snapshot.slider_sld_on {
                        x56_core::t!("live-test-on")
                    } else {
                        x56_core::t!("live-test-off")
                    };
                    let sld_col = if snapshot.slider_sld_on {
                        COLOR_POSITIVE
                    } else {
                        COLOR_TEXT_SECONDARY
                    };
                    theme::meta_kv(ui, &x56_core::t!("live-test-sld"), &sld_val, sld_col);

                    ui.add_space(SPACE_SM);

                    theme::meta_kv(
                        ui,
                        &x56_core::t!("live-test-mode"),
                        snapshot.active_mode.as_str(),
                        COLOR_ACCENT,
                    );
                });
            });

            ui.add_space(SPACE_SM);

            let (stick_tested, stick_total) = self.button_progress(DeviceKind::Stick);
            let (thr_tested, thr_total) = self.button_progress(DeviceKind::Throttle);
            let (axes_tested, axes_total) = self.axes_progress();

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_MD, SPACE_XS);

                let stick_s = x56_core::t!(
                    "hwtest-progress-stick",
                    "tested" => stick_tested.to_string().as_str(),
                    "total" => stick_total.to_string().as_str()
                );
                let thr_s = x56_core::t!(
                    "hwtest-progress-throttle",
                    "tested" => thr_tested.to_string().as_str(),
                    "total" => thr_total.to_string().as_str()
                );
                let axes_s = x56_core::t!(
                    "hwtest-progress-axes",
                    "tested" => axes_tested.to_string().as_str(),
                    "total" => axes_total.to_string().as_str()
                );

                ui.label(
                    RichText::new(thr_s)
                        .color(progress_color(thr_tested, thr_total))
                        .size(FONT_META)
                        .monospace(),
                );
                ui.label(
                    RichText::new(stick_s)
                        .color(progress_color(stick_tested, stick_total))
                        .size(FONT_META)
                        .monospace(),
                );
                ui.label(
                    RichText::new(axes_s)
                        .color(progress_color(axes_tested, axes_total))
                        .size(FONT_META)
                        .monospace(),
                );

                let last_text = match &self.last_pressed_id {
                    Some(id) => {
                        let name = localized_node_name_by_id(id, id);
                        x56_core::t!(
                            "hwtest-last-button",
                            "name" => name.as_str(),
                            "id" => id.as_str()
                        )
                    }
                    None => x56_core::t!("hwtest-last-button-none"),
                };
                let last_col = if self.last_pressed_id.is_some() {
                    COLOR_TEXT_PRIMARY
                } else {
                    COLOR_TEXT_MUTED
                };
                ui.label(
                    RichText::new(last_text)
                        .color(last_col)
                        .size(FONT_META)
                        .monospace(),
                );
            });
        });
    }

    fn render_stick_column(
        &mut self,
        ui: &mut Ui,
        live_axes: &LiveAxesState,
        snapshot: &LiveButtonsSnapshot,
        profile: &ProgrammingProfile,
    ) {
        self.render_device_map_card(ui, DeviceKind::Stick, snapshot, profile);
        ui.add_space(SPACE_MD);
        self.render_stick_axes_card(ui, live_axes, snapshot);
    }

    fn render_throttle_column(
        &mut self,
        ui: &mut Ui,
        live_axes: &LiveAxesState,
        snapshot: &LiveButtonsSnapshot,
        profile: &ProgrammingProfile,
    ) {
        self.render_device_map_card(ui, DeviceKind::Throttle, snapshot, profile);
        ui.add_space(SPACE_MD);
        self.render_throttle_axes_card(ui, live_axes, snapshot);
    }

    fn render_device_map_card(
        &mut self,
        ui: &mut Ui,
        device: DeviceKind,
        snapshot: &LiveButtonsSnapshot,
        profile: &ProgrammingProfile,
    ) {
        let (title, connected) = match device {
            DeviceKind::Stick => (
                x56_core::t!("hwtest-stick-map-title"),
                snapshot.stick_connected,
            ),
            DeviceKind::Throttle => (
                x56_core::t!("hwtest-throttle-map-title"),
                snapshot.throttle_connected,
            ),
        };
        let (tested_cnt, total_cnt) = self.button_progress(device);

        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(120.0);
            ui.set_width(inner_w);

            ui.horizontal(|ui| {
                ui.set_width(inner_w);
                theme::section_heading(ui, &title);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{tested_cnt}/{total_cnt}"))
                            .color(progress_color(tested_cnt, total_cnt))
                            .size(FONT_META)
                            .monospace(),
                    );
                    ui.add_space(SPACE_SM);
                    let (dot_col, status_txt, txt_col) = if connected {
                        (
                            COLOR_POSITIVE,
                            x56_core::t!("header-online"),
                            COLOR_TEXT_SECONDARY,
                        )
                    } else {
                        (
                            COLOR_WARNING,
                            x56_core::t!("header-offline"),
                            COLOR_TEXT_MUTED,
                        )
                    };
                    ui.label(RichText::new(status_txt).color(txt_col).size(FONT_META));
                    theme::status_dot(ui, dot_col, 7.0);
                });
            });
            ui.add_space(SPACE_SM);

            theme::canvas_frame().show(ui, |ui| {
                let avail_w = ui.available_width();
                ui.set_width(avail_w);
                let aspect = 790.0 / 1180.0;
                let canvas_height = (avail_w * aspect).clamp(120.0, 440.0);
                let canvas_width = (canvas_height / aspect).min(avail_w);
                let canvas_size = Vec2::new(canvas_width, canvas_height);
                let (response, painter) = ui
                    .vertical_centered(|ui| ui.allocate_painter(canvas_size, Sense::hover()))
                    .inner;
                let rect = response.rect;

                painter.rect_filled(rect, RADIUS_CONTROL as f32, COLOR_SURFACE_LOWEST);
                draw_subtle_grid(&painter, rect, 64.0);

                let s = rect.width() / 1180.0;
                let nodes = get_device_schematic_nodes(device);
                let mouse_pos = ui.input(|i| i.pointer.hover_pos());

                let hovered_slot = match device {
                    DeviceKind::Stick => &mut self.hovered_stick_id,
                    DeviceKind::Throttle => &mut self.hovered_throttle_id,
                };
                if let Some(mpos) = mouse_pos {
                    if rect.contains(mpos) {
                        *hovered_slot = find_hovered_node_id(&nodes, rect, s, device, mpos);
                    } else {
                        *hovered_slot = None;
                    }
                }
                let active_hovered = hovered_slot.as_deref();

                let im = InteractionMap {
                    pressed: &snapshot.pressed_buttons,
                    hovered: active_hovered,
                    profile,
                    mode: snapshot.active_mode,
                    highlighted_ids: Some(&self.tested_buttons),
                };

                match device {
                    DeviceKind::Stick => {
                        ProgrammingTab::draw_vector_stick_silhouette(&painter, rect, &im);
                    }
                    DeviceKind::Throttle => {
                        ProgrammingTab::draw_vector_throttle_silhouette(&painter, rect, &im);
                    }
                }

                // Draw compact tested indicator dots over every verified control node.
                for node in &nodes {
                    if self.tested_buttons.contains(node.id) {
                        let badge_pos = tested_badge_pos(node, rect, s, device);
                        painter.circle_filled(
                            badge_pos,
                            (3.2 * s.max(0.75)).min(4.0),
                            COLOR_POSITIVE,
                        );
                    }
                }

                if let Some(hid) = active_hovered {
                    if let Some(node) = nodes.iter().find(|n| n.id == hid) {
                        let meta = control_meta_for(node.id);
                        let loc_name = localized_node_name_by_id(node.id, node.full_name);
                        let is_pressed = snapshot.pressed_buttons.contains(node.id);
                        let is_tested = self.tested_buttons.contains(node.id);
                        let (status_str, status_col) = if is_pressed {
                            (x56_core::t!("hwtest-status-pressed"), COLOR_ACCENT)
                        } else if is_tested {
                            (x56_core::t!("hwtest-status-tested"), COLOR_POSITIVE)
                        } else {
                            (x56_core::t!("hwtest-status-pending"), COLOR_TEXT_MUTED)
                        };

                        response.on_hover_ui_at_pointer(|ui| {
                            theme::subcard_frame().show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&loc_name)
                                            .color(COLOR_TEXT_PRIMARY)
                                            .size(FONT_SMALL),
                                    );
                                    ui.label(
                                        RichText::new(x56_core::t!(
                                            "prog-schematic-meta-id",
                                            "id" => node.id,
                                            "code" => node.short_label
                                        ))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(FONT_META)
                                        .monospace(),
                                    );
                                    ui.add_space(SPACE_XS);
                                    ui.label(
                                        RichText::new(status_str)
                                            .color(status_col)
                                            .size(FONT_SMALL),
                                    );
                                    ui.label(
                                        RichText::new(x56_core::t!(
                                            "prog-schematic-meta-group",
                                            "group" => meta.group.as_str(),
                                            "kind" => meta.kind.as_str()
                                        ))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(FONT_META),
                                    );
                                });
                            });
                        });
                    }
                }
            });
        });
    }

    fn render_stick_axes_card(
        &self,
        ui: &mut Ui,
        live_axes: &LiveAxesState,
        snapshot: &LiveButtonsSnapshot,
    ) {
        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(120.0);
            ui.set_width(inner_w);
            theme::section_heading(ui, &x56_core::t!("hwtest-stick-axes-title"));
            ui.add_space(SPACE_SM);

            let btn_c_pressed = snapshot.pressed_buttons.contains("stick_btn_c");
            let btn_c_tested = self.tested_buttons.contains("stick_btn_c");

            if inner_w >= 400.0 {
                let gap = SPACE_SM;
                let half_w = ((inner_w - gap) * 0.5).floor();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        Vec2::new(half_w, 10.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            self.render_stick_main_gimbal_subcard(ui, half_w, live_axes);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(half_w, 10.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            self.render_ministick_gimbal_subcard(
                                ui,
                                half_w,
                                &x56_core::t!("hwtest-stick-ministick"),
                                live_axes.ministick_x as f32,
                                live_axes.ministick_y as f32,
                                btn_c_pressed,
                                btn_c_tested,
                                self.tested_axes.contains("ministick_x")
                                    || self.tested_axes.contains("ministick_y"),
                            );
                        },
                    );
                });
            } else {
                self.render_stick_main_gimbal_subcard(ui, inner_w, live_axes);
                ui.add_space(SPACE_SM);
                self.render_ministick_gimbal_subcard(
                    ui,
                    inner_w,
                    &x56_core::t!("hwtest-stick-ministick"),
                    live_axes.ministick_x as f32,
                    live_axes.ministick_y as f32,
                    btn_c_pressed,
                    btn_c_tested,
                    self.tested_axes.contains("ministick_x")
                        || self.tested_axes.contains("ministick_y"),
                );
            }
        });
    }

    fn render_stick_main_gimbal_subcard(&self, ui: &mut Ui, width: f32, live_axes: &LiveAxesState) {
        theme::subcard_frame().show(ui, |ui| {
            let inner_w = (width - 2.0 * SPACE_MD - 2.0).max(80.0);
            ui.vertical(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                let any_tested = self.tested_axes.contains("stick_x")
                    || self.tested_axes.contains("stick_y")
                    || self.tested_axes.contains("stick_z");
                render_subcard_title_with_status(
                    ui,
                    &x56_core::t!("hwtest-main-gimbal"),
                    any_tested,
                );
                ui.add_space(SPACE_SM);

                let box_size = inner_w.clamp(110.0, 184.0).floor();
                let (mag, deg) = live_axes.gimbal_vector();
                draw_gimbal_with_twist_z(
                    ui,
                    box_size,
                    live_axes.stick_x as f32,
                    live_axes.stick_y as f32,
                    live_axes.stick_z as f32,
                    mag,
                    deg,
                );
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn render_ministick_gimbal_subcard(
        &self,
        ui: &mut Ui,
        width: f32,
        title: &str,
        x: f32,
        y: f32,
        push_pressed: bool,
        push_tested: bool,
        axes_tested: bool,
    ) {
        theme::subcard_frame().show(ui, |ui| {
            let inner_w = (width - 2.0 * SPACE_MD - 2.0).max(80.0);
            ui.vertical(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                render_subcard_title_with_status(ui, title, axes_tested || push_tested);
                ui.add_space(SPACE_SM);

                let box_size = inner_w.clamp(110.0, 168.0).floor();
                draw_gimbal_2d(ui, box_size, x, y, push_pressed, push_tested);
            });
        });
    }

    fn render_throttle_axes_card(
        &self,
        ui: &mut Ui,
        live_axes: &LiveAxesState,
        snapshot: &LiveButtonsSnapshot,
    ) {
        theme::card_frame().show(ui, |ui| {
            let inner_w = ui.available_width().max(120.0);
            ui.set_width(inner_w);
            theme::section_heading(ui, &x56_core::t!("hwtest-throttle-axes-title"));
            ui.add_space(SPACE_SM);

            let mini_push_pressed = snapshot.pressed_buttons.contains("thr_ministick_push");
            let mini_push_tested = self.tested_buttons.contains("thr_ministick_push");

            if inner_w >= 400.0 {
                let gap = SPACE_SM;
                let half_w = ((inner_w - gap) * 0.5).floor();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        Vec2::new(half_w, 10.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            self.render_dual_throttle_bars_subcard(ui, half_w, live_axes);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(half_w, 10.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            self.render_ministick_gimbal_subcard(
                                ui,
                                half_w,
                                &x56_core::t!("hwtest-thr-ministick"),
                                live_axes.throttle_ministick_x as f32,
                                live_axes.throttle_ministick_y as f32,
                                mini_push_pressed,
                                mini_push_tested,
                                self.tested_axes.contains("throttle_ministick_x")
                                    || self.tested_axes.contains("throttle_ministick_y"),
                            );
                        },
                    );
                });
            } else {
                self.render_dual_throttle_bars_subcard(ui, inner_w, live_axes);
                ui.add_space(SPACE_SM);
                self.render_ministick_gimbal_subcard(
                    ui,
                    inner_w,
                    &x56_core::t!("hwtest-thr-ministick"),
                    live_axes.throttle_ministick_x as f32,
                    live_axes.throttle_ministick_y as f32,
                    mini_push_pressed,
                    mini_push_tested,
                    self.tested_axes.contains("throttle_ministick_x")
                        || self.tested_axes.contains("throttle_ministick_y"),
                );
            }

            ui.add_space(SPACE_SM);

            theme::subcard_frame().show(ui, |ui| {
                let sub_w = ui.available_width().max(100.0);
                ui.set_width(sub_w);
                let any_rot_tested = ["rotary_1", "rotary_2", "rotary_3", "rotary_4"]
                    .iter()
                    .any(|id| self.tested_axes.contains(id));
                render_subcard_title_with_status(
                    ui,
                    &x56_core::t!("hwtest-rotaries-title"),
                    any_rot_tested,
                );
                ui.add_space(SPACE_SM);

                let rotaries = [
                    (
                        x56_core::t!("hwtest-rty-1"),
                        live_axes.rotary_1 as f32,
                        Some(snapshot.pressed_buttons.contains("thr_rty_f_push")),
                        Some(self.tested_buttons.contains("thr_rty_f_push")),
                        self.tested_axes.contains("rotary_1"),
                    ),
                    (
                        x56_core::t!("hwtest-rty-2"),
                        live_axes.rotary_2 as f32,
                        Some(snapshot.pressed_buttons.contains("thr_rty_g_push")),
                        Some(self.tested_buttons.contains("thr_rty_g_push")),
                        self.tested_axes.contains("rotary_2"),
                    ),
                    (
                        x56_core::t!("hwtest-rty-3"),
                        live_axes.rotary_3 as f32,
                        None,
                        None,
                        self.tested_axes.contains("rotary_3"),
                    ),
                    (
                        x56_core::t!("hwtest-rty-4"),
                        live_axes.rotary_4 as f32,
                        None,
                        None,
                        self.tested_axes.contains("rotary_4"),
                    ),
                ];

                if sub_w >= 320.0 {
                    let gap = SPACE_MD;
                    let cell_w = ((sub_w - gap * 3.0) / 4.0).floor().clamp(68.0, 96.0);
                    let total_rot_w = cell_w * 4.0 + gap * 3.0;
                    ui.vertical_centered(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(total_rot_w, 10.0),
                            Layout::left_to_right(Align::TOP),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for (lbl, val, push_p, push_t, ax_t) in &rotaries {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(cell_w, 10.0),
                                        Layout::top_down(Align::Center),
                                        |ui| {
                                            ui.set_width(cell_w);
                                            draw_circular_rotary_gauge(
                                                ui,
                                                cell_w.clamp(68.0, 84.0),
                                                lbl,
                                                *val,
                                                *push_p,
                                                *push_t,
                                                *ax_t,
                                            );
                                        },
                                    );
                                }
                            },
                        );
                    });
                } else {
                    let gap = SPACE_MD;
                    let cell_w = ((sub_w - gap) * 0.5).floor().clamp(68.0, 96.0);
                    let total_rot_w = cell_w * 2.0 + gap;
                    for chunk in rotaries.chunks(2) {
                        ui.vertical_centered(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(total_rot_w, 10.0),
                                Layout::left_to_right(Align::TOP),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for (lbl, val, push_p, push_t, ax_t) in chunk {
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(cell_w, 10.0),
                                            Layout::top_down(Align::Center),
                                            |ui| {
                                                ui.set_width(cell_w);
                                                draw_circular_rotary_gauge(
                                                    ui,
                                                    cell_w.clamp(68.0, 84.0),
                                                    lbl,
                                                    *val,
                                                    *push_p,
                                                    *push_t,
                                                    *ax_t,
                                                );
                                            },
                                        );
                                    }
                                },
                            );
                        });
                        ui.add_space(SPACE_SM);
                    }
                }
            });
        });
    }

    fn render_dual_throttle_bars_subcard(
        &self,
        ui: &mut Ui,
        width: f32,
        live_axes: &LiveAxesState,
    ) {
        theme::subcard_frame().show(ui, |ui| {
            let inner_w = (width - 2.0 * SPACE_MD - 2.0).max(80.0);
            ui.vertical(|ui| {
                ui.set_max_width(inner_w);
                ui.set_width(inner_w);

                let tested = self.tested_axes.contains("throttle_left")
                    || self.tested_axes.contains("throttle_right");
                render_subcard_title_with_status(ui, &x56_core::t!("hwtest-dual-throttle"), tested);
                ui.add_space(SPACE_SM);

                draw_vertical_throttle_bars(
                    ui,
                    live_axes.throttle_left as f32,
                    live_axes.throttle_right as f32,
                    self.tested_axes.contains("throttle_left"),
                    self.tested_axes.contains("throttle_right"),
                );
            });
        });
    }
}

// ============================================================================
// MATHEMATICAL HELPERS & VECTOR INSTRUMENT WIDGETS
// ============================================================================

/// Maps a normalized bipolar value `[-1.0, 1.0]` to an angle on a 270-degree top-centered dial.
///
/// `-1.0` maps to `-225°` (7:30 o'clock), `0.0` maps to `-90°` (12 o'clock),
/// and `+1.0` maps to `+45°` (4:30 o'clock) in screen coordinates (+Y downward).
#[inline]
pub fn dial_angle_rad(norm: f32) -> f32 {
    -FRAC_PI_2 + norm.clamp(-1.0, 1.0) * TWIST_ARC_HALF_SPAN_RAD
}

/// Computes normalized vector `(magnitude, degrees)` from 2D `(x, y)` coordinates.
#[inline]
pub fn vector_polar_coords(x: f32, y: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt().min(std::f32::consts::SQRT_2);
    let mut deg = y.atan2(x).to_degrees();
    if deg < 0.0 {
        deg += 360.0;
    }
    (mag, deg)
}

fn progress_color(tested: usize, total: usize) -> Color32 {
    if total > 0 && tested >= total {
        COLOR_POSITIVE
    } else if tested > 0 {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_SECONDARY
    }
}

fn render_subcard_title_with_status(ui: &mut Ui, title: &str, tested: bool) {
    let row_w = ui.available_width();
    ui.horizontal(|ui| {
        ui.set_max_width(row_w);
        ui.set_width(row_w);
        let title_w = (row_w - 14.0).max(40.0);
        ui.allocate_ui_with_layout(
            Vec2::new(title_w, 18.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .color(COLOR_TEXT_PRIMARY)
                            .size(FONT_SMALL),
                    )
                    .wrap(),
                );
            },
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let col = if tested {
                COLOR_POSITIVE
            } else {
                COLOR_TEXT_MUTED
            };
            theme::status_dot(ui, col, 6.0);
        });
    });
}

fn draw_subtle_grid(painter: &egui::Painter, rect: Rect, step: f32) {
    let mut gx = rect.min.x + step;
    while gx < rect.max.x {
        painter.line_segment(
            [Pos2::new(gx, rect.min.y), Pos2::new(gx, rect.max.y)],
            Stroke::new(0.5, COLOR_BORDER_SUBTLE),
        );
        gx += step;
    }
    let mut gy = rect.min.y + step;
    while gy < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, gy), Pos2::new(rect.max.x, gy)],
            Stroke::new(0.5, COLOR_BORDER_SUBTLE),
        );
        gy += step;
    }
}

fn find_hovered_node_id(
    nodes: &[SchematicNode],
    rect: Rect,
    s: f32,
    device: DeviceKind,
    mpos: Pos2,
) -> Option<String> {
    let mut best_dist = f32::INFINITY;
    let mut hovered = None;

    for node in nodes {
        let center = Pos2::new(
            ProgrammingTab::compact_vector_map_x(rect, node.norm_x, device),
            rect.min.y + node.norm_y * rect.height(),
        );
        let dist = (mpos - center).length();
        let is_hit = if node.is_rect {
            let half_w = (node.rect_size.x * s * 0.5).max(12.0);
            let half_h = (node.rect_size.y * s * 0.5).max(10.0);
            mpos.x >= center.x - half_w
                && mpos.x <= center.x + half_w
                && mpos.y >= center.y - half_h
                && mpos.y <= center.y + half_h
        } else {
            let hit_r = (node.radius * s + 2.0).max(10.0);
            dist <= hit_r
        };
        if is_hit && dist < best_dist {
            best_dist = dist;
            hovered = Some(node.id.to_string());
        }
    }

    hovered
}

fn tested_badge_pos(node: &SchematicNode, rect: Rect, s: f32, device: DeviceKind) -> Pos2 {
    let center = Pos2::new(
        ProgrammingTab::compact_vector_map_x(rect, node.norm_x, device),
        rect.min.y + node.norm_y * rect.height(),
    );
    if node.is_rect {
        let half_w = (node.rect_size.x * s * 0.5).max(12.0);
        let half_h = (node.rect_size.y * s * 0.5).max(10.0);
        Pos2::new(center.x + half_w - 3.0, center.y - half_h + 3.0)
    } else if node.radius >= 18.0 {
        let r = (node.radius * s).max(8.0);
        Pos2::new(center.x + r * 0.68, center.y - r * 0.68)
    } else {
        Pos2::new(center.x + 5.0 * s.max(0.7), center.y - 5.0 * s.max(0.7))
    }
}

fn draw_arc_polyline(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start_rad: f32,
    end_rad: f32,
    stroke: Stroke,
) {
    let span = (end_rad - start_rad).abs();
    if span < 1e-4 {
        return;
    }
    let steps = ((span * 24.0).ceil() as usize).clamp(4, 48);
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = start_rad + (end_rad - start_rad) * t;
        pts.push(Pos2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    painter.add(Shape::line(pts, stroke));
}

/// Renders the main Stick X/Y 2D gimbal with an integrated perimeter Z-twist arc.
fn draw_gimbal_with_twist_z(
    ui: &mut Ui,
    box_size: f32,
    x: f32,
    y: f32,
    z: f32,
    vect_mag: f64,
    vect_deg: f64,
) {
    ui.vertical_centered(|ui| {
        let (resp, painter) = ui.allocate_painter(Vec2::new(box_size, box_size), Sense::hover());
        let r = resp.rect;

        painter.rect_filled(r, RADIUS_CONTROL as f32, COLOR_SURFACE_LOWEST);
        painter.rect_stroke(
            r,
            RADIUS_CONTROL as f32,
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
            StrokeKind::Inside,
        );

        let center = r.center();
        let outer_r = (box_size * 0.5) - 10.0;
        let gimbal_r = (outer_r - 22.0).max(24.0);

        // Outer Twist Z perimeter track (-135 deg to +135 deg)
        let z_min_rad = dial_angle_rad(-1.0);
        let z_zero_rad = dial_angle_rad(0.0);
        let z_max_rad = dial_angle_rad(1.0);
        let z_cur_rad = dial_angle_rad(z);

        draw_arc_polyline(
            &painter,
            center,
            outer_r,
            z_min_rad,
            z_max_rad,
            Stroke::new(3.0, COLOR_SURFACE_HIGH),
        );

        // Tick marks at Z = -1, 0, +1
        for tick_rad in [z_min_rad, z_zero_rad, z_max_rad] {
            let p_in = Pos2::new(
                center.x + (outer_r - 4.0) * tick_rad.cos(),
                center.y + (outer_r - 4.0) * tick_rad.sin(),
            );
            let p_out = Pos2::new(
                center.x + (outer_r + 3.0) * tick_rad.cos(),
                center.y + (outer_r + 3.0) * tick_rad.sin(),
            );
            painter.line_segment([p_in, p_out], Stroke::new(1.0, COLOR_TEXT_MUTED));
        }

        // Active Z twist arc + marker dot
        draw_arc_polyline(
            &painter,
            center,
            outer_r,
            z_zero_rad,
            z_cur_rad,
            Stroke::new(3.0, COLOR_ACCENT),
        );
        let z_tip = Pos2::new(
            center.x + outer_r * z_cur_rad.cos(),
            center.y + outer_r * z_cur_rad.sin(),
        );
        painter.circle_filled(z_tip, 3.5, COLOR_ACCENT);

        // Inner X/Y gimbal rings and crosshair
        painter.circle_stroke(center, gimbal_r, Stroke::new(1.0, COLOR_GRID_HUD));
        painter.circle_stroke(center, gimbal_r * 0.5, Stroke::new(1.0, COLOR_GRID_HUD));
        painter.line_segment(
            [
                Pos2::new(center.x - gimbal_r, center.y),
                Pos2::new(center.x + gimbal_r, center.y),
            ],
            Stroke::new(1.0, COLOR_BORDER_SUBTLE),
        );
        painter.line_segment(
            [
                Pos2::new(center.x, center.y - gimbal_r),
                Pos2::new(center.x, center.y + gimbal_r),
            ],
            Stroke::new(1.0, COLOR_BORDER_SUBTLE),
        );

        let gx = x.clamp(-1.0, 1.0);
        let gy = y.clamp(-1.0, 1.0);
        let dot_pos = Pos2::new(center.x + gx * gimbal_r, center.y - gy * gimbal_r);
        painter.line_segment([center, dot_pos], Stroke::new(1.2, VECTOR_LINE_STROKE));
        painter.circle_filled(dot_pos, 4.5, COLOR_ACCENT);

        painter.text(
            Pos2::new(center.x, r.max.y - 10.0),
            egui::Align2::CENTER_CENTER,
            "Z",
            FontId::monospace(10.5),
            COLOR_TEXT_MUTED,
        );

        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(format!("X {:+.3}  Y {:+.3}  Z {:+.3}", gx, gy, z))
                .color(COLOR_TEXT_PRIMARY)
                .size(FONT_META)
                .monospace(),
        );
        ui.label(
            RichText::new(format!("Mag {:.3}  {:03.0}°", vect_mag, vect_deg))
                .color(COLOR_TEXT_SECONDARY)
                .size(FONT_META)
                .monospace(),
        );
    });
}

/// Renders an independent 2D gimbal radar for an analog ministick with push-button status.
fn draw_gimbal_2d(
    ui: &mut Ui,
    box_size: f32,
    x: f32,
    y: f32,
    push_pressed: bool,
    push_tested: bool,
) {
    ui.vertical_centered(|ui| {
        let (resp, painter) = ui.allocate_painter(Vec2::new(box_size, box_size), Sense::hover());
        let r = resp.rect;

        painter.rect_filled(r, RADIUS_CONTROL as f32, COLOR_SURFACE_LOWEST);
        painter.rect_stroke(
            r,
            RADIUS_CONTROL as f32,
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
            StrokeKind::Inside,
        );

        let center = r.center();
        let half = ((box_size * 0.5) - 14.0).max(24.0);

        painter.circle_stroke(center, half, Stroke::new(1.0, COLOR_GRID_HUD));
        painter.circle_stroke(center, half * 0.5, Stroke::new(1.0, COLOR_GRID_HUD));
        painter.line_segment(
            [
                Pos2::new(r.min.x + 8.0, center.y),
                Pos2::new(r.max.x - 8.0, center.y),
            ],
            Stroke::new(1.0, COLOR_BORDER_SUBTLE),
        );
        painter.line_segment(
            [
                Pos2::new(center.x, r.min.y + 8.0),
                Pos2::new(center.x, r.max.y - 8.0),
            ],
            Stroke::new(1.0, COLOR_BORDER_SUBTLE),
        );

        let gx = x.clamp(-1.0, 1.0);
        let gy = y.clamp(-1.0, 1.0);
        let dot_pos = Pos2::new(center.x + gx * half, center.y - gy * half);
        painter.line_segment([center, dot_pos], Stroke::new(1.2, VECTOR_LINE_STROKE));
        let dot_col = if push_pressed {
            COLOR_POSITIVE
        } else {
            COLOR_ACCENT
        };
        painter.circle_filled(dot_pos, 4.5, dot_col);

        let (mag, deg) = vector_polar_coords(gx, gy);

        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(format!("X {:+.3}  Y {:+.3}", gx, gy))
                .color(COLOR_TEXT_PRIMARY)
                .size(FONT_META)
                .monospace(),
        );
        ui.label(
            RichText::new(format!("Mag {:.3}  {:03.0}°", mag, deg))
                .color(COLOR_TEXT_SECONDARY)
                .size(FONT_META)
                .monospace(),
        );

        let (push_txt, push_col) = if push_pressed {
            (x56_core::t!("curves-push-active"), COLOR_POSITIVE)
        } else if push_tested {
            (x56_core::t!("hwtest-status-tested"), COLOR_POSITIVE)
        } else {
            (x56_core::t!("curves-push-idle"), COLOR_TEXT_MUTED)
        };
        theme::meta_kv(ui, &x56_core::t!("curves-push-label"), &push_txt, push_col);
    });
}

/// Renders side-by-side graduated vertical bars for Left and Right throttle levers.
fn draw_vertical_throttle_bars(
    ui: &mut Ui,
    left_val: f32,
    right_val: f32,
    left_tested: bool,
    right_tested: bool,
) {
    let bars = [
        (x56_core::t!("hwtest-thr-left"), left_val, left_tested),
        (x56_core::t!("hwtest-thr-right"), right_val, right_tested),
    ];

    ui.vertical_centered(|ui| {
        let col_w = 58.0;
        let gap = 20.0;
        let total_w = col_w * 2.0 + gap;
        let bar_h = 152.0;
        let bar_w = 34.0;

        ui.allocate_ui_with_layout(
            Vec2::new(total_w, bar_h + 48.0),
            Layout::left_to_right(Align::TOP),
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, raw_val, tested) in bars {
                    let val = raw_val.clamp(0.0, 1.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(col_w, bar_h + 48.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            let (lbl_col, border_col) = if tested {
                                (COLOR_POSITIVE, COLOR_POSITIVE)
                            } else {
                                (COLOR_TEXT_SECONDARY, COLOR_BORDER_HAIRLINE)
                            };
                            ui.label(
                                RichText::new(&label)
                                    .color(lbl_col)
                                    .size(FONT_META)
                                    .monospace(),
                            );
                            ui.add_space(SPACE_XS);

                            let (resp, painter) =
                                ui.allocate_painter(Vec2::new(bar_w, bar_h), Sense::hover());
                            let r = resp.rect;

                            painter.rect_filled(r, CornerRadius::same(4), COLOR_SURFACE_LOWEST);
                            painter.rect_stroke(
                                r,
                                CornerRadius::same(4),
                                Stroke::new(1.0, border_col),
                                StrokeKind::Inside,
                            );

                            // Graduated 25% / 50% / 75% markings
                            for frac in [0.25_f32, 0.50, 0.75] {
                                let y_tick = r.max.y - frac * r.height();
                                painter.line_segment(
                                    [
                                        Pos2::new(r.min.x + 2.0, y_tick),
                                        Pos2::new(r.max.x - 2.0, y_tick),
                                    ],
                                    Stroke::new(1.0, COLOR_BORDER_SUBTLE),
                                );
                            }

                            // Bottom-to-top fill bar
                            let inner = r.shrink(3.0);
                            let fill_h = (inner.height() * val).max(0.0);
                            if fill_h > 0.5 {
                                let fill_rect = Rect::from_min_max(
                                    Pos2::new(inner.min.x, inner.max.y - fill_h),
                                    inner.max,
                                );
                                painter.rect_filled(fill_rect, CornerRadius::same(2), COLOR_ACCENT);
                                painter.line_segment(
                                    [
                                        Pos2::new(fill_rect.min.x, fill_rect.min.y),
                                        Pos2::new(fill_rect.max.x, fill_rect.min.y),
                                    ],
                                    Stroke::new(1.5, COLOR_TEXT_PRIMARY),
                                );
                            }

                            ui.add_space(SPACE_XS);
                            ui.label(
                                RichText::new(format!("{:5.1}%", val * 100.0))
                                    .color(COLOR_TEXT_PRIMARY)
                                    .size(FONT_META)
                                    .monospace(),
                            );
                        },
                    );
                }
            },
        );
    });
}

/// Renders a circular 270-degree instrument dial gauge for a rotary control (`-1.0 .. +1.0`).
fn draw_circular_rotary_gauge(
    ui: &mut Ui,
    dial_size: f32,
    label: &str,
    value: f32,
    push_pressed: Option<bool>,
    push_tested: Option<bool>,
    axis_tested: bool,
) {
    ui.vertical_centered(|ui| {
        let (resp, painter) = ui.allocate_painter(Vec2::new(dial_size, dial_size), Sense::hover());
        let r = resp.rect;
        let center = r.center();
        let radius = (dial_size * 0.5) - 6.0;

        painter.circle_filled(center, radius, COLOR_SURFACE_LOWEST);
        let ring_col = if axis_tested {
            COLOR_POSITIVE
        } else {
            COLOR_BORDER_HAIRLINE
        };
        painter.circle_stroke(center, radius, Stroke::new(1.0, ring_col));

        let arc_r = (radius - 6.0).max(12.0);
        let min_rad = dial_angle_rad(-1.0);
        let zero_rad = dial_angle_rad(0.0);
        let max_rad = dial_angle_rad(1.0);
        let cur_rad = dial_angle_rad(value);

        draw_arc_polyline(
            &painter,
            center,
            arc_r,
            min_rad,
            max_rad,
            Stroke::new(3.5, COLOR_SURFACE_HIGH),
        );

        // Scale ticks at min, zero, max
        for tick_rad in [min_rad, zero_rad, max_rad] {
            let p_in = Pos2::new(
                center.x + (arc_r - 3.5) * tick_rad.cos(),
                center.y + (arc_r - 3.5) * tick_rad.sin(),
            );
            let p_out = Pos2::new(
                center.x + (arc_r + 3.0) * tick_rad.cos(),
                center.y + (arc_r + 3.0) * tick_rad.sin(),
            );
            painter.line_segment([p_in, p_out], Stroke::new(1.0, COLOR_TEXT_MUTED));
        }

        // Active sweep from 12 o'clock (0.0) to current angle
        draw_arc_polyline(
            &painter,
            center,
            arc_r,
            zero_rad,
            cur_rad,
            Stroke::new(3.5, COLOR_ACCENT),
        );

        // Radial needle
        let needle_tip = Pos2::new(
            center.x + (arc_r - 2.0) * cur_rad.cos(),
            center.y + (arc_r - 2.0) * cur_rad.sin(),
        );
        painter.line_segment([center, needle_tip], Stroke::new(2.0, COLOR_TEXT_PRIMARY));
        painter.circle_filled(needle_tip, 2.5, COLOR_ACCENT);

        // Center hub (lights up in COLOR_POSITIVE for push-capable rotaries F/G)
        let (hub_fill, hub_stroke) = match (push_pressed, push_tested) {
            (Some(true), _) => (COLOR_POSITIVE, COLOR_POSITIVE),
            (_, Some(true)) => (COLOR_SURFACE_HIGH, COLOR_POSITIVE),
            _ => (COLOR_SURFACE_HIGH, COLOR_BORDER_HAIRLINE),
        };
        painter.circle_filled(center, 5.5, hub_fill);
        painter.circle_stroke(center, 5.5, Stroke::new(1.0, hub_stroke));

        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(label)
                .color(if axis_tested {
                    COLOR_POSITIVE
                } else {
                    COLOR_TEXT_SECONDARY
                })
                .size(FONT_META),
        );
        ui.label(
            RichText::new(format!("{:+.2}", value.clamp(-1.0, 1.0)))
                .color(COLOR_TEXT_PRIMARY)
                .size(FONT_META)
                .monospace(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hardware_test_tab_starts_empty() {
        let tab = HardwareTestTab::default();
        assert_eq!(tab.view_filter, TestViewFilter::Both);
        assert!(tab.tested_buttons.is_empty());
        assert!(tab.tested_axes.is_empty());
        assert!(tab.baseline_axes.is_none());
        assert!(tab.last_pressed_id.is_none());
        assert_eq!(tab.button_progress(DeviceKind::Stick), (0, 18));
        assert_eq!(tab.button_progress(DeviceKind::Throttle), (0, 33));
        assert_eq!(tab.axes_progress(), (0, 13));
    }

    #[test]
    fn tracks_pressed_buttons_and_resets_cleanly() {
        let mut tab = HardwareTestTab::new();
        let axes = LiveAxesState::default();
        let mut snap = LiveButtonsSnapshot::default();

        snap.pressed_buttons.insert("stick_trigger".to_string());
        snap.pressed_buttons.insert("thr_btn_e".to_string());
        tab.update_session_tracking(&axes, &snap);

        assert!(tab.tested_buttons.contains("stick_trigger"));
        assert!(tab.tested_buttons.contains("thr_btn_e"));
        assert_eq!(tab.button_progress(DeviceKind::Stick), (1, 18));
        assert_eq!(tab.button_progress(DeviceKind::Throttle), (1, 33));
        assert!(tab.last_pressed_id.is_some());

        // Releasing buttons keeps them marked as tested in session
        snap.pressed_buttons.clear();
        tab.update_session_tracking(&axes, &snap);
        assert_eq!(tab.button_progress(DeviceKind::Stick), (1, 18));
        assert_eq!(tab.button_progress(DeviceKind::Throttle), (1, 33));

        // Reset clears everything
        tab.reset_test();
        assert!(tab.tested_buttons.is_empty());
        assert!(tab.last_pressed_id.is_none());
        assert_eq!(tab.button_progress(DeviceKind::Stick), (0, 18));
        assert_eq!(tab.button_progress(DeviceKind::Throttle), (0, 33));
    }

    #[test]
    fn tracks_all_13_axes_when_deflected_beyond_threshold() {
        let mut tab = HardwareTestTab::new();
        let snap = LiveButtonsSnapshot::default();

        let base = LiveAxesState::default();
        tab.update_session_tracking(&base, &snap);
        assert_eq!(tab.axes_progress(), (0, 13));

        // Sub-threshold noise should not trigger tested state
        let mut noisy = base;
        noisy.stick_x = 0.03;
        noisy.rotary_1 = -0.05;
        tab.update_session_tracking(&noisy, &snap);
        assert_eq!(tab.axes_progress(), (0, 13));

        // Significant movement across all 13 axes
        let moved = LiveAxesState {
            stick_x: 0.65,
            stick_y: -0.42,
            stick_z: 0.50,
            ministick_x: -0.75,
            ministick_y: 0.80,
            throttle_left: 0.55,
            throttle_right: 0.90,
            throttle_ministick_x: 0.45,
            throttle_ministick_y: -0.60,
            rotary_1: 0.35,
            rotary_2: -0.45,
            rotary_3: 0.85,
            rotary_4: -0.95,
            ..Default::default()
        };
        tab.update_session_tracking(&moved, &snap);
        assert_eq!(tab.axes_progress(), (13, 13));

        tab.reset_test();
        assert_eq!(tab.axes_progress(), (0, 13));
        assert!(tab.baseline_axes.is_none());
    }

    #[test]
    fn dial_angle_math_maps_endpoints_and_center() {
        let center = dial_angle_rad(0.0);
        let min = dial_angle_rad(-1.0);
        let max = dial_angle_rad(1.0);

        assert!((center - (-FRAC_PI_2)).abs() < 1e-5);
        assert!((min - (-FRAC_PI_2 - 3.0 * FRAC_PI_4)).abs() < 1e-5);
        assert!((max - (-FRAC_PI_2 + 3.0 * FRAC_PI_4)).abs() < 1e-5);
        // Clamping beyond [-1.0, 1.0]
        assert!((dial_angle_rad(5.0) - max).abs() < 1e-5);
        assert!((dial_angle_rad(-5.0) - min).abs() < 1e-5);
    }

    #[test]
    fn vector_polar_coords_handles_cardinals() {
        let (mag_r, deg_r) = vector_polar_coords(1.0, 0.0);
        assert!((mag_r - 1.0).abs() < 1e-5);
        assert!((deg_r - 0.0).abs() < 1e-3);

        let (mag_d, deg_d) = vector_polar_coords(0.0, -1.0);
        assert!((mag_d - 1.0).abs() < 1e-5);
        assert!((deg_d - 270.0).abs() < 1e-3);
    }
}
