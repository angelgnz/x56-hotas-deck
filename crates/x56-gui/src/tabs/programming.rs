//! Button mapping and mode programming tab for Saitek / Logitech G X56 HOTAS.

use std::collections::HashSet;
use std::time::Instant;

use eframe::egui::{self, Color32, Key, RichText, Stroke, Ui, Vec2};
use x56_core::{
    catalog_for_device, parse_color, ButtonDescriptor, ButtonMonitor, CommandCell, DeviceKind,
    LiveButtonsSnapshot, MacroStep, ModeSlot, ProgrammingProfile, TestTriggerStatus,
    TriggeredCommandEvent,
};

use crate::curve_math::LiveInputPoller;
use crate::icons;
use crate::theme::{
    self, COLOR_ACCENT, COLOR_ACCENT_BORDER, COLOR_BORDER_HAIRLINE, COLOR_BORDER_STRONG,
    COLOR_BORDER_SUBTLE, COLOR_NEGATIVE, COLOR_ON_ACCENT, COLOR_POSITIVE, COLOR_SURFACE_HIGHEST,
    COLOR_SURFACE_LOWEST, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY, COLOR_TEXT_SECONDARY,
    COLOR_WARNING, FONT_BODY, FONT_HEADING, FONT_META, FONT_SMALL, RADIUS_CONTROL, SPACE_LG,
    SPACE_MD, SPACE_SM, SPACE_XS,
};

// Matte anodized graphite hardware palette for the 2D schematic canvas.
const CHASSIS_FILL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1d);
const CHASSIS_WELL: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const CONTROL_BASE: Color32 = Color32::from_rgb(0x22, 0x22, 0x26);
const CONTROL_CAP: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x30);
const CONTROL_BEVEL: Color32 = Color32::from_rgb(0x36, 0x36, 0x3c);
const CONTROL_HAIRLINE: Color32 = Color32::from_rgb(0x3f, 0x3f, 0x46);

/// Filter categories for the button mapping catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterCategory {
    #[default]
    Todos,
    Gatillos,
    Setas,
    BotonesFrontales,
    Macros,
    Shift,
}

impl FilterCategory {
    pub fn label(&self) -> String {
        match self {
            Self::Todos => x56_core::t!("prog-filter-all"),
            Self::Gatillos => x56_core::t!("prog-filter-triggers"),
            Self::Setas => x56_core::t!("prog-filter-hats"),
            Self::BotonesFrontales => x56_core::t!("prog-filter-face"),
            Self::Macros => x56_core::t!("prog-filter-macros"),
            Self::Shift => x56_core::t!("prog-filter-shift"),
        }
    }
}

/// State and UI logic for button mapping, mode layers, and macro recording.
pub struct ProgrammingTab {
    pub selected_device: DeviceKind,
    pub selected_mode: ModeSlot,
    pub follow_hardware_mode: bool,
    pub show_diagram_view: bool,
    pub profile: ProgrammingProfile,
    pub editing_button_id: Option<String>,
    pub pending_keys: Vec<String>,
    pub pending_name: String,
    pub hovered_button_id: Option<String>,
    pub show_test_window: bool,
    pub test_log: Vec<TriggeredCommandEvent>,
    pub status_message: Option<(String, bool)>,
    pub test_input_text: String,

    pub search_query: String,
    pub filter_category: FilterCategory,
    pub last_detected_button: Option<String>,
    pub last_hex_code: String,
    pub last_state_down: bool,
    pub is_recording_macro: bool,
    pub recorded_steps: Vec<MacroStep>,
    pub macro_recorder_open: bool,
    pub macro_target_button_id: Option<String>,
    pub macro_name_draft: String,
    pub macro_record_timings: bool,
    pub last_record_instant: Option<Instant>,
    pub active_detected_id: Option<String>,
    pub prev_pressed_buttons: HashSet<String>,
    pub currently_pressed_count: usize,
    pub active_pressed_summary: String,
}

/// Interaction state for schematic vector controls.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum InteractState {
    #[default]
    Idle,
    Hovered,
    Pressed,
}

impl InteractState {
    #[allow(dead_code)]
    pub fn is_active(self) -> bool {
        self != Self::Idle
    }
}

/// Interaction state for bidirectional schematic controls (K1, SC, TGL 1-4, SW 1-6).
#[derive(Copy, Clone, Debug, Default)]
pub struct Dir2Interact {
    pub up: InteractState,
    pub down: InteractState,
}

impl Dir2Interact {
    pub fn any(self) -> InteractState {
        self.up.max(self.down)
    }
}

/// Interaction state for 4-way hat/D-pad schematic controls (POV, H1, H2, H3, H4).
#[derive(Copy, Clone, Debug, Default)]
pub struct Dir4Interact {
    pub up: InteractState,
    pub right: InteractState,
    pub down: InteractState,
    pub left: InteractState,
}

impl Dir4Interact {
    pub fn any(self) -> InteractState {
        self.up.max(self.right).max(self.down).max(self.left)
    }
}

/// Resolved interaction map for an immediate-mode schematic render frame.
pub(crate) struct InteractionMap<'a> {
    pub pressed: &'a HashSet<String>,
    pub hovered: Option<&'a str>,
    pub profile: &'a ProgrammingProfile,
    pub mode: ModeSlot,
    pub highlighted_ids: Option<&'a HashSet<String>>,
}

impl InteractionMap<'_> {
    pub fn is_assigned(&self, id: &str) -> bool {
        if let Some(highlighted) = self.highlighted_ids {
            highlighted.contains(id)
        } else {
            self.profile
                .get_cell(self.mode, id)
                .is_some_and(|c| c.is_assigned())
        }
    }

    pub fn assigned_dir2(&self, up: &str, down: &str) -> (bool, bool) {
        (self.is_assigned(up), self.is_assigned(down))
    }

    pub fn assigned_dir4(&self, u: &str, r: &str, d: &str, l: &str) -> (bool, bool, bool, bool) {
        (
            self.is_assigned(u),
            self.is_assigned(r),
            self.is_assigned(d),
            self.is_assigned(l),
        )
    }

    pub fn get(&self, id: &str) -> InteractState {
        if self.pressed.contains(id) {
            InteractState::Pressed
        } else if self.hovered == Some(id) {
            InteractState::Hovered
        } else {
            InteractState::Idle
        }
    }

    pub fn dir2(&self, id_up: &str, id_down: &str) -> Dir2Interact {
        Dir2Interact {
            up: self.get(id_up),
            down: self.get(id_down),
        }
    }

    pub fn dir4(&self, id_u: &str, id_r: &str, id_d: &str, id_l: &str) -> Dir4Interact {
        Dir4Interact {
            up: self.get(id_u),
            right: self.get(id_r),
            down: self.get(id_d),
            left: self.get(id_l),
        }
    }
}

impl Default for ProgrammingTab {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgrammingTab {
    pub fn new() -> Self {
        Self {
            selected_device: DeviceKind::Stick,
            selected_mode: ModeSlot::M1,
            follow_hardware_mode: true,
            show_diagram_view: true,
            profile: ProgrammingProfile::default(),
            editing_button_id: None,
            pending_keys: Vec::new(),
            pending_name: String::new(),
            hovered_button_id: None,
            show_test_window: false,
            test_log: Vec::new(),
            status_message: None,
            test_input_text: String::new(),

            search_query: String::new(),
            filter_category: FilterCategory::Todos,
            last_detected_button: None,
            last_hex_code: "----".to_string(),
            last_state_down: false,
            is_recording_macro: false,
            recorded_steps: Vec::new(),
            macro_recorder_open: false,
            macro_target_button_id: None,
            macro_name_draft: String::new(),
            macro_record_timings: true,
            last_record_instant: None,
            active_detected_id: None,
            prev_pressed_buttons: HashSet::new(),
            currently_pressed_count: 0,
            active_pressed_summary: String::new(),
        }
    }

    /// Loads a programming profile and synchronizes it with the button monitor.
    pub fn load_programming_profile(&mut self, prof: ProgrammingProfile, monitor: &ButtonMonitor) {
        self.profile = prof;
        monitor.set_profile(self.profile.clone());
        self.editing_button_id = None;
        self.pending_keys.clear();
        self.pending_name.clear();
        self.status_message = Some((x56_core::t!("prog-profile-synced"), false));
    }

    /// Returns a copy of the active programming profile.
    pub fn current_programming_profile(&self) -> ProgrammingProfile {
        self.profile.clone()
    }

