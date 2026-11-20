//! Interactive RGB LED lighting control tab per mode (M1, M2, S1) and presets.

use eframe::egui::{self, Color32, CornerRadius, FontId, RichText, Sense, Stroke, Ui, Vec2};
use x56_core::{
    parse_color, ButtonMonitor, DeviceKind, FullProfile, ModeBindings, ModeSlot,
    ProgrammingProfile, RgbColor, TargetDevice, X56Manager,
};

use crate::theme;

/// Mode scope for RGB lighting selection and persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RgbModeScope {
    /// Mode 1 (Standard).
    #[default]
    M1,
    /// Mode 2 (Secondary).
    M2,
    /// Mode S1 (Auxiliary).
    S1,
    /// All modes (M1, M2, S1) and base profile fallback color.
    AllModes,
}

impl RgbModeScope {
    pub fn from_slot(slot: ModeSlot) -> Self {
        match slot {
            ModeSlot::M1 => Self::M1,
            ModeSlot::M2 => Self::M2,
            ModeSlot::S1 => Self::S1,
        }
    }

    pub fn as_slot(self) -> Option<ModeSlot> {
        match self {
            Self::M1 => Some(ModeSlot::M1),
            Self::M2 => Some(ModeSlot::M2),
            Self::S1 => Some(ModeSlot::S1),
            Self::AllModes => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::M1 => x56_core::t!("rgb-mode-m1"),
            Self::M2 => x56_core::t!("rgb-mode-m2"),
            Self::S1 => x56_core::t!("rgb-mode-s1"),
            Self::AllModes => x56_core::t!("rgb-mode-global"),
        }
    }

    fn localized_short_label(self) -> String {
        match self {
            Self::M1 => "M1".to_string(),
            Self::M2 => "M2".to_string(),
            Self::S1 => "S1".to_string(),
            Self::AllModes => x56_core::t!("rgb-mode-all-short"),
        }
    }
}

pub struct RgbTab {
    pub selected_mode: RgbModeScope,
    pub follow_hardware_mode: bool,
    pub last_seen_hw_mode: Option<ModeSlot>,
    pub target: TargetDevice,
    pub color_rgba: [f32; 3],
    pub fallback_stick_hex: String,
    pub fallback_throttle_hex: String,
    pub status_message: Option<(String, bool)>,
    subcard_height: f32,
}