    /// Main UI entry point for the button programming tab.
    pub fn ui(&mut self, ui: &mut Ui, monitor: &ButtonMonitor, _poller: &LiveInputPoller) {
        let avail_w = ui.available_width();
        ui.set_max_width(avail_w);

        let snapshot = monitor.snapshot();

        if self.follow_hardware_mode {
            self.selected_mode = snapshot.active_mode;
        }

        self.update_live_detection_state(&snapshot);

        if self.is_recording_macro {
            self.handle_live_macro_recording(ui);
        } else {
            self.handle_live_key_capture(ui);
        }

        self.render_top_toolbar(ui, monitor, &snapshot);

        ui.add_space(SPACE_SM);

        let mut dismiss_status = false;
        if let Some((ref msg, is_err)) = self.status_message {
            let dot_col = if is_err {
                COLOR_NEGATIVE
            } else {
                COLOR_POSITIVE
            };
            theme::subcard_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    theme::status_dot(ui, dot_col, 8.0);
                    ui.label(
                        RichText::new(msg)
                            .color(COLOR_TEXT_PRIMARY)
                            .size(FONT_SMALL),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(icons::X)
                                        .color(COLOR_TEXT_MUTED)
                                        .size(FONT_SMALL),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            dismiss_status = true;
                        }
                    });
                });
            });
            ui.add_space(SPACE_SM);
        }
        if dismiss_status {
            self.status_message = None;
        }

        let total_w = ui.available_width();

        if self.show_diagram_view {
            if total_w >= 850.0 {
                let gap = SPACE_MD;
                let left_w = ((total_w - gap) * 0.5).max(240.0);

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;

                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(left_w);
                            egui::ScrollArea::vertical()
                                .id_salt("left_col_scroll")
                                .max_width(left_w)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    self.render_evdev_hook_card(ui, &snapshot);
                                    ui.add_space(SPACE_MD);

                                    self.render_vector_map_card(ui, &snapshot);
                                    ui.add_space(SPACE_MD);

                                    self.render_quick_macro_actions(ui);
                                    ui.add_space(SPACE_SM);
                                });
                        },
                    );

                    let right_w = ui.available_width().max(240.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(right_w);
                            self.render_search_and_stats(ui);
                            ui.add_space(SPACE_SM);

                            self.render_filter_pills(ui);
                            ui.add_space(SPACE_SM);

                            self.render_macro_recorder_panel(ui, monitor);

                            egui::ScrollArea::vertical()
                                .id_salt("mapping_rows_scroll")
                                .max_width(right_w)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    self.render_mapping_cards(ui, monitor, &snapshot);
                                });
                        },
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("programming_stacked_scroll")
                    .max_width(total_w)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.render_evdev_hook_card(ui, &snapshot);
                        ui.add_space(SPACE_MD);

                        self.render_vector_map_card(ui, &snapshot);
                        ui.add_space(SPACE_MD);

                        self.render_quick_macro_actions(ui);
                        ui.add_space(SPACE_SM);
                        self.render_macro_recorder_panel(ui, monitor);
                        ui.add_space(SPACE_MD);
                        ui.separator();
                        ui.add_space(SPACE_MD);

                        self.render_search_and_stats(ui);
                        ui.add_space(SPACE_SM);

                        self.render_filter_pills(ui);
                        ui.add_space(SPACE_SM);

                        self.render_mapping_cards(ui, monitor, &snapshot);
                    });
            }
        } else {
            ui.allocate_ui_with_layout(
                Vec2::new(total_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(total_w);
                    self.render_search_and_stats(ui);
                    ui.add_space(SPACE_SM);
                    self.render_filter_pills(ui);
                    ui.add_space(SPACE_SM);
                    self.render_macro_recorder_panel(ui, monitor);
                    egui::ScrollArea::vertical()
                        .id_salt("mapping_rows_scroll_full")
                        .max_width(total_w)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.render_mapping_cards(ui, monitor, &snapshot);
                        });
                },
            );
        }

        if self.show_test_window {
            self.render_test_modal(ui, monitor, &snapshot);
        }
    }

    /// Updates live detection telemetry from physical button snapshot.
    fn update_live_detection_state(&mut self, snapshot: &LiveButtonsSnapshot) {
        self.currently_pressed_count = snapshot.pressed_buttons.len();
        if !snapshot.pressed_buttons.is_empty() {
            let all_desc = x56_core::all_button_descriptors();
            let mut labels: Vec<String> = snapshot
                .pressed_buttons
                .iter()
                .map(|id| {
                    all_desc
                        .iter()
                        .find(|d| &d.id == id)
                        .map(|d| {
                            let loc = localized_node_name_by_id(&d.id, d.label);
                            format!("{loc} ({})", d.id.to_uppercase())
                        })
                        .unwrap_or_else(|| id.to_uppercase())
                })
                .collect();
            labels.sort();
            self.active_pressed_summary = labels.join("\n");
        } else {
            self.active_pressed_summary.clear();
        }

        if snapshot.pressed_buttons.is_empty() {
            self.last_state_down = false;
            self.active_detected_id = None;
            self.prev_pressed_buttons.clear();
            return;
        }

        self.last_state_down = true;

        let mut newly_pressed: Vec<String> = snapshot
            .pressed_buttons
            .iter()
            .filter(|id| !self.prev_pressed_buttons.contains(*id))
            .cloned()
            .collect();

        if !newly_pressed.is_empty() {
            newly_pressed.sort();
            self.active_detected_id = Some(newly_pressed[0].clone());
        } else if let Some(ref current) = self.active_detected_id {
            if !snapshot.pressed_buttons.contains(current) {
                let mut remaining: Vec<String> = snapshot.pressed_buttons.iter().cloned().collect();
                remaining.sort();
                self.active_detected_id = Some(remaining[0].clone());
            }
        } else {
            let mut remaining: Vec<String> = snapshot.pressed_buttons.iter().cloned().collect();
            remaining.sort();
            self.active_detected_id = Some(remaining[0].clone());
        }

        self.prev_pressed_buttons = snapshot.pressed_buttons.clone();

        if let Some(ref btn_id) = self.active_detected_id {
            let all_desc = x56_core::all_button_descriptors();
            if let Some(desc) = all_desc.iter().find(|d| &d.id == btn_id) {
                let loc = localized_node_name_by_id(&desc.id, desc.label);
                self.last_detected_button = Some(format!("{} · {loc}", desc.id.to_uppercase()));
                self.last_hex_code = if desc.evdev_code != 0 {
                    format!("{:#06x}", desc.evdev_code)
                } else {
                    "ABS_HAT0".to_string()
                };
                self.hovered_button_id = Some(desc.id.clone());
            } else {
                self.last_detected_button = Some(btn_id.to_uppercase());
                self.hovered_button_id = Some(btn_id.clone());
            }

            if self.macro_recorder_open && self.macro_target_button_id.is_none() {
                self.macro_target_button_id = Some(btn_id.clone());
            }
        }
    }

    /// Captures live keyboard presses for the mapping cell currently being edited.
    fn handle_live_key_capture(&mut self, ui: &mut Ui) {
        if self.editing_button_id.is_none() || ui.ctx().egui_wants_keyboard_input() {
            return;
        }

        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = event
                {
                    if *key == Key::Backspace && !self.pending_keys.is_empty() {
                        self.pending_keys.pop();
                        continue;
                    }

                    if let Some(name) = map_key_to_name(*key) {
                        let name_str = name.to_string();
                        if !self.pending_keys.contains(&name_str) {
                            self.pending_keys.push(name_str);
                        }
                    }
                }
            }
        });
    }

    /// Captures live keyboard chords while macro sequence recording is active.
    fn handle_live_macro_recording(&mut self, ui: &mut Ui) {
        if !self.is_recording_macro || ui.ctx().egui_wants_keyboard_input() {
            return;
        }

        let captured_chords: Vec<Vec<String>> = ui.input(|i| {
            let mut chords = Vec::new();
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                {
                    if matches!(
                        key,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::AltLeft
                            | Key::AltRight
                    ) {
                        continue;
                    }

                    if let Some(name) = map_key_to_name(*key) {
                        let mut chord = Vec::new();
                        if modifiers.ctrl {
                            chord.push("Ctrl".to_string());
                        }
                        if modifiers.shift {
                            chord.push("Shift".to_string());
                        }
                        if modifiers.alt {
                            chord.push("Alt".to_string());
                        }
                        if !chord.iter().any(|k| k == name) {
                            chord.push(name.to_string());
                        }
                        chords.push(chord);
                    }
                }
            }
            chords
        });

        let now = Instant::now();
        for chord in captured_chords {
            self.record_key_chord(chord, now);
        }
    }

    /// Records a key chord as a step in the macro sequence.
    pub(crate) fn record_key_chord(&mut self, chord: Vec<String>, now: Instant) {
        if chord.is_empty() {
            return;
        }

        let delay_ms = if self.recorded_steps.is_empty() {
            0
        } else if self.macro_record_timings {
            self.last_record_instant
                .map(|prev| now.saturating_duration_since(prev).as_millis().min(5_000) as u64)
                .unwrap_or(0)
        } else {
            50
        };

        self.last_record_instant = Some(now);
        self.recorded_steps.push(MacroStep {
            keys: chord,
            delay_ms,
        });
    }

    // ========================================================================
    // 1. TOP TOOLBAR
    // ========================================================================

    fn render_top_toolbar(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        theme::subcard_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, SPACE_XS);

                // Device switcher: Stick / Throttle
                let is_stick = self.selected_device == DeviceKind::Stick;
                if theme::segmented_button(ui, &x56_core::t!("curves-stick-short"), is_stick)
                    .clicked()
                    && !is_stick
                {
                    self.selected_device = DeviceKind::Stick;
                    self.editing_button_id = None;
                    self.hovered_button_id = None;
                }

                let is_throttle = self.selected_device == DeviceKind::Throttle;
                if theme::segmented_button(ui, &x56_core::t!("curves-throttle-short"), is_throttle)
                    .clicked()
                    && !is_throttle
                {
                    self.selected_device = DeviceKind::Throttle;
                    self.editing_button_id = None;
                    self.hovered_button_id = None;
                }

                ui.add_space(SPACE_XS);

                // View switcher: Split / List
                let split_lbl = short_view_label(&x56_core::t!("prog-view-dual"));
                let list_lbl = short_view_label(&x56_core::t!("prog-view-cells"));
                if theme::segmented_button(ui, &split_lbl, self.show_diagram_view)
                    .on_hover_text(x56_core::t!("prog-view-tooltip"))
                    .clicked()
                    && !self.show_diagram_view
                {
                    self.show_diagram_view = true;
                }
                if theme::segmented_button(ui, &list_lbl, !self.show_diagram_view)
                    .on_hover_text(x56_core::t!("prog-view-tooltip"))
                    .clicked()
                    && self.show_diagram_view
                {
                    self.show_diagram_view = false;
                }

                ui.add_space(SPACE_XS);

                // Rotary mode detents: M1 / M2 / S1 with optional 6px LED color dot
                for slot in ModeSlot::ALL {
                    let is_sel = self.selected_mode == slot;
                    let bindings = self.profile.bindings_for(slot);
                    let led_col = bindings
                        .rgb_for(self.selected_device)
                        .or_else(|| bindings.rgb_for(DeviceKind::Stick))
                        .and_then(|hex| parse_color(hex).ok())
                        .map(|c| Color32::from_rgb(c.r, c.g, c.b));

                    let detent_label = if led_col.is_some() {
                        format!("  {}", slot.as_str())
                    } else {
                        slot.as_str().to_string()
                    };

                    let resp = theme::mode_detent(ui, &detent_label, is_sel)
                        .on_hover_text(slot.localized_description());
                    if let Some(dot_col) = led_col {
                        if ui.is_rect_visible(resp.rect) {
                            let dot_pos = egui::pos2(resp.rect.min.x + 11.0, resp.rect.center().y);
                            ui.painter().circle_filled(dot_pos, 3.0, dot_col);
                        }
                    }
                    if resp.clicked() {
                        self.selected_mode = slot;
                        self.follow_hardware_mode = false;
                        self.editing_button_id = None;
                    }
                }

                // Sync dial toggle
                let sync_label = if self.follow_hardware_mode {
                    x56_core::t!("prog-sync-hw-on")
                } else {
                    x56_core::t!("prog-sync-hw-off")
                };
                if theme::segmented_button(ui, &sync_label, self.follow_hardware_mode)
                    .on_hover_text(x56_core::t!("prog-sync-hw-tooltip"))
                    .clicked()
                {
                    self.follow_hardware_mode = !self.follow_hardware_mode;
                    if self.follow_hardware_mode {
                        self.selected_mode = snapshot.active_mode;
                    }
                }

                ui.add_space(SPACE_XS);

                // uinput router semantic status / toggle
                let uinput_ready = monitor.uinput_available();
                if !uinput_ready {
                    let router_text = format!(
                        "{}: {}",
                        x56_core::t!("prog-router-uinput"),
                        x56_core::t!("prog-router-offline")
                    );
                    theme::status_dot(ui, COLOR_NEGATIVE, 8.0);
                    ui.label(
                        RichText::new(router_text)
                            .color(COLOR_TEXT_MUTED)
                            .size(FONT_META),
                    )
                    .on_hover_text(x56_core::t!("sidebar-uinput-offline-tooltip"));
                } else {
                    let is_active = monitor.is_active();
                    let dot_col = if is_active {
                        COLOR_POSITIVE
                    } else {
                        COLOR_WARNING
                    };
                    let toggle_lbl = if is_active {
                        x56_core::t!("prog-uinput-active-short")
                    } else {
                        x56_core::t!("prog-uinput-paused-short")
                    };
                    theme::status_dot(ui, dot_col, 8.0);
                    if theme::ghost_button(ui, &toggle_lbl)
                        .on_hover_text(x56_core::t!("prog-uinput-toggle-tooltip"))
                        .clicked()
                    {
                        if is_active {
                            monitor.set_active(false);
                            self.status_message =
                                Some((x56_core::t!("prog-uinput-paused-msg"), false));
                        } else {
                            monitor.set_profile(self.profile.clone());
                            monitor.set_active(true);
                            self.status_message =
                                Some((x56_core::t!("prog-uinput-resumed-msg"), false));
                        }
                    }
                }

                // Test loop modal trigger
                if theme::secondary_button(ui, &x56_core::t!("prog-test-loop")).clicked() {
                    self.show_test_window = true;
                }
            });
        });
    }

    // ========================================================================
    // 2. LIVE DETECTION MONITOR CARD
    // ========================================================================

    fn render_evdev_hook_card(&self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                theme::section_heading(ui, &x56_core::t!("prog-monitor-title"));
                ui.add_space(SPACE_SM);
                let connected = snapshot.stick_connected || snapshot.throttle_connected;
                let (dot_col, status_txt) = if connected {
                    (COLOR_POSITIVE, x56_core::t!("prog-hook-active"))
                } else {
                    (COLOR_WARNING, x56_core::t!("prog-hook-no-usb"))
                };
                theme::status_dot(ui, dot_col, 8.0);
                ui.label(
                    RichText::new(status_txt)
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_META),
                );
            });

            ui.add_space(SPACE_SM);

            theme::subcard_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(x56_core::t!("prog-monitor-event"))
                            .color(COLOR_TEXT_MUTED)
                            .size(FONT_META),
                    );
                    if let Some(ref det) = self.last_detected_button {
                        ui.label(
                            RichText::new(det)
                                .color(COLOR_ACCENT)
                                .size(FONT_SMALL)
                                .monospace(),
                        );
                    } else {
                        ui.label(
                            RichText::new(x56_core::t!("prog-monitor-idle"))
                                .color(COLOR_TEXT_SECONDARY)
                                .size(FONT_SMALL),
                        );
                    }

                    if self.currently_pressed_count > 1 {
                        let count_str = self.currently_pressed_count.to_string();
                        let tip = x56_core::t!(
                            "prog-monitor-multi",
                            "count" => &count_str,
                            "list" => &self.active_pressed_summary
                        );
                        ui.label(
                            RichText::new(x56_core::t!(
                                "prog-monitor-active-count",
                                "count" => &count_str
                            ))
                            .color(COLOR_WARNING)
                            .size(FONT_META)
                            .monospace(),
                        )
                        .on_hover_text(tip);
                    }
                });
            });

            ui.add_space(SPACE_SM);

            ui.columns(3, |cols| {
                let state_val = if self.last_state_down {
                    x56_core::t!("prog-monitor-val-down")
                } else {
                    x56_core::t!("prog-monitor-val-up")
                };

                let items = [
                    (
                        x56_core::t!("prog-monitor-hex-code"),
                        self.last_hex_code.clone(),
                    ),
                    (
                        x56_core::t!("prog-monitor-hw-mode"),
                        snapshot.active_mode.as_str().to_string(),
                    ),
                    (x56_core::t!("prog-monitor-state"), state_val),
                ];

                for (idx, (label, val)) in items.into_iter().enumerate() {
                    theme::subcard_frame().show(&mut cols[idx], |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.label(RichText::new(label).color(COLOR_TEXT_MUTED).size(FONT_META));
                            ui.label(
                                RichText::new(val)
                                    .color(COLOR_TEXT_PRIMARY)
                                    .size(FONT_HEADING)
                                    .monospace(),
                            );
                        });
                    });
                }
            });
        });
    }

    // ========================================================================
    // 3. PHYSICAL STICK & DUAL THROTTLE SCHEMATIC
    // ========================================================================

    #[inline]
    pub(crate) fn compact_vector_map_x(rect: egui::Rect, norm_x: f32, _device: DeviceKind) -> f32 {
        rect.min.x + norm_x * rect.width()
    }

    fn render_vector_map_card(&mut self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = match self.selected_device {
                DeviceKind::Stick => x56_core::t!("prog-schematic-stick-title"),
                DeviceKind::Throttle => x56_core::t!("prog-schematic-throttle-title"),
            };
            theme::section_heading(ui, &title);
            ui.add_space(SPACE_SM);

            theme::canvas_frame().show(ui, |ui| {
                let avail_w = ui.available_width();
                let aspect = 790.0 / 1180.0;
                let canvas_height = (avail_w * aspect).clamp(120.0, 540.0);
                let canvas_width = (canvas_height / aspect).min(avail_w);
                let canvas_size = Vec2::new(canvas_width, canvas_height);
                let (response, painter) = ui.allocate_painter(canvas_size, egui::Sense::click());
                let rect = response.rect;

                painter.rect_filled(rect, RADIUS_CONTROL as f32, COLOR_SURFACE_LOWEST);

                // Ultra-subtle hairline reference grid
                let step = 64.0;
                let mut gx = rect.min.x + step;
                while gx < rect.max.x {
                    painter.line_segment(
                        [egui::pos2(gx, rect.min.y), egui::pos2(gx, rect.max.y)],
                        Stroke::new(0.5, COLOR_BORDER_SUBTLE),
                    );
                    gx += step;
                }
                let mut gy = rect.min.y + step;
                while gy < rect.max.y {
                    painter.line_segment(
                        [egui::pos2(rect.min.x, gy), egui::pos2(rect.max.x, gy)],
                        Stroke::new(0.5, COLOR_BORDER_SUBTLE),
                    );
                    gy += step;
                }

                let s = rect.width() / 1180.0;
                let nodes = get_device_schematic_nodes(self.selected_device);
                let mouse_pos = ui.input(|i| i.pointer.hover_pos());
                let clicked = response.clicked();

                let mut newly_hovered: Option<String> = None;
                let mut best_dist = f32::INFINITY;
                let mut clicked_pin: Option<String> = None;

                if let Some(mpos) = mouse_pos {
                    if rect.contains(mpos) {
                        for node in &nodes {
                            let center = egui::pos2(
                                Self::compact_vector_map_x(rect, node.norm_x, self.selected_device),
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
                                newly_hovered = Some(node.id.to_string());
                                if clicked {
                                    clicked_pin = Some(node.id.to_string());
                                }
                            }
                        }
                    }
                }

                if let Some(ref h) = newly_hovered {
                    self.hovered_button_id = Some(h.clone());
                } else if let Some(mpos) = mouse_pos {
                    if rect.contains(mpos) {
                        self.hovered_button_id = None;
                    }
                }
                let active_hovered = self.hovered_button_id.as_deref();

                let im = InteractionMap {
                    pressed: &snapshot.pressed_buttons,
                    hovered: active_hovered,
                    profile: &self.profile,
                    mode: self.selected_mode,
                    highlighted_ids: None,
                };

                match self.selected_device {
                    DeviceKind::Stick => Self::draw_vector_stick_silhouette(&painter, rect, &im),
                    DeviceKind::Throttle => {
                        Self::draw_vector_throttle_silhouette(&painter, rect, &im);
                    }
                }

                for node in &nodes {
                    if active_hovered == Some(node.id) {
                        let meta = control_meta_for(node.id);
                        let loc_name = localized_node_name_by_id(node.id, node.full_name);
                        let (mapped_str, is_assigned) = if let Some(cell) =
                            self.profile.get_cell(self.selected_mode, node.id)
                        {
                            if cell.is_assigned() {
                                (format!("{} [{}]", cell.name, cell.keys_display()), true)
                            } else {
                                (unassigned_label(), false)
                            }
                        } else {
                            (unassigned_label(), false)
                        };

                        response.clone().on_hover_ui_at_pointer(|ui| {
                            theme::subcard_frame().show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&loc_name)
                                            .color(COLOR_TEXT_PRIMARY)
                                            .size(FONT_BODY),
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
                                        RichText::new(mapped_str)
                                            .color(if is_assigned {
                                                COLOR_ACCENT
                                            } else {
                                                COLOR_TEXT_SECONDARY
                                            })
                                            .size(FONT_SMALL),
                                    );
                                    ui.label(
                                        RichText::new(x56_core::t!(
                                            "prog-schematic-meta-group",
                                            "group" => &meta.group,
                                            "kind" => &meta.kind
                                        ))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(FONT_META),
                                    );
                                });
                            });
                        });
                    }
                }

                if let Some(pin_id) = clicked_pin {
                    if self.macro_recorder_open || self.is_recording_macro {
                        self.macro_target_button_id = Some(pin_id.clone());
                        self.hovered_button_id = Some(pin_id);
                    } else {
                        self.search_query = pin_id.clone();
                        self.editing_button_id = Some(pin_id.clone());
                        if let Some(cell) = self.profile.get_cell(self.selected_mode, &pin_id) {
                            if cell.is_assigned() {
                                self.pending_name = cell.name.clone();
                                self.pending_keys = cell.keys.clone();
                            } else {
                                self.pending_name.clear();
                                self.pending_keys.clear();
                            }
                        } else {
                            self.pending_name.clear();
                            self.pending_keys.clear();
                        }
                    }
                }
            });
        });
    }

    #[inline]
    fn schematic_control_style(
        st: InteractState,
        is_assigned: bool,
    ) -> (Color32, Color32, Color32) {
        match st {
            InteractState::Pressed => (COLOR_ACCENT, COLOR_ACCENT, COLOR_ON_ACCENT),
            InteractState::Hovered => (
                COLOR_SURFACE_HIGHEST,
                COLOR_TEXT_PRIMARY,
                COLOR_TEXT_PRIMARY,
            ),
            InteractState::Idle => (
                CONTROL_BASE,
                if is_assigned {
                    COLOR_ACCENT_BORDER
                } else {
                    CONTROL_HAIRLINE
                },
                COLOR_TEXT_SECONDARY,
            ),
        }
    }

    /// Draws the anodized graphite Stick chassis silhouette and interactive controls.
    pub(crate) fn draw_vector_stick_silhouette(
        painter: &egui::Painter,
        rect: egui::Rect,
        im: &InteractionMap,
    ) {
        let s = rect.width() / 1180.0;
        let stroke_w = (1.2 * s).max(1.0);
        let outline = Stroke::new(stroke_w, CONTROL_HAIRLINE);
        let p = |nx: f32, ny: f32| {
            egui::pos2(
                rect.min.x + nx * rect.width(),
                rect.min.y + ny * rect.height(),
            )
        };

        // Upper stick head
        let head_rect = egui::Rect::from_min_max(p(0.26, 0.04), p(0.74, 0.36));
        painter.rect_filled(head_rect, 18.0 * s, CHASSIS_FILL);
        painter.rect_stroke(head_rect, 18.0 * s, outline, egui::StrokeKind::Inside);

        let inner_head = egui::Rect::from_min_max(p(0.28, 0.06), p(0.72, 0.34));
        painter.rect_stroke(
            inner_head,
            12.0 * s,
            Stroke::new(stroke_w, CONTROL_BEVEL),
            egui::StrokeKind::Inside,
        );

        // Grip shaft
        let shaft_rect = egui::Rect::from_min_max(p(0.38, 0.34), p(0.62, 0.78));
        painter.rect_filled(shaft_rect, 14.0 * s, CHASSIS_WELL);
        painter.rect_stroke(shaft_rect, 14.0 * s, outline, egui::StrokeKind::Inside);

        painter.line_segment(
            [p(0.40, 0.38), p(0.40, 0.74)],
            Stroke::new(stroke_w, CONTROL_BEVEL),
        );
        painter.line_segment(
            [p(0.60, 0.38), p(0.60, 0.74)],
            Stroke::new(stroke_w, CONTROL_BEVEL),
        );

        // Palm rest base
        let base_rect = egui::Rect::from_min_max(p(0.32, 0.77), p(0.68, 0.93));
        painter.rect_filled(base_rect, 16.0 * s, CHASSIS_FILL);
        painter.rect_stroke(base_rect, 16.0 * s, outline, egui::StrokeKind::Inside);

        // POV 1 (8-way)
        Self::draw_dpad(
            painter,
            p(0.330, 0.242),
            0.30 * s,
            im.dir4(
                "stick_pov_up",
                "stick_pov_right",
                "stick_pov_down",
                "stick_pov_left",
            ),
            im.assigned_dir4(
                "stick_pov_up",
                "stick_pov_right",
                "stick_pov_down",
                "stick_pov_left",
            ),
            "POV",
        );

        // Button A
        Self::draw_circular_button(
            painter,
            p(0.415, 0.138),
            0.32 * s,
            im.get("stick_btn_a"),
            im.is_assigned("stick_btn_a"),
            "A",
            true,
        );

        // Hat 1
        Self::draw_dpad(
            painter,
            p(0.525, 0.121),
            0.30 * s,
            im.dir4(
                "stick_h1_up",
                "stick_h1_right",
                "stick_h1_down",
                "stick_h1_left",
            ),
            im.assigned_dir4(
                "stick_h1_up",
                "stick_h1_right",
                "stick_h1_down",
                "stick_h1_left",
            ),
            "H1",
        );

        // Button B
        Self::draw_circular_button(
            painter,
            p(0.670, 0.072),
            0.30 * s,
            im.get("stick_btn_b"),
            im.is_assigned("stick_btn_b"),
            "B",
            false,
        );

        // Hat 2
        Self::draw_dpad(
            painter,
            p(0.630, 0.285),
            0.30 * s,
            im.dir4(
                "stick_h2_up",
                "stick_h2_right",
                "stick_h2_down",
                "stick_h2_left",
            ),
            im.assigned_dir4(
                "stick_h2_up",
                "stick_h2_right",
                "stick_h2_down",
                "stick_h2_left",
            ),
            "H2",
        );

        // Primary trigger
        Self::draw_stick_trigger(
            painter,
            p(0.500, 0.420),
            s,
            im.get("stick_trigger"),
            im.is_assigned("stick_trigger"),
        );

        // Button C (ministick push)
        Self::draw_circular_button(
            painter,
            p(0.440, 0.560),
            0.28 * s,
            im.get("stick_btn_c"),
            im.is_assigned("stick_btn_c"),
            "C",
            false,
        );

        // Pinkie lever
        Self::draw_stick_pinkie_lever(
            painter,
            p(0.500, 0.700),
            s,
            im.get("stick_pinkie"),
            im.is_assigned("stick_pinkie"),
        );

        // Button D
        Self::draw_circular_button(
            painter,
            p(0.570, 0.840),
            0.28 * s,
            im.get("stick_btn_d"),
            im.is_assigned("stick_btn_d"),
            "D",
            false,
        );
    }

    /// Draws the Stick primary trigger with live mechanical travel offset.
    fn draw_stick_trigger(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        st: InteractState,
        is_assigned: bool,
    ) {
        let is_pressed = st == InteractState::Pressed;
        let (fill_col, stroke_col, text_col) = Self::schematic_control_style(st, is_assigned);
        let guard_w = 68.0 * s;
        let guard_h = 36.0 * s;

        let guard_rect = egui::Rect::from_center_size(center, egui::vec2(guard_w, guard_h));
        painter.rect_filled(guard_rect, 6.0 * s, CHASSIS_WELL);
        painter.rect_stroke(
            guard_rect,
            6.0 * s,
            Stroke::new((1.5 * s).max(1.0), stroke_col),
            egui::StrokeKind::Inside,
        );

        let blade_w = 52.0 * s;
        let blade_h = 24.0 * s;
        let blade_y = if is_pressed {
            center.y - 3.0 * s
        } else {
            center.y
        };
        let blade_rect = egui::Rect::from_center_size(
            egui::pos2(center.x, blade_y),
            egui::vec2(blade_w, blade_h),
        );

        painter.rect_filled(blade_rect, 4.0 * s, fill_col);
        painter.rect_stroke(
            blade_rect,
            4.0 * s,
            Stroke::new((1.4 * s).max(1.0), stroke_col),
            egui::StrokeKind::Inside,
        );
        painter.text(
            egui::pos2(center.x, blade_y),
            egui::Align2::CENTER_CENTER,
            "TRG",
            egui::FontId::monospace((14.0 * s).max(8.5)),
            text_col,
        );
    }

    /// Draws the Stick pinkie lever with live mechanical travel offset.
    fn draw_stick_pinkie_lever(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        st: InteractState,
        is_assigned: bool,
    ) {
        let is_pressed = st == InteractState::Pressed;
        let (fill_col, stroke_col, text_col) = Self::schematic_control_style(st, is_assigned);
        let lever_w = 64.0 * s;
        let lever_h = 26.0 * s;

        let lever_y = if is_pressed {
            center.y - 2.0 * s
        } else {
            center.y
        };
        let lever_rect = egui::Rect::from_center_size(
            egui::pos2(center.x, lever_y),
            egui::vec2(lever_w, lever_h),
        );

        painter.rect_filled(lever_rect, 6.0 * s, fill_col);
        painter.rect_stroke(
            lever_rect,
            6.0 * s,
            Stroke::new((1.5 * s).max(1.0), stroke_col),
            egui::StrokeKind::Inside,
        );
        painter.text(
            egui::pos2(center.x, lever_y),
            egui::Align2::CENTER_CENTER,
            "PNK",
            egui::FontId::monospace((14.0 * s).max(8.5)),
            text_col,
        );
    }

    /// Draws the restrained Dual Throttle chassis silhouette and all 32 interactive controls.
    pub(crate) fn draw_vector_throttle_silhouette(
        painter: &egui::Painter,
        rect: egui::Rect,
        im: &InteractionMap,
    ) {
        let s = rect.width() / 1180.0;
        let stroke_w = (1.2 * s).max(1.0);
        let outline = Stroke::new(stroke_w, CONTROL_HAIRLINE);
        let inner_bevel = Stroke::new(stroke_w, CONTROL_BEVEL);
        let p = |x: f32, y: f32| {
            egui::pos2(
                rect.min.x + (x / 1180.0) * rect.width(),
                rect.min.y + (y / 790.0) * rect.height(),
            )
        };

        // Base console housing behind toggle and switch banks
        let base_deck = egui::Rect::from_min_max(p(110.0, 155.0), p(1118.0, 756.0));
        painter.rect_filled(base_deck, 20.0 * s, CHASSIS_WELL);
        painter.rect_stroke(base_deck, 20.0 * s, outline, egui::StrokeKind::Inside);

        let base_inset = egui::Rect::from_min_max(p(130.0, 174.0), p(1098.0, 738.0));
        painter.rect_stroke(base_inset, 14.0 * s, inner_bevel, egui::StrokeKind::Inside);

        // Right toggle bank recess
        let toggle_bay = egui::Rect::from_min_max(p(864.0, 194.0), p(1090.0, 632.0));
        painter.rect_filled(toggle_bay, 14.0 * s, CHASSIS_FILL);
        painter.rect_stroke(toggle_bay, 14.0 * s, inner_bevel, egui::StrokeKind::Inside);

        // Bottom switch bay recess
        let switch_bay = egui::Rect::from_min_max(p(256.0, 616.0), p(834.0, 744.0));
        painter.rect_filled(switch_bay, 14.0 * s, CHASSIS_FILL);
        painter.rect_stroke(switch_bay, 14.0 * s, inner_bevel, egui::StrokeKind::Inside);

        // Mechanical bridge bar linking left and right throttle levers
        let bridge_rect = egui::Rect::from_min_max(p(290.0, 212.0), p(410.0, 272.0));
        painter.rect_filled(bridge_rect, 8.0 * s, CONTROL_BASE);
        painter.rect_stroke(bridge_rect, 8.0 * s, outline, egui::StrokeKind::Inside);

        // Left throttle lever housing (SC + K1)
        let left_grip = egui::Rect::from_min_max(p(72.0, 68.0), p(318.0, 392.0));
        painter.rect_filled(left_grip, 20.0 * s, CHASSIS_FILL);
        painter.rect_stroke(left_grip, 20.0 * s, outline, egui::StrokeKind::Inside);

        // Right throttle lever housing (I, H, F, G, SLD, E, H3, H4)
        let right_grip = egui::Rect::from_min_max(p(378.0, 52.0), p(820.0, 578.0));
        painter.rect_filled(right_grip, 22.0 * s, CHASSIS_FILL);
        painter.rect_stroke(right_grip, 22.0 * s, outline, egui::StrokeKind::Inside);

        // 1. SC: Scroll wheel
        let sc_center = p(138.0, 208.8);
        Self::draw_scroll_wheel(
            painter,
            sc_center,
            0.38 * s,
            im.dir2("thr_scroll_fwd", "thr_scroll_bck"),
            im.assigned_dir2("thr_scroll_fwd", "thr_scroll_bck"),
        );

        // 2. K1: Double vertical toggle
        let k1_center = p(235.1, 210.8);
        Self::draw_double_toggle_k1(
            painter,
            k1_center,
            0.38 * s,
            im.dir2("thr_k1_up", "thr_k1_down"),
            im.assigned_dir2("thr_k1_up", "thr_k1_down"),
        );

        // 3. Grip circular buttons I and H
        Self::draw_circular_button(
            painter,
            p(484.6, 184.6),
            0.29 * s,
            im.get("thr_btn_i"),
            im.is_assigned("thr_btn_i"),
            "I",
            false,
        );
        Self::draw_circular_button(
            painter,
            p(594.6, 184.6),
            0.29 * s,
            im.get("thr_btn_h"),
            im.is_assigned("thr_btn_h"),
            "H",
            false,
        );

        // 4. Upper rotaries F and G
        Self::draw_circular_button(
            painter,
            p(745.0, 118.0),
            0.40 * s,
            im.get("thr_rty_f_push"),
            im.is_assigned("thr_rty_f_push"),
            "F",
            true,
        );
        Self::draw_circular_button(
            painter,
            p(745.0, 272.0),
            0.40 * s,
            im.get("thr_rty_g_push"),
            im.is_assigned("thr_rty_g_push"),
            "G",
            true,
        );

        // 5. SLD horizontal slider
        let sld_on = im.get("thr_sld") == InteractState::Pressed;
        Self::draw_horizontal_slider_sld(
            painter,
            p(594.0, 290.0),
            0.40 * s,
            im.get("thr_sld"),
            im.is_assigned("thr_sld"),
            sld_on,
        );

        // 6. Square thumb button E
        Self::draw_square_button(
            painter,
            p(581.6, 391.6),
            0.29 * s,
            im.get("thr_btn_e"),
            im.is_assigned("thr_btn_e"),
            "E",
        );

        // 7. Hat 3
        Self::draw_dpad(
            painter,
            p(694.7, 394.7),
            0.31 * s,
            im.dir4("thr_h3_up", "thr_h3_right", "thr_h3_down", "thr_h3_left"),
            im.assigned_dir4("thr_h3_up", "thr_h3_right", "thr_h3_down", "thr_h3_left"),
            "H3",
        );

        // 8. Hat 4
        Self::draw_dpad(
            painter,
            p(647.7, 509.7),
            0.31 * s,
            im.dir4("thr_h4_up", "thr_h4_right", "thr_h4_down", "thr_h4_left"),
            im.assigned_dir4("thr_h4_up", "thr_h4_right", "thr_h4_down", "thr_h4_left"),
            "H4",
        );

        // 9. Right toggle column: TGL 4..1
        Self::draw_circular_double_toggle(
            painter,
            p(1035.0, 262.0),
            0.34 * s,
            im.dir2("thr_sw_13", "thr_sw_14"),
            im.assigned_dir2("thr_sw_13", "thr_sw_14"),
            "TGL 4",
            false,
            "",
            "",
        );
        Self::draw_circular_double_toggle(
            painter,
            p(938.0, 352.0),
            0.34 * s,
            im.dir2("thr_sw_11", "thr_sw_12"),
            im.assigned_dir2("thr_sw_11", "thr_sw_12"),
            "TGL 3",
            false,
            "",
            "",
        );
        Self::draw_circular_double_toggle(
            painter,
            p(1035.0, 472.0),
            0.34 * s,
            im.dir2("thr_sw_9", "thr_sw_10"),
            im.assigned_dir2("thr_sw_9", "thr_sw_10"),
            "TGL 2",
            false,
            "",
            "",
        );
        Self::draw_circular_double_toggle(
            painter,
            p(938.0, 562.0),
            0.34 * s,
            im.dir2("thr_sw_7", "thr_sw_8"),
            im.assigned_dir2("thr_sw_7", "thr_sw_8"),
            "TGL 1",
            false,
            "",
            "",
        );

        // 10. Bottom row switches: SW 1..6
        Self::draw_circular_double_toggle(
            painter,
            p(345.0, 685.0),
            0.36 * s,
            im.dir2("thr_sw_1", "thr_sw_2"),
            im.assigned_dir2("thr_sw_1", "thr_sw_2"),
            "SW",
            true,
            "1",
            "2",
        );
        Self::draw_circular_double_toggle(
            painter,
            p(545.0, 685.0),
            0.36 * s,
            im.dir2("thr_sw_3", "thr_sw_4"),
            im.assigned_dir2("thr_sw_3", "thr_sw_4"),
            "SW",
            true,
            "3",
            "4",
        );
        Self::draw_circular_double_toggle(
            painter,
            p(745.0, 685.0),
            0.36 * s,
            im.dir2("thr_sw_5", "thr_sw_6"),
            im.assigned_dir2("thr_sw_5", "thr_sw_6"),
            "SW",
            true,
            "5",
            "6",
        );
    }

    /// Draws a circular two-way toggle (TGL 1-4 or SW 1-6) with animated lever travel.
    #[allow(clippy::too_many_arguments)]
    fn draw_circular_double_toggle(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        d: Dir2Interact,
        assigned: (bool, bool),
        center_label: &str,
        is_numbered: bool,
        top_num: &str,
        bot_num: &str,
    ) {
        let (ass_up, ass_down) = assigned;
        let any_assigned = ass_up || ass_down;
        let is_up = d.up == InteractState::Pressed;
        let is_down = d.down == InteractState::Pressed;
        let is_neutral = !is_up && !is_down;
        let (fill_col, border_col, text_col) = Self::schematic_control_style(d.any(), any_assigned);

        let r_outer = 144.0 * s;
        let r_dish = 120.0 * s;
        let sw = (1.8 * s).max(1.0);

        painter.circle_filled(center, r_outer, CHASSIS_FILL);
        painter.circle_stroke(center, r_outer, Stroke::new(sw, border_col));
        painter.circle_filled(center, r_dish, CONTROL_BASE);
        painter.circle_stroke(center, r_dish, Stroke::new(sw * 0.8, CONTROL_BEVEL));

        // Central vertical slot well
        let well_rect = egui::Rect::from_center_size(center, egui::vec2(76.0 * s, 172.0 * s));
        painter.rect_filled(well_rect, 38.0 * s, CHASSIS_WELL);
        painter.rect_stroke(
            well_rect,
            38.0 * s,
            Stroke::new(sw * 0.8, CONTROL_BEVEL),
            egui::StrokeKind::Inside,
        );

        let handle_y = if is_up {
            center.y - 52.0 * s
        } else if is_down {
            center.y + 52.0 * s
        } else {
            center.y
        };

        painter.line_segment(
            [center, egui::pos2(center.x, handle_y)],
            Stroke::new(
                (4.5 * s).max(2.0),
                if !is_neutral {
                    COLOR_ACCENT
                } else {
                    CONTROL_BEVEL
                },
            ),
        );

        // Top directional indicator
        let top_y = center.y - 100.0 * s;
        let (_, top_stroke_col, top_txt_col) = Self::schematic_control_style(d.up, ass_up);
        let top_stroke = Stroke::new(sw, top_stroke_col);

        if is_numbered {
            let tri = vec![
                egui::pos2(center.x, top_y - 20.0 * s),
                egui::pos2(center.x - 22.0 * s, top_y + 14.0 * s),
                egui::pos2(center.x + 22.0 * s, top_y + 14.0 * s),
            ];
            let tri_fill = if is_up {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(tri, tri_fill, top_stroke));
            painter.text(
                egui::pos2(center.x, top_y + 2.0 * s),
                egui::Align2::CENTER_CENTER,
                top_num,
                egui::FontId::monospace((18.5 * s).max(8.0)),
                top_txt_col,
            );
        } else {
            let tri = vec![
                egui::pos2(center.x, top_y - 24.0 * s),
                egui::pos2(center.x - 18.0 * s, top_y + 6.0 * s),
                egui::pos2(center.x + 18.0 * s, top_y + 6.0 * s),
            ];
            let tri_fill = if is_up {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(tri, tri_fill, top_stroke));
        }

        // Bottom directional indicator
        let bot_y = center.y + 100.0 * s;
        let (_, bot_stroke_col, bot_txt_col) = Self::schematic_control_style(d.down, ass_down);
        let bot_stroke = Stroke::new(sw, bot_stroke_col);

        if is_numbered {
            let tri = vec![
                egui::pos2(center.x, bot_y + 20.0 * s),
                egui::pos2(center.x - 22.0 * s, bot_y - 14.0 * s),
                egui::pos2(center.x + 22.0 * s, bot_y - 14.0 * s),
            ];
            let tri_fill = if is_down {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(tri, tri_fill, bot_stroke));
            painter.text(
                egui::pos2(center.x, bot_y - 2.0 * s),
                egui::Align2::CENTER_CENTER,
                bot_num,
                egui::FontId::monospace((18.5 * s).max(8.0)),
                bot_txt_col,
            );
        } else {
            let tri = vec![
                egui::pos2(center.x, bot_y + 24.0 * s),
                egui::pos2(center.x - 18.0 * s, bot_y - 6.0 * s),
                egui::pos2(center.x + 18.0 * s, bot_y - 6.0 * s),
            ];
            let tri_fill = if is_down {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(tri, tri_fill, bot_stroke));
        }

        // Animated toggle knob
        let knob_pos = egui::pos2(center.x, handle_y);
        let r_knob = 34.0 * s;
        let knob_fill = if d.any() == InteractState::Pressed {
            fill_col
        } else if d.any() == InteractState::Hovered {
            COLOR_SURFACE_HIGHEST
        } else {
            CONTROL_CAP
        };
        painter.circle_filled(knob_pos, r_knob, knob_fill);
        painter.circle_stroke(knob_pos, r_knob, Stroke::new(sw, border_col));

        let knob_font_sz = if center_label.len() <= 2 {
            (20.0 * s).max(8.0)
        } else {
            (16.5 * s).max(7.5)
        };
        painter.text(
            knob_pos,
            egui::Align2::CENTER_CENTER,
            center_label,
            egui::FontId::monospace(knob_font_sz),
            text_col,
        );
    }

    /// Draws a circular push button (I, H, F, G, A, B, C, D) in anodized graphite.
    fn draw_circular_button(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        st: InteractState,
        is_assigned: bool,
        letter: &str,
        is_large: bool,
    ) {
        let is_pressed = st == InteractState::Pressed;
        let (fill_col, stroke_col, text_col) = Self::schematic_control_style(st, is_assigned);
        let r_bezel = 132.0 * s;
        let r_well = 110.0 * s;
        let r_cap = if is_pressed { 84.0 * s } else { 90.0 * s };
        let sw = (1.8 * s).max(1.0);

        painter.circle_filled(center, r_bezel, CHASSIS_FILL);
        painter.circle_stroke(center, r_bezel, Stroke::new(sw, stroke_col));
        painter.circle_filled(center, r_well, CHASSIS_WELL);

        let cap_fill = match st {
            InteractState::Pressed => fill_col,
            InteractState::Hovered => COLOR_SURFACE_HIGHEST,
            InteractState::Idle => CONTROL_CAP,
        };
        painter.circle_filled(center, r_cap, cap_fill);
        painter.circle_stroke(center, r_cap, Stroke::new(sw, stroke_col));

        if is_assigned && st == InteractState::Idle {
            painter.circle_filled(
                egui::pos2(center.x, center.y - r_cap * 0.62),
                (6.0 * s).max(2.0),
                COLOR_ACCENT,
            );
        }

        let font_sz = if is_large {
            (52.0 * s).max(9.0)
        } else {
            (48.0 * s).max(8.5)
        };
        painter.text(
            egui::pos2(center.x, center.y + 1.5 * s),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::monospace(font_sz),
            text_col,
        );
    }

    /// Draws the square thumb button E in anodized graphite.
    fn draw_square_button(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        st: InteractState,
        is_assigned: bool,
        letter: &str,
    ) {
        let is_pressed = st == InteractState::Pressed;
        let (fill_col, stroke_col, text_col) = Self::schematic_control_style(st, is_assigned);
        let half_chassis = 126.0 * s;
        let half_well = 102.0 * s;
        let half_cap = if is_pressed { 76.0 * s } else { 82.0 * s };
        let sw = (1.8 * s).max(1.0);

        let chassis_rect = egui::Rect::from_center_size(
            center,
            egui::vec2(half_chassis * 2.0, half_chassis * 2.0),
        );
        painter.rect_filled(chassis_rect, 14.0 * s, CHASSIS_FILL);
        painter.rect_stroke(
            chassis_rect,
            14.0 * s,
            Stroke::new(sw, stroke_col),
            egui::StrokeKind::Inside,
        );

        let well_rect =
            egui::Rect::from_center_size(center, egui::vec2(half_well * 2.0, half_well * 2.0));
        painter.rect_filled(well_rect, 10.0 * s, CHASSIS_WELL);

        let cap_rect =
            egui::Rect::from_center_size(center, egui::vec2(half_cap * 2.0, half_cap * 2.0));
        let cap_fill = match st {
            InteractState::Pressed => fill_col,
            InteractState::Hovered => COLOR_SURFACE_HIGHEST,
            InteractState::Idle => CONTROL_CAP,
        };
        painter.rect_filled(cap_rect, 8.0 * s, cap_fill);
        painter.rect_stroke(
            cap_rect,
            8.0 * s,
            Stroke::new(sw, stroke_col),
            egui::StrokeKind::Inside,
        );

        if is_assigned && st == InteractState::Idle {
            painter.circle_filled(
                egui::pos2(center.x, center.y - half_cap * 0.62),
                (6.0 * s).max(2.0),
                COLOR_ACCENT,
            );
        }

        painter.text(
            egui::pos2(center.x, center.y + 1.5 * s),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::monospace((48.0 * s).max(8.5)),
            text_col,
        );
    }

    /// Draws the scroll wheel SC without idle neon decorations.
    fn draw_scroll_wheel(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        d: Dir2Interact,
        assigned: (bool, bool),
    ) {
        let (ass_up, ass_down) = assigned;
        let any_assigned = ass_up || ass_down;
        let (fill_col, border_col, text_col) = Self::schematic_control_style(d.any(), any_assigned);
        let sw = (1.8 * s).max(1.0);

        let w = 164.0 * s;
        let h = 300.0 * s;
        let cap_rect = egui::Rect::from_center_size(center, egui::vec2(w, h));
        painter.rect_filled(cap_rect, w * 0.5, CHASSIS_FILL);
        painter.rect_stroke(
            cap_rect,
            w * 0.5,
            Stroke::new(sw, border_col),
            egui::StrokeKind::Inside,
        );

        // Top / bottom scroll arrows
        let (_, up_col, _) = Self::schematic_control_style(d.up, ass_up);
        let top_arrow = vec![
            egui::pos2(center.x, center.y - 136.0 * s),
            egui::pos2(center.x - 15.0 * s, center.y - 114.0 * s),
            egui::pos2(center.x + 15.0 * s, center.y - 114.0 * s),
        ];
        painter.add(egui::Shape::convex_polygon(
            top_arrow,
            if d.up == InteractState::Pressed {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(sw, up_col),
        ));

        let (_, dn_col, _) = Self::schematic_control_style(d.down, ass_down);
        let bot_arrow = vec![
            egui::pos2(center.x, center.y + 136.0 * s),
            egui::pos2(center.x - 15.0 * s, center.y + 114.0 * s),
            egui::pos2(center.x + 15.0 * s, center.y + 114.0 * s),
        ];
        painter.add(egui::Shape::convex_polygon(
            bot_arrow,
            if d.down == InteractState::Pressed {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(sw, dn_col),
        ));

        // Drum cavity & rotary drum
        let cav_rect = egui::Rect::from_center_size(center, egui::vec2(92.0 * s, 212.0 * s));
        painter.rect_filled(cav_rect, 22.0 * s, CHASSIS_WELL);
        painter.rect_stroke(
            cav_rect,
            22.0 * s,
            Stroke::new(sw * 0.8, CONTROL_BEVEL),
            egui::StrokeKind::Inside,
        );

        let drum_rect = egui::Rect::from_center_size(center, egui::vec2(72.0 * s, 196.0 * s));
        painter.rect_filled(drum_rect, 16.0 * s, CONTROL_CAP);
        painter.rect_stroke(
            drum_rect,
            16.0 * s,
            Stroke::new(sw * 0.9, border_col),
            egui::StrokeKind::Inside,
        );

        for i in -5..=5 {
            let ry = center.y + (i as f32) * 16.0 * s;
            let line_col = if i == 0 && d.any() == InteractState::Pressed {
                COLOR_ACCENT
            } else {
                CONTROL_HAIRLINE
            };
            painter.line_segment(
                [
                    egui::pos2(center.x - 26.0 * s, ry),
                    egui::pos2(center.x + 26.0 * s, ry),
                ],
                Stroke::new((1.2 * s).max(0.8), line_col),
            );
        }

        // Center SC badge
        let badge_rect = egui::Rect::from_center_size(center, egui::vec2(48.0 * s, 32.0 * s));
        let badge_fill = if d.any() == InteractState::Pressed {
            fill_col
        } else {
            CHASSIS_WELL
        };
        painter.rect_filled(badge_rect, 6.0 * s, badge_fill);
        painter.rect_stroke(
            badge_rect,
            6.0 * s,
            Stroke::new(sw * 0.8, border_col),
            egui::StrokeKind::Inside,
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "SC",
            egui::FontId::monospace((18.0 * s).max(8.0)),
            text_col,
        );
    }

    /// Draws the vertical rocker toggle K1 with live handle travel.
    fn draw_double_toggle_k1(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        d: Dir2Interact,
        assigned: (bool, bool),
    ) {
        let (ass_up, ass_down) = assigned;
        let any_assigned = ass_up || ass_down;
        let is_up = d.up == InteractState::Pressed;
        let is_down = d.down == InteractState::Pressed;
        let is_neutral = !is_up && !is_down;
        let (fill_col, border_col, text_col) = Self::schematic_control_style(d.any(), any_assigned);
        let sw = (1.8 * s).max(1.0);

        let w = 166.0 * s;
        let h = 300.0 * s;
        let k1_rect = egui::Rect::from_center_size(center, egui::vec2(w, h));
        painter.rect_filled(k1_rect, 14.0 * s, CHASSIS_FILL);
        painter.rect_stroke(
            k1_rect,
            14.0 * s,
            Stroke::new(sw, border_col),
            egui::StrokeKind::Inside,
        );

        let (_, top_col, _) = Self::schematic_control_style(d.up, ass_up);
        let top_arrow = vec![
            egui::pos2(center.x, center.y - 140.0 * s),
            egui::pos2(center.x - 13.0 * s, center.y - 122.0 * s),
            egui::pos2(center.x + 13.0 * s, center.y - 122.0 * s),
        ];
        painter.add(egui::Shape::convex_polygon(
            top_arrow,
            if is_up {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(sw, top_col),
        ));

        let slot_rect = egui::Rect::from_center_size(center, egui::vec2(70.0 * s, 184.0 * s));
        painter.rect_filled(slot_rect, 35.0 * s, CHASSIS_WELL);
        painter.rect_stroke(
            slot_rect,
            35.0 * s,
            Stroke::new(sw * 0.8, CONTROL_BEVEL),
            egui::StrokeKind::Inside,
        );

        let handle_y = if is_up {
            center.y - 58.0 * s
        } else if is_down {
            center.y + 58.0 * s
        } else {
            center.y
        };

        painter.line_segment(
            [center, egui::pos2(center.x, handle_y)],
            Stroke::new(
                (4.0 * s).max(2.0),
                if !is_neutral {
                    COLOR_ACCENT
                } else {
                    CONTROL_BEVEL
                },
            ),
        );

        let h_pos = egui::pos2(center.x, handle_y);
        let handle_pts = vec![
            egui::pos2(h_pos.x - 26.0 * s, h_pos.y - 28.0 * s),
            egui::pos2(h_pos.x + 26.0 * s, h_pos.y - 28.0 * s),
            egui::pos2(h_pos.x + 30.0 * s, h_pos.y),
            egui::pos2(h_pos.x + 26.0 * s, h_pos.y + 28.0 * s),
            egui::pos2(h_pos.x - 26.0 * s, h_pos.y + 28.0 * s),
            egui::pos2(h_pos.x - 30.0 * s, h_pos.y),
        ];
        let handle_fill = match d.any() {
            InteractState::Pressed => fill_col,
            InteractState::Hovered => COLOR_SURFACE_HIGHEST,
            InteractState::Idle => CONTROL_CAP,
        };
        painter.add(egui::Shape::convex_polygon(
            handle_pts,
            handle_fill,
            Stroke::new(sw, border_col),
        ));
        painter.text(
            h_pos,
            egui::Align2::CENTER_CENTER,
            "K1",
            egui::FontId::monospace((19.0 * s).max(8.0)),
            text_col,
        );

        let (_, bot_col, _) = Self::schematic_control_style(d.down, ass_down);
        let bot_arrow = vec![
            egui::pos2(center.x, center.y + 140.0 * s),
            egui::pos2(center.x - 13.0 * s, center.y + 122.0 * s),
            egui::pos2(center.x + 13.0 * s, center.y + 122.0 * s),
        ];
        painter.add(egui::Shape::convex_polygon(
            bot_arrow,
            if is_down {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(sw, bot_col),
        ));
    }

    /// Draws the horizontal two-position slider SLD with live mechanical travel.
    fn draw_horizontal_slider_sld(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        st: InteractState,
        is_assigned: bool,
        is_on: bool,
    ) {
        let (fill_col, border_col, text_col) = Self::schematic_control_style(st, is_assigned);
        let sw = (1.8 * s).max(1.0);
        let sld_rect = egui::Rect::from_center_size(center, egui::vec2(256.0 * s, 162.0 * s));
        painter.rect_filled(sld_rect, 34.0 * s, CHASSIS_FILL);
        painter.rect_stroke(
            sld_rect,
            34.0 * s,
            Stroke::new(sw, border_col),
            egui::StrokeKind::Inside,
        );

        let track_rect = egui::Rect::from_center_size(center, egui::vec2(168.0 * s, 74.0 * s));
        painter.rect_filled(track_rect, 37.0 * s, CHASSIS_WELL);
        painter.rect_stroke(
            track_rect,
            37.0 * s,
            Stroke::new(sw * 0.8, CONTROL_BEVEL),
            egui::StrokeKind::Inside,
        );

        let handle_x = if is_on {
            center.x + 46.0 * s
        } else {
            center.x - 46.0 * s
        };

        painter.line_segment(
            [
                egui::pos2(center.x - 46.0 * s, center.y),
                egui::pos2(handle_x, center.y),
            ],
            Stroke::new(
                (4.5 * s).max(2.0),
                if is_on { COLOR_ACCENT } else { CONTROL_BEVEL },
            ),
        );

        let h_pos = egui::pos2(handle_x, center.y);
        let handle_pts = vec![
            egui::pos2(h_pos.x - 32.0 * s, h_pos.y - 28.0 * s),
            egui::pos2(h_pos.x + 32.0 * s, h_pos.y - 28.0 * s),
            egui::pos2(h_pos.x + 36.0 * s, h_pos.y),
            egui::pos2(h_pos.x + 32.0 * s, h_pos.y + 28.0 * s),
            egui::pos2(h_pos.x - 32.0 * s, h_pos.y + 28.0 * s),
            egui::pos2(h_pos.x - 36.0 * s, h_pos.y),
        ];
        let handle_fill = match st {
            InteractState::Pressed => fill_col,
            InteractState::Hovered => COLOR_SURFACE_HIGHEST,
            InteractState::Idle => CONTROL_CAP,
        };
        painter.add(egui::Shape::convex_polygon(
            handle_pts,
            handle_fill,
            Stroke::new(sw, border_col),
        ));
        painter.text(
            h_pos,
            egui::Align2::CENTER_CENTER,
            "SLD",
            egui::FontId::monospace((19.0 * s).max(8.0)),
            text_col,
        );
    }

    /// Draws a 4-way D-pad / castle hat (H3, H4, POV, H1, H2) in anodized graphite.
    fn draw_dpad(
        painter: &egui::Painter,
        center: egui::Pos2,
        s: f32,
        d: Dir4Interact,
        assigned: (bool, bool, bool, bool),
        label: &str,
    ) {
        let (ass_up, ass_right, ass_down, ass_left) = assigned;
        let any_assigned = ass_up || ass_right || ass_down || ass_left;
        let (fill_col, rim_col, text_col) = Self::schematic_control_style(d.any(), any_assigned);
        let sw = (1.8 * s).max(1.0);

        let r_rim = 162.0 * s;
        let r_dish = 138.0 * s;
        painter.circle_filled(center, r_rim, CHASSIS_FILL);
        painter.circle_stroke(center, r_rim, Stroke::new(sw, rim_col));
        painter.circle_filled(center, r_dish, CHASSIS_WELL);
        painter.circle_stroke(center, r_dish, Stroke::new(sw * 0.8, CONTROL_BEVEL));

        let arm_w = 76.0 * s;
        let arm_len = 268.0 * s;
        let v_rect = egui::Rect::from_center_size(center, egui::vec2(arm_w, arm_len));
        painter.rect_filled(v_rect, 14.0 * s, CONTROL_CAP);
        painter.rect_stroke(
            v_rect,
            14.0 * s,
            Stroke::new(sw * 0.8, CONTROL_HAIRLINE),
            egui::StrokeKind::Inside,
        );

        let h_rect = egui::Rect::from_center_size(center, egui::vec2(arm_len, arm_w));
        painter.rect_filled(h_rect, 14.0 * s, CONTROL_CAP);
        painter.rect_stroke(
            h_rect,
            14.0 * s,
            Stroke::new(sw * 0.8, CONTROL_HAIRLINE),
            egui::StrokeKind::Inside,
        );

        let d_offset = 116.0 * s;
        let arrow_size = 18.0 * s;

        let draw_dir_arrow = |pts: Vec<egui::Pos2>, st: InteractState, is_ass: bool| {
            let (_, st_col, _) = Self::schematic_control_style(st, is_ass);
            let fill = if st == InteractState::Pressed {
                COLOR_ACCENT
            } else {
                Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(
                pts,
                fill,
                Stroke::new(sw, st_col),
            ));
        };

        draw_dir_arrow(
            vec![
                egui::pos2(center.x, center.y - d_offset),
                egui::pos2(
                    center.x - arrow_size * 0.7,
                    center.y - d_offset + arrow_size,
                ),
                egui::pos2(
                    center.x + arrow_size * 0.7,
                    center.y - d_offset + arrow_size,
                ),
            ],
            d.up,
            ass_up,
        );
        draw_dir_arrow(
            vec![
                egui::pos2(center.x, center.y + d_offset),
                egui::pos2(
                    center.x - arrow_size * 0.7,
                    center.y + d_offset - arrow_size,
                ),
                egui::pos2(
                    center.x + arrow_size * 0.7,
                    center.y + d_offset - arrow_size,
                ),
            ],
            d.down,
            ass_down,
        );
        draw_dir_arrow(
            vec![
                egui::pos2(center.x - d_offset, center.y),
                egui::pos2(
                    center.x - d_offset + arrow_size,
                    center.y - arrow_size * 0.7,
                ),
                egui::pos2(
                    center.x - d_offset + arrow_size,
                    center.y + arrow_size * 0.7,
                ),
            ],
            d.left,
            ass_left,
        );
        draw_dir_arrow(
            vec![
                egui::pos2(center.x + d_offset, center.y),
                egui::pos2(
                    center.x + d_offset - arrow_size,
                    center.y - arrow_size * 0.7,
                ),
                egui::pos2(
                    center.x + d_offset - arrow_size,
                    center.y + arrow_size * 0.7,
                ),
            ],
            d.right,
            ass_right,
        );

        let hub_fill = match d.any() {
            InteractState::Pressed => fill_col,
            InteractState::Hovered => COLOR_SURFACE_HIGHEST,
            InteractState::Idle => CONTROL_BASE,
        };
        painter.circle_filled(center, 40.0 * s, hub_fill);
        painter.circle_stroke(center, 40.0 * s, Stroke::new(sw, rim_col));
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace((24.0 * s).max(8.0)),
            text_col,
        );
    }

    // ========================================================================
    // 4. QUICK MACRO ACTIONS & RECORDER DRAWER
    // ========================================================================

    fn render_quick_macro_actions(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section_heading(ui, &x56_core::t!("prog-macro-title"));
            ui.add_space(SPACE_SM);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_SM, SPACE_XS);

                if theme::secondary_button(ui, &x56_core::t!("prog-macro-new")).clicked() {
                    self.macro_recorder_open = true;
                    self.is_recording_macro = false;
                    self.recorded_steps.clear();
                    self.last_record_instant = None;
                    if self.macro_target_button_id.is_none() {
                        self.macro_target_button_id = self
                            .editing_button_id
                            .clone()
                            .or_else(|| self.hovered_button_id.clone());
                    }
                    if self.macro_name_draft.trim().is_empty() {
                        self.macro_name_draft = x56_core::t!("prog-macro-default-name");
                    }
                    self.status_message = Some((x56_core::t!("prog-macro-ready"), false));
                }

                let rec_resp = if self.is_recording_macro {
                    theme::danger_button(ui, &x56_core::t!("prog-macro-stop"))
                } else {
                    theme::secondary_button(ui, &x56_core::t!("prog-macro-record"))
                };

                if rec_resp.clicked() {
                    if !self.is_recording_macro {
                        self.is_recording_macro = true;
                        self.macro_recorder_open = true;
                        self.recorded_steps.clear();
                        self.last_record_instant = None;
                        if self.macro_target_button_id.is_none() {
                            self.macro_target_button_id = self
                                .editing_button_id
                                .clone()
                                .or_else(|| self.hovered_button_id.clone());
                        }
                        ui.ctx().memory_mut(|m| m.request_focus(egui::Id::NULL));
                        self.status_message = Some((x56_core::t!("prog-macro-rec-live"), false));
                    } else {
                        self.is_recording_macro = false;
                        self.last_record_instant = None;
                        self.status_message = Some((
                            format!(
                                "{} ({})",
                                x56_core::t!("prog-macro-captured-msg"),
                                self.recorded_steps.len()
                            ),
                            false,
                        ));
                    }
                }
            });
        });
    }

    fn render_macro_recorder_panel(&mut self, ui: &mut Ui, monitor: &ButtonMonitor) {
        if !(self.macro_recorder_open || self.is_recording_macro || !self.recorded_steps.is_empty())
        {
            return;
        }

        theme::subcard_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal_wrapped(|ui| {
                let (dot_col, status_txt) = if self.is_recording_macro {
                    (COLOR_NEGATIVE, x56_core::t!("prog-macro-rec-live"))
                } else {
                    (
                        COLOR_TEXT_MUTED,
                        format!(
                            "{} ({})",
                            x56_core::t!("prog-macro-ready"),
                            self.recorded_steps.len()
                        ),
                    )
                };
                theme::status_dot(ui, dot_col, 8.0);
                ui.label(
                    RichText::new(status_txt)
                        .color(COLOR_TEXT_PRIMARY)
                        .size(FONT_SMALL),
                );
            });

            ui.add_space(SPACE_SM);

            let all_desc = x56_core::all_button_descriptors();
            let selected_label = self
                .macro_target_button_id
                .as_deref()
                .and_then(|id| all_desc.iter().find(|d| d.id == id))
                .map(|d| {
                    let prefix = if d.device == DeviceKind::Stick {
                        x56_core::t!("curves-stick-short")
                    } else {
                        x56_core::t!("curves-throttle-short")
                    };
                    let loc = localized_node_name_by_id(&d.id, d.label);
                    format!("{prefix} · {loc}")
                })
                .unwrap_or_else(|| x56_core::t!("prog-macro-no-target"));

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(x56_core::t!("prog-macro-target-btn"))
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_SMALL),
                );
                egui::ComboBox::from_id_salt("macro_target_btn_combo")
                    .selected_text(selected_label)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for desc in &all_desc {
                            let prefix = if desc.device == DeviceKind::Stick {
                                x56_core::t!("curves-stick-short")
                            } else {
                                x56_core::t!("curves-throttle-short")
                            };
                            let loc = localized_node_name_by_id(&desc.id, desc.label);
                            let item_label = format!("{prefix} · {loc}");
                            ui.selectable_value(
                                &mut self.macro_target_button_id,
                                Some(desc.id.clone()),
                                item_label,
                            );
                        }
                    });
            });

            ui.add_space(SPACE_XS);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(x56_core::t!("prog-macro-name-label"))
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_SMALL),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.macro_name_draft)
                        .hint_text(x56_core::t!("prog-macro-name-hint"))
                        .desired_width(170.0),
                );
                ui.checkbox(
                    &mut self.macro_record_timings,
                    RichText::new(x56_core::t!("prog-macro-keep-delays"))
                        .color(COLOR_TEXT_SECONDARY)
                        .size(FONT_SMALL),
                );
            });

            ui.add_space(SPACE_SM);

            egui::ScrollArea::vertical()
                .id_salt("macro_steps_scroll")
                .max_height(130.0)
                .stick_to_bottom(self.is_recording_macro)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if self.recorded_steps.is_empty() {
                        ui.label(
                            RichText::new(x56_core::t!("prog-macro-no-steps"))
                                .color(COLOR_TEXT_MUTED)
                                .size(FONT_SMALL),
                        );
                    } else {
                        let mut remove_idx: Option<usize> = None;
                        for (idx, step) in self.recorded_steps.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("#{}", idx + 1))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(FONT_META)
                                        .monospace(),
                                );

                                if idx > 0 {
                                    ui.add(
                                        egui::DragValue::new(&mut step.delay_ms)
                                            .range(0..=5000)
                                            .speed(10.0)
                                            .suffix(" ms"),
                                    );
                                }

                                theme::keycap(ui, &step.keys_display());

                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(icons::X)
                                                .color(COLOR_NEGATIVE)
                                                .size(FONT_META),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    remove_idx = Some(idx);
                                }
                            });
                        }
                        if let Some(idx) = remove_idx {
                            self.recorded_steps.remove(idx);
                            if idx == 0 {
                                if let Some(first) = self.recorded_steps.first_mut() {
                                    first.delay_ms = 0;
                                }
                            }
                        }
                    }
                });

            ui.add_space(SPACE_SM);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_SM, SPACE_XS);
                let can_save =
                    self.macro_target_button_id.is_some() && !self.recorded_steps.is_empty();
                let save_clicked = ui
                    .add_enabled_ui(can_save, |ui| {
                        theme::primary_button(ui, &x56_core::t!("prog-macro-save")).clicked()
                    })
                    .inner;
                if save_clicked {
                    if let Some(button_id) = self.macro_target_button_id.clone() {
                        let btn_label = all_desc
                            .iter()
                            .find(|d| d.id == button_id)
                            .map(|d| localized_node_name_by_id(&d.id, d.label))
                            .unwrap_or_else(|| button_id.clone());
                        let name = if self.macro_name_draft.trim().is_empty() {
                            btn_label.clone()
                        } else {
                            self.macro_name_draft.trim().to_string()
                        };
                        let cell = CommandCell {
                            button_id,
                            name,
                            keys: Vec::new(),
                            steps: self.recorded_steps.clone(),
                        };
                        self.profile.set_cell(self.selected_mode, cell);
                        monitor.set_profile(self.profile.clone());

                        self.is_recording_macro = false;
                        self.macro_recorder_open = false;
                        self.recorded_steps.clear();
                        self.last_record_instant = None;
                        self.macro_name_draft.clear();
                        self.status_message = Some((
                            format!("{} {btn_label}", x56_core::t!("prog-macro-saved-msg")),
                            false,
                        ));
                    }
                }

                if theme::secondary_button(ui, &x56_core::t!("prog-macro-clear")).clicked() {
                    self.recorded_steps.clear();
                    self.last_record_instant = None;
                }

                if theme::ghost_button(ui, &x56_core::t!("prog-macro-close")).clicked() {
                    self.is_recording_macro = false;
                    self.macro_recorder_open = false;
                    self.recorded_steps.clear();
                    self.last_record_instant = None;
                }
            });
        });
        ui.add_space(SPACE_SM);
    }

    // ========================================================================
    // 6. RIGHT COLUMN: SEARCH, FILTER CHIPS & SIGNATURE MAPPING CARDS
    // ========================================================================

    fn render_search_and_stats(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_SM, SPACE_XS);

            let search_w = (ui.available_width() - 180.0).clamp(140.0, 260.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(x56_core::t!("prog-search-placeholder"))
                    .desired_width(search_w),
            );
            if !self.search_query.is_empty()
                && theme::ghost_button(ui, &x56_core::t!("prog-clear")).clicked()
            {
                self.search_query.clear();
            }

            let catalog = catalog_for_device(self.selected_device);
            let total_buttons = catalog.len();
            let assigned_count = catalog
                .iter()
                .filter(|b| {
                    self.profile
                        .get_cell(self.selected_mode, &b.id)
                        .is_some_and(|c| c.is_assigned())
                })
                .count();
            let assigned_str = assigned_count.to_string();
            let total_str = total_buttons.to_string();

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(x56_core::t!(
                        "prog-mapped-count",
                        "assigned" => &assigned_str,
                        "total" => &total_str
                    ))
                    .color(COLOR_TEXT_SECONDARY)
                    .size(FONT_META)
                    .monospace(),
                );
            });
        });
    }

    fn render_filter_pills(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, SPACE_XS);
            let categories = [
                FilterCategory::Todos,
                FilterCategory::Gatillos,
                FilterCategory::Setas,
                FilterCategory::BotonesFrontales,
                FilterCategory::Macros,
                FilterCategory::Shift,
            ];

            for cat in categories {
                let is_sel = self.filter_category == cat;
                if theme::segmented_button(ui, &cat.label(), is_sel).clicked() {
                    self.filter_category = cat;
                }
            }
        });
    }

    fn render_mapping_cards(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let catalog = catalog_for_device(self.selected_device);
        let q = self.search_query.trim().to_lowercase();

        let mut displayed_count = 0;

        for btn in &catalog {
            if !q.is_empty() {
                let loc_name = localized_node_name_by_id(&btn.id, btn.label).to_lowercase();
                let matches_id = btn.id.to_lowercase().contains(&q);
                let matches_label = btn.label.to_lowercase().contains(&q) || loc_name.contains(&q);
                let matches_code = format!("{:#06x}", btn.evdev_code).contains(&q);
                let matches_assigned = self
                    .profile
                    .get_cell(self.selected_mode, &btn.id)
                    .is_some_and(|c| {
                        c.name.to_lowercase().contains(&q)
                            || c.keys_display().to_lowercase().contains(&q)
                    });

                if !matches_id && !matches_label && !matches_code && !matches_assigned {
                    continue;
                }
            }

            if !self.matches_category(&btn.id) {
                continue;
            }

            displayed_count += 1;
            self.render_mapping_card(ui, monitor, snapshot, btn);
            ui.add_space(SPACE_XS);
        }

        if displayed_count == 0 {
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(SPACE_LG);
                    ui.label(
                        RichText::new(x56_core::t!("prog-empty-filter"))
                            .color(COLOR_TEXT_MUTED)
                            .size(FONT_SMALL),
                    );
                    ui.add_space(SPACE_LG);
                });
            });
        }
    }

    fn matches_category(&self, id: &str) -> bool {
        match self.filter_category {
            FilterCategory::Todos => true,
            FilterCategory::Gatillos => id.contains("trigger") || id.contains("pinkie"),
            FilterCategory::Setas => {
                id.contains("pov")
                    || id.contains("h1")
                    || id.contains("h2")
                    || id.contains("h3")
                    || id.contains("h4")
                    || id.contains("ministick")
                    || id == "stick_btn_c"
            }
            FilterCategory::BotonesFrontales => {
                id.contains("btn")
                    || id.contains("sw")
                    || id.contains("rty")
                    || id.contains("k1")
                    || id.contains("sld")
                    || id.contains("scroll")
            }
            FilterCategory::Macros => {
                if let Some(cell) = self.profile.get_cell(self.selected_mode, id) {
                    cell.is_sequence() || cell.keys.len() > 1
                } else {
                    false
                }
            }
            FilterCategory::Shift => id.contains("pinkie") || id.contains("sld"),
        }
    }

    /// Signature 2-row mapping card.
    fn render_mapping_card(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
        btn: &ButtonDescriptor,
    ) {
        let is_physically_pressed = snapshot.pressed_buttons.contains(&btn.id);
        let is_editing = self.editing_button_id.as_deref() == Some(&btn.id);
        let is_hovered = self.hovered_button_id.as_deref() == Some(&btn.id);

        let cell_opt = self.profile.get_cell(self.selected_mode, &btn.id).cloned();

        let border_stroke = if is_editing {
            Stroke::new(1.0, COLOR_ACCENT)
        } else if is_physically_pressed {
            Stroke::new(1.0, COLOR_POSITIVE)
        } else if is_hovered {
            Stroke::new(1.0, COLOR_BORDER_STRONG)
        } else {
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE)
        };

        let frame_resp = theme::subcard_frame().stroke(border_stroke).show(ui, |ui| {
            ui.set_width(ui.available_width());

            if is_editing {
                self.render_inline_edit_controls(ui, monitor, btn);
            } else {
                let btn_label = localized_node_name_by_id(&btn.id, btn.label);
                let ev_str = if btn.evdev_code != 0 {
                    format!("{:#06x}", btn.evdev_code)
                } else {
                    "ABS_HAT0".to_string()
                };

                // Row 1: Button label + optional pressed dot + right-aligned quiet mono code
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&btn_label)
                            .color(COLOR_TEXT_PRIMARY)
                            .size(FONT_BODY),
                    );
                    if is_physically_pressed {
                        theme::status_dot(ui, COLOR_POSITIVE, 8.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(ev_str)
                                .color(COLOR_TEXT_MUTED)
                                .size(FONT_META)
                                .monospace(),
                        );
                    });
                });

                ui.add_space(SPACE_XS);

                // Row 2: Left = recessed keycaps + action name (or muted Unassigned);
                // Right = uniform compact Test / Edit / Clear actions.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, SPACE_XS);

                    if let Some(c) = cell_opt.as_ref().filter(|c| c.is_assigned()) {
                        if c.is_sequence() {
                            for step in c.steps.iter().take(3) {
                                theme::keycap(ui, &step.keys_display());
                            }
                            if c.steps.len() > 3 {
                                let count_str = c.steps.len().to_string();
                                ui.label(
                                    RichText::new(x56_core::t!(
                                        "prog-card-sequence-count",
                                        "count" => &count_str
                                    ))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(FONT_META)
                                    .monospace(),
                                );
                            }
                        } else {
                            for k in &c.keys {
                                theme::keycap(ui, k);
                            }
                        }

                        if !c.name.trim().is_empty() {
                            ui.add_space(SPACE_XS);
                            ui.label(
                                RichText::new(&c.name)
                                    .color(COLOR_TEXT_SECONDARY)
                                    .size(FONT_SMALL),
                            );
                        }
                    } else {
                        ui.label(
                            RichText::new(unassigned_label())
                                .color(COLOR_TEXT_MUTED)
                                .size(FONT_SMALL),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, SPACE_XS);

                        if cell_opt.as_ref().is_some_and(|c| c.is_assigned())
                            && theme::ghost_button(ui, &x56_core::t!("prog-card-clear"))
                                .on_hover_text(x56_core::t!("prog-card-clear-tooltip"))
                                .clicked()
                        {
                            self.profile.remove_cell(self.selected_mode, &btn.id);
                            monitor.set_profile(self.profile.clone());
                            self.status_message = Some((
                                x56_core::t!("prog-card-cleared-msg", "label" => &btn_label),
                                false,
                            ));
                        }

                        if theme::secondary_button(ui, &x56_core::t!("prog-card-edit"))
                            .on_hover_text(x56_core::t!("prog-card-edit-tooltip"))
                            .clicked()
                        {
                            if let Some(ref c) = cell_opt {
                                if c.is_sequence() {
                                    self.macro_recorder_open = true;
                                    self.is_recording_macro = false;
                                    self.macro_target_button_id = Some(btn.id.clone());
                                    self.macro_name_draft = c.name.clone();
                                    self.recorded_steps = c.steps.clone();
                                    self.last_record_instant = None;
                                    self.editing_button_id = None;
                                } else if c.is_assigned() {
                                    self.editing_button_id = Some(btn.id.clone());
                                    self.pending_name = c.name.clone();
                                    self.pending_keys = c.keys.clone();
                                } else {
                                    self.editing_button_id = Some(btn.id.clone());
                                    self.pending_name.clear();
                                    self.pending_keys.clear();
                                }
                            } else {
                                self.editing_button_id = Some(btn.id.clone());
                                self.pending_name.clear();
                                self.pending_keys.clear();
                            }
                        }

                        if theme::ghost_button(ui, &x56_core::t!("prog-card-test"))
                            .on_hover_text(x56_core::t!("prog-card-test-tooltip"))
                            .clicked()
                        {
                            if let Some(c) = cell_opt.as_ref().filter(|c| c.is_assigned()) {
                                match monitor.test_cell(c) {
                                    TestTriggerStatus::Emitted => {
                                        let keys = c.keys_display();
                                        self.status_message = Some((
                                            x56_core::t!(
                                                "prog-card-test-sent",
                                                "keys" => &keys,
                                                "name" => &btn_label
                                            ),
                                            false,
                                        ));
                                    }
                                    TestTriggerStatus::Paused => {
                                        self.status_message =
                                            Some((x56_core::t!("prog-card-test-paused"), true));
                                    }
                                    TestTriggerStatus::UinputUnavailable => {
                                        self.status_message =
                                            Some((x56_core::t!("prog-card-test-no-uinput"), true));
                                    }
                                    TestTriggerStatus::Unassigned => {
                                        self.status_message = Some((
                                            x56_core::t!(
                                                "prog-card-unassigned-err",
                                                "label" => &btn_label
                                            ),
                                            true,
                                        ));
                                    }
                                }
                            } else {
                                self.status_message = Some((
                                    x56_core::t!(
                                        "prog-card-unassigned-err",
                                        "label" => &btn_label
                                    ),
                                    true,
                                ));
                            }
                        }
                    });
                });
            }
        });

        if frame_resp.response.hovered() {
            self.hovered_button_id = Some(btn.id.clone());
        }
    }

    /// Inline binding editor with live key capture and recessed keycaps.
    fn render_inline_edit_controls(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
    ) {
        let card_w = ui.available_width();
        let btn_label = localized_node_name_by_id(&btn.id, btn.label);

        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(x56_core::t!(
                        "prog-editor-title",
                        "code" => btn.code_label,
                        "label" => &btn_label
                    ))
                    .color(COLOR_TEXT_PRIMARY)
                    .size(FONT_BODY),
                );
                ui.add_space(SPACE_SM);
                theme::status_dot(ui, COLOR_ACCENT, 6.0);
                ui.label(
                    RichText::new(x56_core::t!("prog-editor-capturing"))
                        .color(COLOR_ACCENT)
                        .size(FONT_META),
                );
            });

            ui.add_space(SPACE_XS);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, SPACE_XS);
                ui.label(
                    RichText::new(x56_core::t!("prog-editor-keys"))
                        .color(COLOR_TEXT_MUTED)
                        .size(FONT_META),
                );
                if self.pending_keys.is_empty() {
                    ui.label(
                        RichText::new(x56_core::t!("prog-editor-capture-hint"))
                            .color(COLOR_TEXT_SECONDARY)
                            .size(FONT_SMALL),
                    );
                } else {
                    for key in &self.pending_keys {
                        theme::keycap(ui, key);
                    }
                    if theme::ghost_button(ui, &x56_core::t!("prog-clear")).clicked() {
                        self.pending_keys.clear();
                    }
                }
            });

            ui.add_space(SPACE_XS);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_SM, SPACE_XS);
                ui.label(
                    RichText::new(x56_core::t!("prog-editor-action"))
                        .color(COLOR_TEXT_MUTED)
                        .size(FONT_META),
                );
                let edit_w = (card_w - 80.0).clamp(120.0, 220.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.pending_name)
                        .desired_width(edit_w)
                        .hint_text(x56_core::t!("prog-editor-action-hint")),
                );

                if theme::primary_button(ui, &x56_core::t!("prog-editor-confirm")).clicked() {
                    let name_final = if self.pending_name.trim().is_empty() {
                        btn_label.clone()
                    } else {
                        self.pending_name.trim().to_string()
                    };

                    let cell = CommandCell {
                        button_id: btn.id.clone(),
                        name: name_final,
                        keys: self.pending_keys.clone(),
                        steps: Vec::new(),
                    };

                    self.profile.set_cell(self.selected_mode, cell);
                    monitor.set_profile(self.profile.clone());
                    self.status_message = Some((
                        x56_core::t!("prog-editor-confirmed-msg", "label" => &btn_label),
                        false,
                    ));

                    self.editing_button_id = None;
                    self.pending_keys.clear();
                    self.pending_name.clear();
                }

                if theme::ghost_button(ui, &x56_core::t!("prog-editor-cancel")).clicked() {
                    self.editing_button_id = None;
                    self.pending_keys.clear();
                    self.pending_name.clear();
                }
            });
        });
    }

    // ========================================================================
    // 7. TEST EVDEV LOOP MODAL
    // ========================================================================

    fn render_test_modal(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let new_events = monitor.drain_events();
        self.test_log.extend(new_events);
        if self.test_log.len() > 300 {
            self.test_log.drain(0..self.test_log.len() - 300);
        }

        let mut close_modal = false;
        let mut open = self.show_test_window;

        let win_title = x56_core::t!("prog-test-title");
        egui::Window::new(win_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(720.0, 480.0))
            .show(ui.ctx(), |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        theme::section_heading(ui, &x56_core::t!("prog-test-subtitle"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::danger_button(ui, &x56_core::t!("prog-test-clear-log"))
                                .clicked()
                            {
                                self.test_log.clear();
                            }
                        });
                    });

                    ui.add_space(SPACE_XS);

                    ui.horizontal(|ui| {
                        theme::meta_kv(
                            ui,
                            &x56_core::t!("prog-test-hw-mode"),
                            snapshot.active_mode.as_str(),
                            COLOR_ACCENT,
                        );
                        ui.add_space(SPACE_MD);
                        let is_active = monitor.is_active();
                        let (u_txt, u_col) = if is_active {
                            (x56_core::t!("prog-test-emitting"), COLOR_POSITIVE)
                        } else {
                            (x56_core::t!("prog-test-paused"), COLOR_WARNING)
                        };
                        theme::status_dot(ui, u_col, 8.0);
                        theme::meta_kv(ui, &x56_core::t!("prog-test-router"), &u_txt, u_col);
                    });

                    ui.add_space(SPACE_SM);

                    ui.label(
                        RichText::new(x56_core::t!("prog-test-input-label"))
                            .color(COLOR_TEXT_SECONDARY)
                            .size(FONT_SMALL),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.test_input_text)
                            .hint_text(x56_core::t!("prog-test-input-hint")),
                    );

                    ui.add_space(SPACE_SM);
                    ui.separator();
                    ui.add_space(SPACE_XS);

                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.test_log.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(SPACE_LG);
                                    ui.label(
                                        RichText::new(x56_core::t!("prog-test-waiting"))
                                            .color(COLOR_TEXT_MUTED)
                                            .size(FONT_SMALL),
                                    );
                                    ui.add_space(SPACE_LG);
                                });
                            } else {
                                egui::Grid::new("test_events_grid")
                                    .striped(true)
                                    .spacing(Vec2::new(SPACE_MD, SPACE_SM))
                                    .show(ui, |ui| {
                                        for col_key in [
                                            "prog-test-col-time",
                                            "prog-test-col-mode",
                                            "prog-test-col-button",
                                            "prog-test-col-command",
                                            "prog-test-col-keys",
                                            "prog-test-col-router",
                                        ] {
                                            ui.label(
                                                RichText::new(x56_core::t!(col_key))
                                                    .color(COLOR_TEXT_MUTED)
                                                    .size(FONT_META),
                                            );
                                        }
                                        ui.end_row();

                                        for ev in &self.test_log {
                                            ui.label(
                                                RichText::new(&ev.timestamp_hms)
                                                    .color(COLOR_TEXT_SECONDARY)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(ev.mode.as_str())
                                                    .color(COLOR_ACCENT)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(&ev.button_label)
                                                    .color(COLOR_TEXT_PRIMARY)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(&ev.command_name)
                                                    .color(COLOR_POSITIVE)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(ev.keys.join("+"))
                                                    .color(COLOR_TEXT_SECONDARY)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            let (st_txt, st_col) = if ev.uinput_emitted {
                                                (
                                                    x56_core::t!("prog-test-status-emitted"),
                                                    COLOR_POSITIVE,
                                                )
                                            } else {
                                                (
                                                    x56_core::t!("prog-test-status-simulated"),
                                                    COLOR_TEXT_MUTED,
                                                )
                                            };
                                            ui.label(
                                                RichText::new(st_txt)
                                                    .color(st_col)
                                                    .size(FONT_META)
                                                    .monospace(),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            }
                        });

                    ui.add_space(SPACE_SM);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::secondary_button(ui, &x56_core::t!("prog-macro-close")).clicked()
                        {
                            close_modal = true;
                        }
                    });
                });
            });

        if !open || close_modal {
            self.show_test_window = false;
        }
    }
}

// ============================================================================
// LOCALIZED HELPERS
// ============================================================================

fn short_view_label(full: &str) -> String {
    full.split(": ").nth(1).unwrap_or(full).trim().to_string()
}

fn unassigned_label() -> String {
    let raw = x56_core::t!("prog-unassigned-suggestion", "key" => "", "action" => "");
    raw.split(" — ").next().unwrap_or(&raw).trim().to_string()
}

pub(crate) struct ControlMeta {
    pub(crate) kind: String,
    pub(crate) group: String,
}

pub(crate) fn control_meta_for(id: &str) -> ControlMeta {
    let (kind_key, group_key) = match id {
        "stick_trigger" => ("prog-maptype-hold", "prog-group-weapons"),
        "stick_btn_a" => ("prog-maptype-instant", "prog-group-targeting"),
        "stick_btn_b" => ("prog-maptype-toggle-delay", "prog-group-systems"),
        "stick_btn_c" => ("prog-maptype-instant", "prog-group-sensors"),
        "stick_btn_d" => ("prog-maptype-toggle", "prog-group-flight"),
        "stick_pinkie" => ("prog-maptype-hold", "prog-group-flight"),
        "stick_pov_up" | "stick_pov_right" | "stick_pov_down" | "stick_pov_left" => {
            ("prog-maptype-analog-pov", "prog-group-view")
        }
        "stick_h1_up" | "stick_h1_right" | "stick_h1_down" | "stick_h1_left" => {
            ("prog-maptype-instant", "prog-group-radar")
        }
        "stick_h2_up" | "stick_h2_right" | "stick_h2_down" | "stick_h2_left" => {
            ("prog-maptype-instant", "prog-group-power")
        }
        "thr_sw_1" | "thr_sw_2" => ("prog-maptype-toggle-safe", "prog-group-flight"),
        "thr_sld" => ("prog-maptype-positional", "prog-group-flight"),
        "thr_btn_e" => ("prog-maptype-hold-spool", "prog-group-navigation"),
        "thr_btn_h" => ("prog-maptype-toggle", "prog-group-mining"),
        "thr_btn_i" => ("prog-maptype-toggle", "prog-group-systems"),
        "thr_ministick_push" => ("prog-maptype-instant", "prog-group-strafe"),
        "thr_rty_f_push" => ("prog-maptype-toggle", "prog-group-propulsion"),
        "thr_rty_g_push" => ("prog-maptype-hold", "prog-group-mining"),
        "thr_scroll_fwd" | "thr_scroll_bck" => ("prog-maptype-instant", "prog-group-propulsion"),
        "thr_k1_up" | "thr_k1_down" => ("prog-maptype-toggle", "prog-group-flight"),
        _ => ("prog-maptype-instant", "prog-group-general"),
    };
    ControlMeta {
        kind: x56_core::t!(kind_key),
        group: x56_core::t!(group_key),
    }
}