impl Default for RgbTab {
    fn default() -> Self {
        Self {
            selected_mode: RgbModeScope::M1,
            follow_hardware_mode: false,
            last_seen_hw_mode: None,
            target: TargetDevice::All,
            color_rgba: [0.0, 1.0, 1.0],
            fallback_stick_hex: "#00ffff".to_string(),
            fallback_throttle_hex: "#00ffff".to_string(),
            status_message: None,
            subcard_height: 0.0,
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

    pub fn current_hex(&self) -> String {
        let c = self.current_rgb_color();
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// Resolves the effective hex color for a given mode and device.
    pub fn effective_hex_for(
        &self,
        profile: &ProgrammingProfile,
        mode: ModeSlot,
        device: DeviceKind,
    ) -> String {
        profile
            .rgb_for_mode(mode, device)
            .map(str::to_string)
            .unwrap_or_else(|| match device {
                DeviceKind::Stick => self.fallback_stick_hex.clone(),
                DeviceKind::Throttle => self.fallback_throttle_hex.clone(),
            })
    }

    /// Converts a hex string (`#RRGGBB`) to `Color32` for UI preview.
    pub fn hex_to_color32(hex: &str) -> Color32 {
        parse_color(hex)
            .map(|c| Color32::from_rgb(c.r, c.g, c.b))
            .unwrap_or(Color32::from_rgb(0, 255, 255))
    }

    /// Loads `color_rgba` from the currently selected mode and target device.
    pub fn load_picker_from_selection(&mut self, profile: &ProgrammingProfile) {
        let mode = self.selected_mode.as_slot().unwrap_or(ModeSlot::M1);
        let dev = match self.target {
            TargetDevice::Throttle => DeviceKind::Throttle,
            TargetDevice::All | TargetDevice::Stick => DeviceKind::Stick,
        };
        let hex = self.effective_hex_for(profile, mode, dev);
        if let Ok(c) = parse_color(&hex) {
            self.color_rgba = [
                (c.r as f32) / 255.0,
                (c.g as f32) / 255.0,
                (c.b as f32) / 255.0,
            ];
        }
    }

    /// Syncs RGB tab state when a full profile is loaded from disk.
    pub fn sync_from_loaded_profile(
        &mut self,
        full: &FullProfile,
        prog: &ProgrammingProfile,
        active_hw_mode: ModeSlot,
    ) {
        if let Some(s_rgb) = full.stick.as_ref().and_then(|s| s.rgb.as_ref()) {
            self.fallback_stick_hex = s_rgb.clone();
        }
        if let Some(t_rgb) = full.throttle.as_ref().and_then(|t| t.rgb.as_ref()) {
            self.fallback_throttle_hex = t_rgb.clone();
        }
        if self.follow_hardware_mode {
            self.selected_mode = RgbModeScope::from_slot(active_hw_mode);
        }
        self.load_picker_from_selection(prog);
    }

    /// Ensures all modes (`M1`, `M2`, `S1`) have explicit RGB entries in the profile.
    pub fn ensure_all_modes_have_rgb(&self, profile: &mut ProgrammingProfile) {
        for slot in ModeSlot::ALL {
            let stick_eff = self.effective_hex_for(profile, slot, DeviceKind::Stick);
            let thr_eff = self.effective_hex_for(profile, slot, DeviceKind::Throttle);
            let bindings = profile.bindings_for_mut(slot);
            if bindings.rgb.is_none()
                && bindings.stick_rgb.is_none()
                && bindings.throttle_rgb.is_none()
            {
                if stick_eff.eq_ignore_ascii_case(&thr_eff) {
                    bindings.rgb = Some(stick_eff);
                } else {
                    bindings.stick_rgb = Some(stick_eff);
                    bindings.throttle_rgb = Some(thr_eff);
                }
            }
        }
    }

    fn apply_hex_to_bindings(
        bindings: &mut ModeBindings,
        target: TargetDevice,
        hex: &str,
        fallback_stick: &str,
        fallback_throttle: &str,
    ) {
        match target {
            TargetDevice::All => {
                bindings.rgb = Some(hex.to_string());
                bindings.stick_rgb = None;
                bindings.throttle_rgb = None;
            }
            TargetDevice::Stick => {
                if bindings.throttle_rgb.is_none() {
                    let prev_thr = bindings
                        .rgb
                        .clone()
                        .unwrap_or_else(|| fallback_throttle.to_string());
                    bindings.throttle_rgb = Some(prev_thr);
                }
                bindings.stick_rgb = Some(hex.to_string());
                if bindings.stick_rgb == bindings.throttle_rgb {
                    bindings.rgb = bindings.stick_rgb.clone();
                    bindings.stick_rgb = None;
                    bindings.throttle_rgb = None;
                }
            }
            TargetDevice::Throttle => {
                if bindings.stick_rgb.is_none() {
                    let prev_stk = bindings
                        .rgb
                        .clone()
                        .unwrap_or_else(|| fallback_stick.to_string());
                    bindings.stick_rgb = Some(prev_stk);
                }
                bindings.throttle_rgb = Some(hex.to_string());
                if bindings.stick_rgb == bindings.throttle_rgb {
                    bindings.rgb = bindings.throttle_rgb.clone();
                    bindings.stick_rgb = None;
                    bindings.throttle_rgb = None;
                }
            }
        }
    }

    /// Updates the selected mode color in `ProgrammingProfile` and syncs `ButtonMonitor`.
    pub fn update_profile_color(
        &mut self,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) {
        let hex = self.current_hex();
        self.ensure_all_modes_have_rgb(profile);

        match self.selected_mode {
            RgbModeScope::M1 | RgbModeScope::M2 | RgbModeScope::S1 => {
                let slot = self.selected_mode.as_slot().unwrap_or(ModeSlot::M1);
                let bindings = profile.bindings_for_mut(slot);
                Self::apply_hex_to_bindings(
                    bindings,
                    self.target,
                    &hex,
                    &self.fallback_stick_hex,
                    &self.fallback_throttle_hex,
                );
                if slot == ModeSlot::M1 {
                    match self.target {
                        TargetDevice::All => {
                            self.fallback_stick_hex = hex.clone();
                            self.fallback_throttle_hex = hex;
                        }
                        TargetDevice::Stick => self.fallback_stick_hex = hex,
                        TargetDevice::Throttle => self.fallback_throttle_hex = hex,
                    }
                }
            }
            RgbModeScope::AllModes => {
                match self.target {
                    TargetDevice::All => {
                        self.fallback_stick_hex = hex.clone();
                        self.fallback_throttle_hex = hex.clone();
                    }
                    TargetDevice::Stick => self.fallback_stick_hex = hex.clone(),
                    TargetDevice::Throttle => self.fallback_throttle_hex = hex.clone(),
                }
                for slot in ModeSlot::ALL {
                    let bindings = profile.bindings_for_mut(slot);
                    Self::apply_hex_to_bindings(
                        bindings,
                        self.target,
                        &hex,
                        &self.fallback_stick_hex,
                        &self.fallback_throttle_hex,
                    );
                }
            }
        }

        monitor.set_fallback_rgb(
            Some(self.fallback_stick_hex.clone()),
            Some(self.fallback_throttle_hex.clone()),
        );
        monitor.set_profile(profile.clone());
    }

    fn localized_target_name(target: TargetDevice) -> String {
        match target {
            TargetDevice::All => x56_core::t!("rgb-target-both-short"),
            TargetDevice::Stick => x56_core::t!("rgb-target-stick-short"),
            TargetDevice::Throttle => x56_core::t!("rgb-target-throttle-short"),
        }
    }

    /// Applies the current color to the selected mode profile and USB hardware.
    pub fn apply_color(&mut self, profile: &mut ProgrammingProfile, monitor: &ButtonMonitor) {
        self.update_profile_color(profile, monitor);
        let color = self.current_rgb_color();
        let target = self.target;
        let hex = self.current_hex();
        let mode_lbl = self.selected_mode.localized_short_label();
        let target_desc = Self::localized_target_name(target);

        match X56Manager::set_rgb(target, color) {
            Ok(0) => {
                self.status_message = Some((
                    x56_core::t!("rgb-status-no-usb", "hex" => &hex, "mode" => &mode_lbl),
                    true,
                ));
            }
            Ok(count) => {
                let count_str = count.to_string();
                self.status_message = Some((
                    x56_core::t!(
                        "rgb-status-applied",
                        "hex" => &hex,
                        "mode" => &mode_lbl,
                        "target" => &target_desc,
                        "count" => &count_str
                    ),
                    false,
                ));
            }
            Err(err) => {
                let err_str = err.to_string();
                self.status_message = Some((
                    x56_core::t!(
                        "rgb-status-saved",
                        "hex" => &hex,
                        "mode" => &mode_lbl,
                        "target" => &target_desc,
                        "err" => &err_str
                    ),
                    true,
                ));
            }
        }
    }

    pub fn set_preset(
        &mut self,
        r: u8,
        g: u8,
        b: u8,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) {
        self.color_rgba = [(r as f32) / 255.0, (g as f32) / 255.0, (b as f32) / 255.0];
        self.apply_color(profile, monitor);
    }

    /// Applies a per-mode tricolor scheme across M1, M2, and S1.
    pub fn apply_tactical_tricolor_scheme(
        &mut self,
        m1_hex: &str,
        m2_hex: &str,
        s1_hex: &str,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) {
        self.fallback_stick_hex = m1_hex.to_string();
        self.fallback_throttle_hex = m1_hex.to_string();

        for (slot, hex) in [
            (ModeSlot::M1, m1_hex),
            (ModeSlot::M2, m2_hex),
            (ModeSlot::S1, s1_hex),
        ] {
            let b = profile.bindings_for_mut(slot);
            b.rgb = Some(hex.to_string());
            b.stick_rgb = None;
            b.throttle_rgb = None;
        }

        monitor.set_fallback_rgb(
            Some(self.fallback_stick_hex.clone()),
            Some(self.fallback_throttle_hex.clone()),
        );
        monitor.set_profile(profile.clone());
        self.load_picker_from_selection(profile);

        let active_hw = monitor.snapshot().active_mode;
        let active_hex = self.effective_hex_for(profile, active_hw, DeviceKind::Stick);
        let active_mode_str = active_hw.as_str();
        let target_desc = Self::localized_target_name(TargetDevice::All);

        self.status_message = Some(match parse_color(&active_hex) {
            Ok(col) => match X56Manager::set_rgb(TargetDevice::All, col) {
                Ok(0) => (
                    x56_core::t!(
                        "rgb-status-no-usb",
                        "hex" => &active_hex,
                        "mode" => active_mode_str
                    ),
                    true,
                ),
                Ok(n) => {
                    let count_str = n.to_string();
                    (
                        x56_core::t!(
                            "rgb-status-applied",
                            "hex" => &active_hex,
                            "mode" => active_mode_str,
                            "target" => &target_desc,
                            "count" => &count_str
                        ),
                        false,
                    )
                }
                Err(e) => {
                    let err_str = e.to_string();
                    (
                        x56_core::t!(
                            "rgb-status-saved",
                            "hex" => &active_hex,
                            "mode" => active_mode_str,
                            "target" => &target_desc,
                            "err" => &err_str
                        ),
                        true,
                    )
                }
            },
            Err(e) => {
                let err_str = e.to_string();
                (
                    x56_core::t!(
                        "rgb-status-saved",
                        "hex" => &active_hex,
                        "mode" => active_mode_str,
                        "target" => &target_desc,
                        "err" => &err_str
                    ),
                    true,
                )
            }
        });
    }

    /// Renders the per-mode RGB lighting UI.
    /// Returns `true` if the user requested saving changes to the TOML profile.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) -> bool {
        ui.set_min_width(ui.available_width());
        let mut save_requested = false;
        let snap = monitor.snapshot();

        if self.follow_hardware_mode && self.last_seen_hw_mode != Some(snap.active_mode) {
            self.last_seen_hw_mode = Some(snap.active_mode);
            self.selected_mode = RgbModeScope::from_slot(snap.active_mode);
            self.load_picker_from_selection(profile);
        } else {
            self.last_seen_hw_mode = Some(snap.active_mode);
        }

        ui.horizontal_wrapped(|ui| {
            theme::section_heading(ui, &x56_core::t!("rgb-title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .checkbox(
                        &mut self.follow_hardware_mode,
                        RichText::new(x56_core::t!("rgb-follow-hw"))
                            .color(theme::COLOR_TEXT_SECONDARY)
                            .size(theme::FONT_SMALL),
                    )
                    .on_hover_text(x56_core::t!("rgb-follow-hw-tooltip"))
                    .changed()
                    && self.follow_hardware_mode
                {
                    self.selected_mode = RgbModeScope::from_slot(snap.active_mode);
                    self.load_picker_from_selection(profile);
                }
            });
        });

        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(x56_core::t!("rgb-desc"))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );

        ui.add_space(theme::SPACE_MD);

        self.render_mode_selector_bar(ui, profile, snap.active_mode);

        ui.add_space(theme::SPACE_MD);

        theme::subcard_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(x56_core::t!("rgb-target-device"))
                        .color(theme::COLOR_TEXT_SECONDARY)
                        .size(theme::FONT_SMALL),
                );
                ui.add_space(theme::SPACE_SM);