pub(crate) fn localized_node_name_by_id(id: &str, fallback: &str) -> String {
    let key = match id {
        "stick_pov_up" => Some("prog-label-pov-up"),
        "stick_pov_right" => Some("prog-label-pov-right"),
        "stick_pov_down" => Some("prog-label-pov-down"),
        "stick_pov_left" => Some("prog-label-pov-left"),
        "stick_btn_a" => Some("prog-label-btn-a"),
        "stick_h1_up" => Some("prog-label-h1-up"),
        "stick_h1_right" => Some("prog-label-h1-right"),
        "stick_h1_down" => Some("prog-label-h1-down"),
        "stick_h1_left" => Some("prog-label-h1-left"),
        "stick_btn_b" => Some("prog-label-btn-b"),
        "stick_h2_up" => Some("prog-label-h2-up"),
        "stick_h2_right" => Some("prog-label-h2-right"),
        "stick_h2_down" => Some("prog-label-h2-down"),
        "stick_h2_left" => Some("prog-label-h2-left"),
        "stick_trigger" => Some("prog-label-trigger"),
        "stick_btn_c" => Some("prog-label-btn-c"),
        "stick_pinkie" => Some("prog-label-pinkie"),
        "stick_btn_d" => Some("prog-label-btn-d"),
        "thr_scroll_fwd" => Some("prog-label-scroll-fwd"),
        "thr_scroll_bck" => Some("prog-label-scroll-bck"),
        "thr_k1_up" => Some("prog-label-k1-up"),
        "thr_k1_down" => Some("prog-label-k1-down"),
        "thr_btn_i" => Some("prog-label-btn-i"),
        "thr_btn_h" => Some("prog-label-btn-h"),
        "thr_sld" => Some("prog-label-sld"),
        "thr_btn_e" => Some("prog-label-btn-e"),
        "thr_rty_f_push" => Some("prog-label-rty-f-push"),
        "thr_rty_g_push" => Some("prog-label-rty-g-push"),
        "thr_h3_up" => Some("prog-label-h3-up"),
        "thr_h3_right" => Some("prog-label-h3-right"),
        "thr_h3_down" => Some("prog-label-h3-down"),
        "thr_h3_left" => Some("prog-label-h3-left"),
        "thr_h4_up" => Some("prog-label-h4-up"),
        "thr_h4_right" => Some("prog-label-h4-right"),
        "thr_h4_down" => Some("prog-label-h4-down"),
        "thr_h4_left" => Some("prog-label-h4-left"),
        "thr_sw_13" => Some("prog-label-sw13"),
        "thr_sw_14" => Some("prog-label-sw14"),
        "thr_sw_11" => Some("prog-label-sw11"),
        "thr_sw_12" => Some("prog-label-sw12"),
        "thr_sw_9" => Some("prog-label-sw9"),
        "thr_sw_10" => Some("prog-label-sw10"),
        "thr_sw_7" => Some("prog-label-sw7"),
        "thr_sw_8" => Some("prog-label-sw8"),
        "thr_sw_1" => Some("prog-label-sw1"),
        "thr_sw_2" => Some("prog-label-sw2"),
        "thr_sw_3" => Some("prog-label-sw3"),
        "thr_sw_4" => Some("prog-label-sw4"),
        "thr_sw_5" => Some("prog-label-sw5"),
        "thr_sw_6" => Some("prog-label-sw6"),
        _ => None,
    };
    key.map(|k| x56_core::t!(k))
        .unwrap_or_else(|| fallback.to_string())
}

/// Maps an egui key to a canonical Linux / uinput key name.
fn map_key_to_name(key: Key) -> Option<&'static str> {
    match key {
        Key::ControlLeft => Some("Ctrl"),
        Key::ControlRight => Some("RCtrl"),
        Key::ShiftLeft => Some("Shift"),
        Key::ShiftRight => Some("RShift"),
        Key::AltLeft => Some("Alt"),
        Key::AltRight => Some("AltGr"),
        Key::Escape => Some("Escape"),
        Key::Tab => Some("Tab"),
        Key::Backspace => Some("Backspace"),
        Key::Enter => Some("Enter"),
        Key::Space => Some("Space"),
        Key::ArrowUp => Some("Up"),
        Key::ArrowDown => Some("Down"),
        Key::ArrowLeft => Some("Left"),
        Key::ArrowRight => Some("Right"),
        Key::Delete => Some("Delete"),
        Key::Insert => Some("Insert"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::A => Some("A"),
        Key::B => Some("B"),
        Key::C => Some("C"),
        Key::D => Some("D"),
        Key::E => Some("E"),
        Key::F => Some("F"),
        Key::G => Some("G"),
        Key::H => Some("H"),
        Key::I => Some("I"),
        Key::J => Some("J"),
        Key::K => Some("K"),
        Key::L => Some("L"),
        Key::M => Some("M"),
        Key::N => Some("N"),
        Key::O => Some("O"),
        Key::P => Some("P"),
        Key::Q => Some("Q"),
        Key::R => Some("R"),
        Key::S => Some("S"),
        Key::T => Some("T"),
        Key::U => Some("U"),
        Key::V => Some("V"),
        Key::W => Some("W"),
        Key::X => Some("X"),
        Key::Y => Some("Y"),
        Key::Z => Some("Z"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::Minus => Some("-"),
        Key::Equals => Some("="),
        Key::OpenBracket => Some("["),
        Key::CloseBracket => Some("]"),
        Key::Semicolon => Some(";"),
        Key::Quote => Some("'"),
        Key::Comma => Some(","),
        Key::Period => Some("."),
        Key::Slash => Some("/"),
        Key::Backslash => Some("\\"),
        Key::Backtick => Some("`"),
        _ => None,
    }
}

// ============================================================================
// SCHEMATIC NODES FOR 2D VECTOR MAP
// ============================================================================

pub(crate) struct SchematicNode {
    pub(crate) id: &'static str,
    pub(crate) short_label: &'static str,
    pub(crate) full_name: &'static str,
    pub(crate) norm_x: f32,
    pub(crate) norm_y: f32,
    pub(crate) radius: f32,
    pub(crate) is_rect: bool,
    pub(crate) rect_size: Vec2,
}

pub(crate) fn get_device_schematic_nodes(kind: DeviceKind) -> Vec<SchematicNode> {
    match kind {
        DeviceKind::Stick => vec![
            // 1. POV 8-Way (cx = 0.330, cy = 0.242)
            SchematicNode {
                id: "stick_pov_up",
                short_label: "▲",
                full_name: "POV Vista Arriba",
                norm_x: 0.330,
                norm_y: 0.198,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_right",
                short_label: "▶",
                full_name: "POV Vista Derecha",
                norm_x: 0.362,
                norm_y: 0.242,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_down",
                short_label: "▼",
                full_name: "POV Vista Abajo",
                norm_x: 0.330,
                norm_y: 0.286,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_left",
                short_label: "◀",
                full_name: "POV Vista Izquierda",
                norm_x: 0.298,
                norm_y: 0.242,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 2. Button A (cx = 0.415, cy = 0.138)
            SchematicNode {
                id: "stick_btn_a",
                short_label: "A",
                full_name: "Botón A (Cabezal Superior)",
                norm_x: 0.415,
                norm_y: 0.138,
                radius: 24.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 3. HAT 1 (Witches) (cx = 0.525, cy = 0.121)
            SchematicNode {
                id: "stick_h1_up",
                short_label: "▲",
                full_name: "HAT 1 (Witches) Arriba",
                norm_x: 0.525,
                norm_y: 0.077,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_right",
                short_label: "▶",
                full_name: "HAT 1 (Witches) Derecha",
                norm_x: 0.557,
                norm_y: 0.121,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_down",
                short_label: "▼",
                full_name: "HAT 1 (Witches) Abajo",
                norm_x: 0.525,
                norm_y: 0.165,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_left",
                short_label: "◀",
                full_name: "HAT 1 (Witches) Izquierda",
                norm_x: 0.493,
                norm_y: 0.121,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 4. Button B (cx = 0.670, cy = 0.072)
            SchematicNode {
                id: "stick_btn_b",
                short_label: "B",
                full_name: "Botón B (Lateral Pulgar)",
                norm_x: 0.670,
                norm_y: 0.072,
                radius: 22.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 5. HAT 2 (Castle) (cx = 0.630, cy = 0.285)
            SchematicNode {
                id: "stick_h2_up",
                short_label: "▲",
                full_name: "HAT 2 (Castle) Arriba",
                norm_x: 0.630,
                norm_y: 0.241,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_right",
                short_label: "▶",
                full_name: "HAT 2 (Castle) Derecha",
                norm_x: 0.662,
                norm_y: 0.285,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_down",
                short_label: "▼",
                full_name: "HAT 2 (Castle) Abajo",
                norm_x: 0.630,
                norm_y: 0.329,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_left",
                short_label: "◀",
                full_name: "HAT 2 (Castle) Izquierda",
                norm_x: 0.598,
                norm_y: 0.285,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 6. Trigger (cx = 0.500, cy = 0.420)
            SchematicNode {
                id: "stick_trigger",
                short_label: "TRG",
                full_name: "Gatillo Principal (Trigger)",
                norm_x: 0.500,
                norm_y: 0.420,
                radius: 16.0,
                is_rect: true,
                rect_size: Vec2::new(54.0, 26.0),
            },
            // 7. Button C (cx = 0.440, cy = 0.560)
            SchematicNode {
                id: "stick_btn_c",
                short_label: "C",
                full_name: "Botón C (Minipalanca Push)",
                norm_x: 0.440,
                norm_y: 0.560,
                radius: 20.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 8. Pinkie Lever (cx = 0.500, cy = 0.700)
            SchematicNode {
                id: "stick_pinkie",
                short_label: "PNK",
                full_name: "Palanca Meñique (Pinkie Lever)",
                norm_x: 0.500,
                norm_y: 0.700,
                radius: 16.0,
                is_rect: true,
                rect_size: Vec2::new(64.0, 26.0),
            },
            // 9. Button D (cx = 0.570, cy = 0.840)
            SchematicNode {
                id: "stick_btn_d",
                short_label: "D",
                full_name: "Botón D (Meñique)",
                norm_x: 0.570,
                norm_y: 0.840,
                radius: 22.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
        ],
        DeviceKind::Throttle => vec![
            // 1. Scroll Wheel (SC)
            SchematicNode {
                id: "thr_scroll_fwd",
                short_label: "▲",
                full_name: "Rueda Scroll Adelante",
                norm_x: 0.1169,
                norm_y: 0.2042,
                radius: 14.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_scroll_bck",
                short_label: "▼",
                full_name: "Rueda Scroll Atrás",
                norm_x: 0.1169,
                norm_y: 0.3244,
                radius: 14.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 2. K1 Switch
            SchematicNode {
                id: "thr_k1_up",
                short_label: "▲",
                full_name: "Interruptor K1 Arriba",
                norm_x: 0.1992,
                norm_y: 0.2029,
                radius: 14.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_k1_down",
                short_label: "▼",
                full_name: "Interruptor K1 Abajo",
                norm_x: 0.1992,
                norm_y: 0.3308,
                radius: 14.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 3. Grip Buttons I, H, SLD, E
            SchematicNode {
                id: "thr_btn_i",
                short_label: "I",
                full_name: "Botón I",
                norm_x: 0.4107,
                norm_y: 0.2337,
                radius: 22.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_btn_h",
                short_label: "H",
                full_name: "Botón H",
                norm_x: 0.5039,
                norm_y: 0.2337,
                radius: 22.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sld",
                short_label: "SLD",
                full_name: "Control Deslizante SLD (2 Posiciones)",
                norm_x: 0.5034,
                norm_y: 0.3671,
                radius: 14.0,
                is_rect: true,
                rect_size: Vec2::new(52.0, 30.0),
            },
            SchematicNode {
                id: "thr_btn_e",
                short_label: "E",
                full_name: "Botón E (Pulgar)",
                norm_x: 0.4929,
                norm_y: 0.4957,
                radius: 14.0,
                is_rect: true,
                rect_size: Vec2::new(34.0, 34.0),
            },
            // 4. Rotaries F and G
            SchematicNode {
                id: "thr_rty_f_push",
                short_label: "F",
                full_name: "Giratorio F Push (RTY 5)",
                norm_x: 0.6314,
                norm_y: 0.1494,
                radius: 28.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_rty_g_push",
                short_label: "G",
                full_name: "Giratorio G Push (RTY 6)",
                norm_x: 0.6314,
                norm_y: 0.3443,
                radius: 28.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 5. HAT 3 (H3)
            SchematicNode {
                id: "thr_h3_up",
                short_label: "▲",
                full_name: "HAT 3 Arriba (H3▲)",
                norm_x: 0.5887,
                norm_y: 0.4596,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_right",
                short_label: "▶",
                full_name: "HAT 3 Derecha (H3▶)",
                norm_x: 0.6155,
                norm_y: 0.4996,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_down",
                short_label: "▼",
                full_name: "HAT 3 Abajo (H3▼)",
                norm_x: 0.5887,
                norm_y: 0.5396,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_left",
                short_label: "◀",
                full_name: "HAT 3 Izquierda (H3◀)",
                norm_x: 0.5619,
                norm_y: 0.4996,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 6. HAT 4 (H4)
            SchematicNode {
                id: "thr_h4_up",
                short_label: "▲",
                full_name: "HAT 4 Arriba (H4▲)",
                norm_x: 0.5489,
                norm_y: 0.6052,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_right",
                short_label: "▶",
                full_name: "HAT 4 Derecha (H4▶)",
                norm_x: 0.5757,
                norm_y: 0.6452,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_down",
                short_label: "▼",
                full_name: "HAT 4 Abajo (H4▼)",
                norm_x: 0.5489,
                norm_y: 0.6852,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_left",
                short_label: "◀",
                full_name: "HAT 4 Izquierda (H4◀)",
                norm_x: 0.5221,
                norm_y: 0.6452,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 7. Right Toggle Column: TGL 4, TGL 3, TGL 2, TGL 1
            SchematicNode {
                id: "thr_sw_13",
                short_label: "▲",
                full_name: "Toggle 4 Arriba (TGL 4 Up)",
                norm_x: 0.8771,
                norm_y: 0.2822,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_14",
                short_label: "▼",
                full_name: "Toggle 4 Abajo (TGL 4 Down)",
                norm_x: 0.8771,
                norm_y: 0.3811,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_11",
                short_label: "▲",
                full_name: "Toggle 3 Arriba (TGL 3 Up)",
                norm_x: 0.7949,
                norm_y: 0.3961,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_12",
                short_label: "▼",
                full_name: "Toggle 3 Abajo (TGL 3 Down)",
                norm_x: 0.7949,
                norm_y: 0.4951,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_9",
                short_label: "▲",
                full_name: "Toggle 2 Arriba (TGL 2 Up)",
                norm_x: 0.8771,
                norm_y: 0.5480,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_10",
                short_label: "▼",
                full_name: "Toggle 2 Abajo (TGL 2 Down)",
                norm_x: 0.8771,
                norm_y: 0.6470,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_7",
                short_label: "▲",
                full_name: "Toggle 1 Arriba (TGL 1 Up)",
                norm_x: 0.7949,
                norm_y: 0.6619,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_8",
                short_label: "▼",
                full_name: "Toggle 1 Abajo (TGL 1 Down)",
                norm_x: 0.7949,
                norm_y: 0.7609,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 8. Bottom Switches: SW 1/2, SW 3/4, SW 5/6
            SchematicNode {
                id: "thr_sw_1",
                short_label: "1",
                full_name: "Switch 1 Arriba (SW 1)",
                norm_x: 0.2924,
                norm_y: 0.8142,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_2",
                short_label: "2",
                full_name: "Switch 2 Abajo (SW 2)",
                norm_x: 0.2924,
                norm_y: 0.9200,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_3",
                short_label: "3",
                full_name: "Switch 3 Arriba (SW 3)",
                norm_x: 0.4619,
                norm_y: 0.8142,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_4",
                short_label: "4",
                full_name: "Switch 4 Abajo (SW 4)",
                norm_x: 0.4619,
                norm_y: 0.9200,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_5",
                short_label: "5",
                full_name: "Switch 5 Arriba (SW 5)",
                norm_x: 0.6314,
                norm_y: 0.8142,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_sw_6",
                short_label: "6",
                full_name: "Switch 6 Abajo (SW 6)",
                norm_x: 0.6314,
                norm_y: 0.9200,
                radius: 15.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_schematic_nodes_match_svg_spec() {
        let nodes = get_device_schematic_nodes(DeviceKind::Throttle);
        assert_eq!(nodes.len(), 32);

        let node_f = nodes.iter().find(|n| n.id == "thr_rty_f_push").unwrap();
        assert_eq!(node_f.short_label, "F");
        assert!((node_f.norm_x - 0.6314).abs() < 1e-3);
        assert!((node_f.norm_y - 0.1494).abs() < 1e-3);

        let node_g = nodes.iter().find(|n| n.id == "thr_rty_g_push").unwrap();
        assert_eq!(node_g.short_label, "G");
        assert!((node_g.norm_x - 0.6314).abs() < 1e-3);
        assert!((node_g.norm_y - 0.3443).abs() < 1e-3);

        let h3_up = nodes.iter().find(|n| n.id == "thr_h3_up").unwrap();
        assert!(h3_up.full_name.contains("H3▲"));
        assert!((h3_up.norm_x - 0.5887).abs() < 1e-3);

        let h4_up = nodes.iter().find(|n| n.id == "thr_h4_up").unwrap();
        assert!(h4_up.full_name.contains("H4▲"));
        assert!((h4_up.norm_x - 0.5489).abs() < 1e-3);

        let sc_fwd = nodes.iter().find(|n| n.id == "thr_scroll_fwd").unwrap();
        assert!((sc_fwd.norm_x - 0.1169).abs() < 1e-3);

        let k1_up = nodes.iter().find(|n| n.id == "thr_k1_up").unwrap();
        assert!((k1_up.norm_x - 0.1992).abs() < 1e-3);

        let tgl4_up = nodes.iter().find(|n| n.id == "thr_sw_13").unwrap();
        assert!((tgl4_up.norm_x - 0.8771).abs() < 1e-3);

        let sw1 = nodes.iter().find(|n| n.id == "thr_sw_1").unwrap();
        assert_eq!(sw1.short_label, "1");
        assert!((sw1.norm_y - 0.8142).abs() < 1e-3);
    }

    #[test]
    fn test_filter_category_localized_labels() {
        assert!(!FilterCategory::Todos.label().is_empty());
        assert!(!FilterCategory::Gatillos.label().is_empty());
        assert!(!FilterCategory::Setas.label().is_empty());
        assert!(!FilterCategory::BotonesFrontales.label().is_empty());
        assert!(!FilterCategory::Macros.label().is_empty());
        assert!(!FilterCategory::Shift.label().is_empty());
    }

    #[test]
    fn test_record_key_chord_timing_and_filtering() {
        use std::time::Duration;

        let mut tab = ProgrammingTab::default();
        let t0 = Instant::now();

        // Empty chord is ignored
        tab.record_key_chord(Vec::new(), t0);
        assert!(tab.recorded_steps.is_empty());

        // First step always has delay_ms == 0
        tab.record_key_chord(vec!["Ctrl".to_string(), "A".to_string()], t0);
        assert_eq!(tab.recorded_steps.len(), 1);
        assert_eq!(tab.recorded_steps[0].delay_ms, 0);
        assert_eq!(tab.recorded_steps[0].keys, vec!["Ctrl", "A"]);

        // Second step with macro_record_timings == true uses elapsed ms, capped at 5000
        let t1 = t0 + Duration::from_millis(240);
        tab.record_key_chord(vec!["B".to_string()], t1);
        assert_eq!(tab.recorded_steps.len(), 2);
        assert_eq!(tab.recorded_steps[1].delay_ms, 240);

        let t2 = t1 + Duration::from_secs(12);
        tab.record_key_chord(vec!["C".to_string()], t2);
        assert_eq!(tab.recorded_steps.len(), 3);
        assert_eq!(tab.recorded_steps[2].delay_ms, 5000);

        // With macro_record_timings == false, subsequent steps use fixed 50ms
        tab.macro_record_timings = false;
        let t3 = t2 + Duration::from_millis(900);
        tab.record_key_chord(vec!["D".to_string()], t3);
        assert_eq!(tab.recorded_steps.len(), 4);
        assert_eq!(tab.recorded_steps[3].delay_ms, 50);
    }

    #[test]
    fn test_filter_category_macros_matches_sequences() {
        let mut tab = ProgrammingTab {
            filter_category: FilterCategory::Macros,
            ..Default::default()
        };
        assert!(!tab.matches_category("stick_trigger"));

        tab.profile.set_cell(
            tab.selected_mode,
            CommandCell {
                button_id: "stick_trigger".to_string(),
                name: "Secuencia".to_string(),
                keys: Vec::new(),
                steps: vec![MacroStep {
                    keys: vec!["A".to_string()],
                    delay_ms: 0,
                }],
            },
        );
        assert!(tab.matches_category("stick_trigger"));
    }

    #[test]
    fn test_filter_category_shift_and_clean_defaults() {
        let tab = ProgrammingTab {
            filter_category: FilterCategory::Shift,
            ..Default::default()
        };
        assert_eq!(tab.last_hex_code, "----");
        assert!(tab.matches_category("stick_pinkie"));
        assert!(tab.matches_category("thr_sld"));
        assert!(!tab.matches_category("stick_flying"));
        assert!(!tab.matches_category("stick_trigger"));
    }
}