                let targets = [
                    (TargetDevice::All, x56_core::t!("rgb-target-both-short")),
                    (TargetDevice::Stick, x56_core::t!("rgb-target-stick-short")),
                    (
                        TargetDevice::Throttle,
                        x56_core::t!("rgb-target-throttle-short"),
                    ),
                ];
                ui.columns(3, |cols| {
                    for (idx, (dev, lbl)) in targets.iter().enumerate() {
                        cols[idx].set_min_width(cols[idx].available_width());
                        let active = self.target == *dev;
                        if cols[idx]
                            .add_sized(
                                Vec2::new(cols[idx].available_width(), 32.0),
                                |ui: &mut Ui| theme::segmented_button(ui, lbl, active),
                            )
                            .clicked()
                        {
                            self.target = *dev;
                            self.load_picker_from_selection(profile);
                        }
                    }
                });
            });
        });

        ui.add_space(theme::SPACE_SM);

        // Fixed-height status feedback row to prevent layout jumping
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if let Some((ref msg, is_err)) = self.status_message {
                    let col = if is_err {
                        theme::COLOR_NEGATIVE
                    } else {
                        theme::COLOR_POSITIVE
                    };
                    theme::status_dot(ui, col, 8.0);
                    ui.label(RichText::new(msg).color(col).size(theme::FONT_SMALL));
                }
            },
        );

        ui.add_space(theme::SPACE_SM);

        let available_w = ui.available_width();
        if available_w >= 580.0 {
            let prev_h = self.subcard_height;
            let mut left_natural_h = 0.0_f32;
            let mut right_natural_h = 0.0_f32;

            ui.columns(2, |cols| {
                theme::subcard_frame().show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    let top = ui.cursor().top();
                    if prev_h > 0.0 {
                        ui.set_min_height(prev_h);
                    }
                    if self.render_color_picker(ui, profile, monitor) {
                        save_requested = true;
                    }
                    left_natural_h = (ui.cursor().top() - top).max(0.0);
                });

                theme::subcard_frame().show(&mut cols[1], |ui| {
                    ui.set_min_width(ui.available_width());
                    let top = ui.cursor().top();
                    if prev_h > 0.0 {
                        ui.set_min_height(prev_h);
                    }
                    self.render_presets(ui, profile, monitor);
                    right_natural_h = (ui.cursor().top() - top).max(0.0);
                });
            });

            self.subcard_height = left_natural_h.max(right_natural_h);
        } else {
            theme::subcard_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if self.render_color_picker(ui, profile, monitor) {
                    save_requested = true;
                }
            });
            ui.add_space(theme::SPACE_MD);
            theme::subcard_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                self.render_presets(ui, profile, monitor);
            });
        }

        save_requested
    }

    fn render_mode_selector_bar(
        &mut self,
        ui: &mut Ui,
        profile: &ProgrammingProfile,
        active_hw_mode: ModeSlot,
    ) {
        let wide = ui.available_width() >= 580.0;
        if wide {
            ui.columns(4, |cols| {
                for (idx, slot) in ModeSlot::ALL.into_iter().enumerate() {
                    self.render_slot_card(&mut cols[idx], profile, slot, active_hw_mode);
                }
                self.render_all_modes_card(&mut cols[3], profile);
            });
        } else {
            ui.columns(2, |cols| {
                self.render_slot_card(&mut cols[0], profile, ModeSlot::M1, active_hw_mode);
                self.render_slot_card(&mut cols[1], profile, ModeSlot::M2, active_hw_mode);
            });
            ui.add_space(theme::SPACE_SM);
            ui.columns(2, |cols| {
                self.render_slot_card(&mut cols[0], profile, ModeSlot::S1, active_hw_mode);
                self.render_all_modes_card(&mut cols[1], profile);
            });
        }
    }

    fn render_slot_card(
        &mut self,
        ui: &mut Ui,
        profile: &ProgrammingProfile,
        slot: ModeSlot,
        active_hw_mode: ModeSlot,
    ) {
        let scope = RgbModeScope::from_slot(slot);
        let is_selected = self.selected_mode == scope;
        let is_hw_active = active_hw_mode == slot;

        let stick_hex = self.effective_hex_for(profile, slot, DeviceKind::Stick);
        let thr_hex = self.effective_hex_for(profile, slot, DeviceKind::Throttle);
        let stick_col = Self::hex_to_color32(&stick_hex);
        let thr_col = Self::hex_to_color32(&thr_hex);

        let fallback_desc = slot.localized_description();
        let mode_desc = profile
            .bindings_for(slot)
            .name
            .as_deref()
            .unwrap_or(&fallback_desc);

        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::click());
        let resp = resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(x56_core::t!(
                "rgb-mode-card-tooltip",
                "mode" => slot.as_str()
            ));

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let (fill, stroke) = if is_selected {
                (
                    theme::COLOR_SURFACE_HIGH,
                    Stroke::new(
                        1.0,
                        if resp.hovered() {
                            theme::COLOR_ACCENT
                        } else {
                            theme::COLOR_ACCENT_BORDER
                        },
                    ),
                )
            } else if resp.hovered() {
                (
                    theme::COLOR_SURFACE_HIGH,
                    Stroke::new(1.0, theme::COLOR_BORDER_STRONG),
                )
            } else {
                (
                    theme::COLOR_SURFACE_CONTAINER,
                    Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                )
            };

            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), fill);
            painter.rect_stroke(
                rect,
                CornerRadius::same(theme::RADIUS_CONTROL),
                stroke,
                egui::StrokeKind::Inside,
            );
            if is_selected {
                painter.hline(
                    (rect.min.x + theme::RADIUS_CONTROL as f32)
                        ..=(rect.max.x - theme::RADIUS_CONTROL as f32),
                    rect.min.y + 1.0,
                    Stroke::new(1.0, theme::COLOR_TOP_HIGHLIGHT),
                );
            }

            let pad = theme::SPACE_SM;
            let swatch_size = 28.0;
            let swatch_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + pad, rect.center().y - swatch_size * 0.5),
                Vec2::splat(swatch_size),
            );

            if stick_hex.eq_ignore_ascii_case(&thr_hex) {
                painter.rect_filled(
                    swatch_rect,
                    CornerRadius::same(theme::RADIUS_KEYCAP),
                    stick_col,
                );
            } else {
                let mid_x = swatch_rect.center().x;
                let left_half =
                    egui::Rect::from_min_max(swatch_rect.min, egui::pos2(mid_x, swatch_rect.max.y));
                let right_half =
                    egui::Rect::from_min_max(egui::pos2(mid_x, swatch_rect.min.y), swatch_rect.max);
                painter.rect_filled(
                    left_half,
                    CornerRadius {
                        nw: theme::RADIUS_KEYCAP,
                        sw: theme::RADIUS_KEYCAP,
                        ne: 0,
                        se: 0,
                    },
                    stick_col,
                );
                painter.rect_filled(
                    right_half,
                    CornerRadius {
                        nw: 0,
                        sw: 0,
                        ne: theme::RADIUS_KEYCAP,
                        se: theme::RADIUS_KEYCAP,
                    },
                    thr_col,
                );
            }
            painter.rect_stroke(
                swatch_rect,
                CornerRadius::same(theme::RADIUS_KEYCAP),
                Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                egui::StrokeKind::Inside,
            );

            let text_left = swatch_rect.max.x + theme::SPACE_SM;
            let text_right = rect.max.x - pad;

            let title_galley = painter.layout_no_wrap(
                slot.as_str().to_string(),
                FontId::proportional(theme::FONT_BODY),
                theme::COLOR_TEXT_PRIMARY,
            );
            let title_pos = egui::pos2(text_left, rect.min.y + pad);
            painter.galley(title_pos, title_galley.clone(), theme::COLOR_TEXT_PRIMARY);

            if is_hw_active {
                let hw_galley = painter.layout_no_wrap(
                    "HW".to_string(),
                    FontId::monospace(theme::FONT_META),
                    theme::COLOR_POSITIVE,
                );
                let hw_pos = egui::pos2(text_right - hw_galley.size().x, rect.min.y + pad + 1.0);
                painter.galley(hw_pos, hw_galley, theme::COLOR_POSITIVE);
            } else {
                let desc_max_w =
                    (text_right - (title_pos.x + title_galley.size().x + 6.0)).max(0.0);
                if desc_max_w > 24.0 {
                    let desc_galley = painter.layout(
                        mode_desc.to_string(),
                        FontId::proportional(theme::FONT_META),
                        theme::COLOR_TEXT_SECONDARY,
                        desc_max_w,
                    );
                    let desc_pos = egui::pos2(
                        title_pos.x + title_galley.size().x + 6.0,
                        rect.min.y + pad + 2.0,
                    );
                    painter.galley(desc_pos, desc_galley, theme::COLOR_TEXT_SECONDARY);
                }
            }

            let hex_summary = if stick_hex.eq_ignore_ascii_case(&thr_hex) {
                stick_hex.to_uppercase()
            } else {
                format!("{} · {}", stick_hex.to_uppercase(), thr_hex.to_uppercase())
            };
            let hex_galley = painter.layout_no_wrap(
                hex_summary,
                FontId::monospace(theme::FONT_META),
                theme::COLOR_TEXT_MUTED,
            );
            let hex_pos = egui::pos2(text_left, rect.max.y - pad - hex_galley.size().y);
            painter.galley(hex_pos, hex_galley, theme::COLOR_TEXT_MUTED);
        }

        if resp.clicked() {
            self.selected_mode = scope;
            self.follow_hardware_mode = false;
            self.load_picker_from_selection(profile);
        }
    }

    fn render_all_modes_card(&mut self, ui: &mut Ui, profile: &ProgrammingProfile) {
        let is_selected = self.selected_mode == RgbModeScope::AllModes;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::click());
        let resp = resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(x56_core::t!("rgb-all-modes-tooltip"));

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let (fill, stroke) = if is_selected {
                (
                    theme::COLOR_SURFACE_HIGH,
                    Stroke::new(
                        1.0,
                        if resp.hovered() {
                            theme::COLOR_ACCENT
                        } else {
                            theme::COLOR_ACCENT_BORDER
                        },
                    ),
                )
            } else if resp.hovered() {
                (
                    theme::COLOR_SURFACE_HIGH,
                    Stroke::new(1.0, theme::COLOR_BORDER_STRONG),
                )
            } else {
                (
                    theme::COLOR_SURFACE_CONTAINER,
                    Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                )
            };

            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), fill);
            painter.rect_stroke(
                rect,
                CornerRadius::same(theme::RADIUS_CONTROL),
                stroke,
                egui::StrokeKind::Inside,
            );
            if is_selected {
                painter.hline(
                    (rect.min.x + theme::RADIUS_CONTROL as f32)
                        ..=(rect.max.x - theme::RADIUS_CONTROL as f32),
                    rect.min.y + 1.0,
                    Stroke::new(1.0, theme::COLOR_TOP_HIGHLIGHT),
                );
            }

            let pad = theme::SPACE_SM;
            let swatch_size = 28.0;
            let swatch_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + pad, rect.center().y - swatch_size * 0.5),
                Vec2::splat(swatch_size),
            );
            let c = self.current_rgb_color();
            painter.rect_filled(
                swatch_rect,
                CornerRadius::same(theme::RADIUS_KEYCAP),
                Color32::from_rgb(c.r, c.g, c.b),
            );
            painter.rect_stroke(
                swatch_rect,
                CornerRadius::same(theme::RADIUS_KEYCAP),
                Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                egui::StrokeKind::Inside,
            );

            let text_left = swatch_rect.max.x + theme::SPACE_SM;
            let title_galley = painter.layout_no_wrap(
                x56_core::t!("rgb-mode-all-short"),
                FontId::proportional(theme::FONT_BODY),
                theme::COLOR_TEXT_PRIMARY,
            );
            painter.galley(
                egui::pos2(text_left, rect.min.y + pad),
                title_galley,
                theme::COLOR_TEXT_PRIMARY,
            );

            let sub_galley = painter.layout_no_wrap(
                x56_core::t!("rgb-mode-global"),
                FontId::proportional(theme::FONT_META),
                theme::COLOR_TEXT_MUTED,
            );
            painter.galley(
                egui::pos2(text_left, rect.max.y - pad - sub_galley.size().y),
                sub_galley,
                theme::COLOR_TEXT_MUTED,
            );
        }

        if resp.clicked() {
            self.selected_mode = RgbModeScope::AllModes;
            self.follow_hardware_mode = false;
            self.load_picker_from_selection(profile);
        }
    }

    fn render_color_picker(
        &mut self,
        ui: &mut Ui,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) -> bool {
        let mut save_requested = false;

        let mode_label = self.selected_mode.label();
        theme::section_heading(
            ui,
            &x56_core::t!("rgb-color-picker-heading", "mode" => &mode_label),
        );
        ui.add_space(theme::SPACE_MD);

        let rgb = self.current_rgb_color();
        let mut srgb = [rgb.r, rgb.g, rgb.b];
        let mut color_changed = false;

        ui.horizontal(|ui| {
            let picker_resp = ui
                .scope(|ui| {
                    ui.spacing_mut().interact_size = Vec2::new(36.0, 32.0);
                    ui.color_edit_button_srgb(&mut srgb)
                })
                .inner;
            if picker_resp.changed() {
                self.color_rgba = [
                    (srgb[0] as f32) / 255.0,
                    (srgb[1] as f32) / 255.0,
                    (srgb[2] as f32) / 255.0,
                ];
                color_changed = true;
            }

            ui.add_space(theme::SPACE_XS);
            ui.vertical(|ui| {
                let hex = self.current_hex().to_uppercase();
                ui.label(
                    RichText::new(hex)
                        .color(theme::COLOR_TEXT_PRIMARY)
                        .size(theme::FONT_BODY)
                        .monospace(),
                );
                ui.label(
                    RichText::new(x56_core::t!("rgb-color-preview"))
                        .color(theme::COLOR_TEXT_MUTED)
                        .size(theme::FONT_META),
                );
            });
        });

        ui.add_space(theme::SPACE_LG);

        let rgb = self.current_rgb_color();
        let mut r_u8 = rgb.r;
        let mut g_u8 = rgb.g;
        let mut b_u8 = rgb.b;

        let r_val = format!("{:3}", r_u8);
        ui.label(
            RichText::new(x56_core::t!("rgb-red", "val" => &r_val))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );
        ui.add_space(theme::SPACE_XS);
        let r_resp = ui.add(egui::Slider::new(&mut r_u8, 0..=255).show_value(false));
        if r_resp.changed() {
            self.color_rgba[0] = (r_u8 as f32) / 255.0;
        }
        ui.add_space(theme::SPACE_MD);

        let g_val = format!("{:3}", g_u8);
        ui.label(
            RichText::new(x56_core::t!("rgb-green", "val" => &g_val))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );
        ui.add_space(theme::SPACE_XS);
        let g_resp = ui.add(egui::Slider::new(&mut g_u8, 0..=255).show_value(false));
        if g_resp.changed() {
            self.color_rgba[1] = (g_u8 as f32) / 255.0;
        }
        ui.add_space(theme::SPACE_MD);

        let b_val = format!("{:3}", b_u8);
        ui.label(
            RichText::new(x56_core::t!("rgb-blue", "val" => &b_val))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );
        ui.add_space(theme::SPACE_XS);
        let b_resp = ui.add(egui::Slider::new(&mut b_u8, 0..=255).show_value(false));
        if b_resp.changed() {
            self.color_rgba[2] = (b_u8 as f32) / 255.0;
        }

        if color_changed || r_resp.changed() || g_resp.changed() || b_resp.changed() {
            self.update_profile_color(profile, monitor);
        }
        if r_resp.drag_stopped() || g_resp.drag_stopped() || b_resp.drag_stopped() {
            self.apply_color(profile, monitor);
        }

        ui.add_space(theme::SPACE_LG);

        let mode_short = self.selected_mode.localized_short_label();
        ui.horizontal_wrapped(|ui| {
            let apply_lbl = x56_core::t!("rgb-apply-to", "mode" => &mode_short);
            if theme::primary_button(ui, &apply_lbl)
                .on_hover_text(x56_core::t!("rgb-apply-tooltip"))
                .clicked()
            {
                self.apply_color(profile, monitor);
            }

            let save_lbl = x56_core::t!("rgb-save-to-toml");
            if theme::secondary_button(ui, &save_lbl)
                .on_hover_text(x56_core::t!("rgb-save-tooltip"))
                .clicked()
            {
                self.apply_color(profile, monitor);
                save_requested = true;
            }
        });

        save_requested
    }

    fn render_presets(
        &mut self,
        ui: &mut Ui,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) {
        let mode_short = self.selected_mode.localized_short_label();
        theme::section_heading(
            ui,
            &x56_core::t!("rgb-presets-heading", "mode" => &mode_short),
        );
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(x56_core::t!("rgb-presets-desc"))
                .color(theme::COLOR_TEXT_SECONDARY)
                .size(theme::FONT_SMALL),
        );

        ui.add_space(theme::SPACE_MD);

        let current_rgb = self.current_rgb_color();
        let presets: [(String, u8, u8, u8); 8] = [
            (x56_core::t!("rgb-preset-cyan-rhino"), 0x00, 0xff, 0xff),
            (x56_core::t!("rgb-preset-tactical-amber"), 0xff, 0x66, 0x00),
            (x56_core::t!("rgb-preset-night-combat"), 0xff, 0x11, 0x00),
            (x56_core::t!("rgb-preset-supercruise"), 0x9d, 0x4e, 0xdd),
            (x56_core::t!("rgb-preset-hud-green"), 0x00, 0xff, 0x44),
            (x56_core::t!("rgb-preset-ops-blue"), 0x00, 0x88, 0xff),
            (x56_core::t!("rgb-preset-pure-white"), 0xff, 0xff, 0xff),
            (x56_core::t!("rgb-preset-off"), 0x00, 0x00, 0x00),
        ];

        for (name, r, g, b) in presets {
            let is_match = current_rgb.r == r && current_rgb.g == g && current_rgb.b == b;
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let (fill, stroke) = if is_match {
                    (
                        theme::COLOR_ACCENT_SOFT,
                        Stroke::new(1.0, theme::COLOR_ACCENT_BORDER),
                    )
                } else if resp.hovered() {
                    (
                        theme::COLOR_SURFACE_HIGH,
                        Stroke::new(1.0, theme::COLOR_BORDER_STRONG),
                    )
                } else {
                    (
                        theme::COLOR_SURFACE_LOW,
                        Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                    )
                };

                painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), fill);
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(theme::RADIUS_CONTROL),
                    stroke,
                    egui::StrokeKind::Inside,
                );

                let pad = theme::SPACE_SM;
                let swatch_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + pad, rect.center().y - 10.0),
                    Vec2::splat(20.0),
                );
                painter.rect_filled(
                    swatch_rect,
                    CornerRadius::same(theme::RADIUS_KEYCAP),
                    Color32::from_rgb(r, g, b),
                );
                painter.rect_stroke(
                    swatch_rect,
                    CornerRadius::same(theme::RADIUS_KEYCAP),
                    Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                    egui::StrokeKind::Inside,
                );

                let name_galley = painter.layout_no_wrap(
                    name,
                    FontId::proportional(theme::FONT_SMALL),
                    theme::COLOR_TEXT_PRIMARY,
                );
                painter.galley(
                    egui::pos2(
                        swatch_rect.max.x + theme::SPACE_SM,
                        rect.center().y - name_galley.size().y * 0.5,
                    ),
                    name_galley,
                    theme::COLOR_TEXT_PRIMARY,
                );

                let hex_text = format!("#{:02X}{:02X}{:02X}", r, g, b);
                let hex_galley = painter.layout_no_wrap(
                    hex_text,
                    FontId::monospace(theme::FONT_META),
                    theme::COLOR_TEXT_MUTED,
                );
                let right_edge = if is_match {
                    rect.max.x - pad - 14.0
                } else {
                    rect.max.x - pad
                };
                painter.galley(
                    egui::pos2(
                        right_edge - hex_galley.size().x,
                        rect.center().y - hex_galley.size().y * 0.5,
                    ),
                    hex_galley,
                    theme::COLOR_TEXT_MUTED,
                );

                if is_match {
                    painter.circle_filled(
                        egui::pos2(rect.max.x - pad - 4.0, rect.center().y),
                        3.5,
                        theme::COLOR_ACCENT,
                    );
                }
            }

            if resp.clicked() {
                self.set_preset(r, g, b, profile, monitor);
            }
            ui.add_space(theme::SPACE_XS);
        }

        ui.add_space(theme::SPACE_SM);
        ui.separator();
        ui.add_space(theme::SPACE_SM);

        ui.label(
            RichText::new(x56_core::t!("rgb-quick-schemes"))
                .color(theme::COLOR_TEXT_MUTED)
                .size(theme::FONT_META),
        );
        ui.add_space(theme::SPACE_SM);

        let schemes = [
            (
                x56_core::t!("rgb-scheme-tactical"),
                x56_core::t!("rgb-scheme-tactical-tooltip"),
                ["#9d4edd", "#ff1100", "#0088ff"],
            ),
            (
                x56_core::t!("rgb-scheme-cockpit"),
                x56_core::t!("rgb-scheme-cockpit-tooltip"),
                ["#00ff44", "#ff6600", "#00ffff"],
            ),
        ];

        for (label, tooltip, hexes) in schemes {
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(tooltip);

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let (fill, stroke) = if resp.hovered() {
                    (
                        theme::COLOR_SURFACE_HIGH,
                        Stroke::new(1.0, theme::COLOR_BORDER_STRONG),
                    )
                } else {
                    (
                        theme::COLOR_SURFACE_LOW,
                        Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                    )
                };
                painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), fill);
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(theme::RADIUS_CONTROL),
                    stroke,
                    egui::StrokeKind::Inside,
                );

                let pad = theme::SPACE_SM;
                let mut x = rect.min.x + pad;
                for hex in hexes {
                    let sq = egui::Rect::from_min_size(
                        egui::pos2(x, rect.center().y - 5.0),
                        Vec2::splat(10.0),
                    );
                    painter.rect_filled(sq, CornerRadius::same(2), Self::hex_to_color32(hex));
                    painter.rect_stroke(
                        sq,
                        CornerRadius::same(2),
                        Stroke::new(1.0, theme::COLOR_BORDER_HAIRLINE),
                        egui::StrokeKind::Inside,
                    );
                    x += 10.0 + theme::SPACE_XS;
                }

                let label_x = x + theme::SPACE_XS;
                let max_w = (rect.max.x - pad - label_x).max(0.0);
                let galley = painter.layout(
                    label,
                    FontId::proportional(theme::FONT_SMALL),
                    theme::COLOR_TEXT_PRIMARY,
                    max_w,
                );
                painter.galley(
                    egui::pos2(label_x, rect.center().y - galley.size().y * 0.5),
                    galley,
                    theme::COLOR_TEXT_PRIMARY,
                );
            }

            if resp.clicked() {
                self.apply_tactical_tricolor_scheme(hexes[0], hexes[1], hexes[2], profile, monitor);
            }
            ui.add_space(theme::SPACE_XS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_mode_rgb_selection_and_update() {
        let mut tab = RgbTab::default();
        let mut profile = ProgrammingProfile::default();
        let monitor = ButtonMonitor::new();

        tab.selected_mode = RgbModeScope::M1;
        tab.target = TargetDevice::All;
        tab.color_rgba = [
            0x9d as f32 / 255.0,
            0x4e as f32 / 255.0,
            0xdd as f32 / 255.0,
        ];
        tab.update_profile_color(&mut profile, &monitor);

        tab.selected_mode = RgbModeScope::M2;
        tab.color_rgba = [1.0, 0x11 as f32 / 255.0, 0.0];
        tab.update_profile_color(&mut profile, &monitor);

        tab.selected_mode = RgbModeScope::S1;
        tab.target = TargetDevice::Stick;
        tab.color_rgba = [0.0, 0x88 as f32 / 255.0, 1.0];
        tab.update_profile_color(&mut profile, &monitor);

        tab.target = TargetDevice::Throttle;
        tab.color_rgba = [1.0, 0x66 as f32 / 255.0, 0.0];
        tab.update_profile_color(&mut profile, &monitor);

        assert_eq!(
            profile.rgb_for_mode(ModeSlot::M1, DeviceKind::Stick),
            Some("#9d4edd")
        );
        assert_eq!(
            profile.rgb_for_mode(ModeSlot::M2, DeviceKind::Throttle),
            Some("#ff1100")
        );
        assert_eq!(
            profile.rgb_for_mode(ModeSlot::S1, DeviceKind::Stick),
            Some("#0088ff")
        );
        assert_eq!(
            profile.rgb_for_mode(ModeSlot::S1, DeviceKind::Throttle),
            Some("#ff6600")
        );

        tab.selected_mode = RgbModeScope::M2;
        tab.target = TargetDevice::All;
        tab.load_picker_from_selection(&profile);
        assert_eq!(tab.current_hex(), "#ff1100");
    }

    #[test]
    fn test_rgb_mode_scope_localized_labels() {
        assert!(!RgbModeScope::M1.label().is_empty());
        assert!(!RgbModeScope::M2.label().is_empty());
        assert!(!RgbModeScope::S1.label().is_empty());
        assert!(!RgbModeScope::AllModes.label().is_empty());
    }

    #[test]
    fn test_rgb_subcards_do_not_grow_across_frames() {
        let ctx = egui::Context::default();
        let mut tab = RgbTab::default();
        let mut profile = ProgrammingProfile::default();
        let monitor = ButtonMonitor::new();
        let mut first_h = None;
        for _ in 0..5 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                ..Default::default()
            };
            let mut card_h = 0.0;
            let mut out = ctx.run_ui(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let r = crate::theme::card_frame().show(ui, |ui| {
                                tab.ui(ui, &mut profile, &monitor);
                            });
                            card_h = r.response.rect.height();
                        });
                });
            });
            out.textures_delta.clear();
            let baseline = *first_h.get_or_insert(card_h);
            assert!(
                (card_h - baseline).abs() < 1.0,
                "RGB card height drifted from {baseline} to {card_h}"
            );
        }
    }
}
