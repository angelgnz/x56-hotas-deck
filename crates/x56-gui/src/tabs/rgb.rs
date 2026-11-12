//! Pestaña de control interactivo de iluminación LED RGB por modo (M1, M2, S1) y preajustes.

use eframe::egui::{self, Color32, RichText, Stroke, Ui};
use x56_core::{
    parse_color, ButtonMonitor, DeviceKind, FullProfile, ModeBindings, ModeSlot,
    ProgrammingProfile, RgbColor, TargetDevice, X56Manager,
};

use crate::icons;
use crate::theme::{
    self, COLOR_ON_SURFACE, COLOR_ON_SURFACE_VARIANT, COLOR_OUTLINE, COLOR_OUTLINE_VARIANT,
    COLOR_PRIMARY, COLOR_PRIMARY_CONTAINER, COLOR_SURFACE_HIGH, COLOR_SURFACE_LOW,
    COLOR_TERTIARY_CONTAINER,
};

/// Alcance de modo para la selección y guardado de iluminación RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RgbModeScope {
    /// Configura el color específico del Modo 1 (M1 - Navegación / Vuelo).
    #[default]
    M1,
    /// Configura el color específico del Modo 2 (M2 - Combate / Ataque).
    M2,
    /// Configura el color específico del Modo S1 (S1 - Operaciones / Aterrizaje).
    S1,
    /// Configura el mismo color en todos los modos (M1, M2, S1) y como color base del perfil.
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

    pub fn short_label(self) -> &'static str {
        match self {
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::S1 => "S1",
            Self::AllModes => "TODOS",
        }
    }
}

pub struct RgbTab {
    pub selected_mode: RgbModeScope,
    pub follow_hardware_mode: bool,
    pub last_seen_hw_mode: Option<ModeSlot>,
    pub target: TargetDevice,
    pub color_rgba: [f32; 3], // 0.0 .. 1.0 para egui color picker
    pub fallback_stick_hex: String,
    pub fallback_throttle_hex: String,
    pub status_message: Option<(String, bool)>,
}

impl Default for RgbTab {
    fn default() -> Self {
        Self {
            selected_mode: RgbModeScope::M1,
            follow_hardware_mode: false,
            last_seen_hw_mode: None,
            target: TargetDevice::All,
            color_rgba: [0.0, 1.0, 1.0], // Cyan de fábrica
            fallback_stick_hex: "#00ffff".to_string(),
            fallback_throttle_hex: "#00ffff".to_string(),
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

    pub fn current_hex(&self) -> String {
        let c = self.current_rgb_color();
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// Resuelve el color hexadecimal efectivo para un modo y dispositivo dado.
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

    /// Convierte una cadena hex (`#RRGGBB`) a `Color32` para previsualización en la interfaz.
    pub fn hex_to_color32(hex: &str) -> Color32 {
        parse_color(hex)
            .map(|c| Color32::from_rgb(c.r, c.g, c.b))
            .unwrap_or(Color32::from_rgb(0, 255, 255))
    }

    /// Carga en el selector (`color_rgba`) el color configurado para el modo y dispositivo activos.
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

    /// Sincroniza el estado de la pestaña RGB al cargar un perfil completo desde disco.
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

    /// Garantiza que todos los modos (`M1`, `M2`, `S1`) tengan colores explícitos en el perfil
    /// antes de guardarlo a disco o enviarlo al monitor.
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

    /// Actualiza el color del modo seleccionado en `ProgrammingProfile` y sincroniza `ButtonMonitor`.
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
                // Si se edita M1, actualizamos también el color de reserva base para coherencia en TOML
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

    /// Aplica el color actual tanto al perfil del modo seleccionado como al hardware físico USB.
    pub fn apply_color(&mut self, profile: &mut ProgrammingProfile, monitor: &ButtonMonitor) {
        self.update_profile_color(profile, monitor);
        let color = self.current_rgb_color();
        let target = self.target;
        let hex = self.current_hex();
        let hex = hex.as_str();
        let mode_lbl = self.selected_mode.short_label();
        let target_desc = match target {
            TargetDevice::All => "Ambos",
            TargetDevice::Stick => "Stick",
            TargetDevice::Throttle => "Throttle",
        };

        match X56Manager::set_rgb(target, color) {
            Ok(count) => {
                let count = count.to_string();
                let count = count.as_str();
                self.status_message = Some((
                    x56_core::t!(
                        "rgb-status-applied",
                        "hex" => hex,
                        "mode" => mode_lbl,
                        "target" => target_desc,
                        "count" => count
                    ),
                    false,
                ));
            }
            Err(err) => {
                let err_str = err.to_string();
                let err_str = err_str.as_str();
                self.status_message = Some((
                    x56_core::t!(
                        "rgb-status-saved",
                        "hex" => hex,
                        "mode" => mode_lbl,
                        "target" => target_desc,
                        "err" => err_str
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

    /// Aplica un esquema táctico tricolor diferenciado por modo (M1, M2, S1).
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

        // Enviar al hardware el color del modo físico activo
        let active_hw = monitor.snapshot().active_mode;
        let active_hex = self.effective_hex_for(profile, active_hw, DeviceKind::Stick);
        if let Ok(col) = parse_color(&active_hex) {
            let _ = X56Manager::set_rgb(TargetDevice::All, col);
        }

        self.status_message = Some((
            format!("Esquema por modo asignado: M1={m1_hex} | M2={m2_hex} | S1={s1_hex}"),
            false,
        ));
    }

    /// Renderiza la interfaz de iluminación RGB por modo.
    /// Devuelve `true` si el usuario solicitó guardar los cambios en el archivo de perfil TOML.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) -> bool {
        let mut save_requested = false;
        let snap = monitor.snapshot();

        // Sincronizar con el selector físico si el usuario activó el seguimiento automático
        if self.follow_hardware_mode && self.last_seen_hw_mode != Some(snap.active_mode) {
            self.last_seen_hw_mode = Some(snap.active_mode);
            self.selected_mode = RgbModeScope::from_slot(snap.active_mode);
            self.load_picker_from_selection(profile);
        } else {
            self.last_seen_hw_mode = Some(snap.active_mode);
        }

        // Cabecera principal Avionics Vector
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{} {}", icons::PALETTE, x56_core::t!("rgb-title")))
                    .color(COLOR_PRIMARY)
                    .size(14.0)
                    .strong()
                    .monospace(),
            );

            ui.separator();

            theme::telemetry_badge(
                ui,
                &x56_core::t!("rgb-active-hw-mode"),
                snap.active_mode.as_str(),
                COLOR_PRIMARY_CONTAINER,
            );

            if ui
                .checkbox(
                    &mut self.follow_hardware_mode,
                    RichText::new(x56_core::t!("rgb-follow-hw"))
                        .color(COLOR_ON_SURFACE_VARIANT)
                        .size(11.0),
                )
                .on_hover_text(x56_core::t!("rgb-follow-hw-tooltip"))
                .changed()
                && self.follow_hardware_mode
            {
                self.selected_mode = RgbModeScope::from_slot(snap.active_mode);
                self.load_picker_from_selection(profile);
            }
        });

        ui.label(
            RichText::new(x56_core::t!("rgb-desc"))
                .color(COLOR_ON_SURFACE_VARIANT)
                .size(11.5),
        );

        ui.add_space(10.0);

        // Tarjetas selectoras de Modo (M1, M2, S1 y Global) con vista previa de color en vivo
        self.render_mode_selector_bar(ui, profile, snap.active_mode);

        ui.add_space(10.0);

        // Selector de Dispositivo Destino
        theme::sunken_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(x56_core::t!("rgb-target-device"))
                        .color(COLOR_OUTLINE)
                        .size(11.0)
                        .strong()
                        .monospace(),
                );

                for (dev, lbl) in [
                    (TargetDevice::All, x56_core::t!("rgb-target-both")),
                    (TargetDevice::Stick, x56_core::t!("rgb-target-stick")),
                    (TargetDevice::Throttle, x56_core::t!("rgb-target-throttle")),
                ] {
                    let active = self.target == dev;
                    if theme::tactical_button(ui, &lbl, active, false).clicked() {
                        self.target = dev;
                        self.load_picker_from_selection(profile);
                    }
                }
            });
        });

        ui.add_space(8.0);

        if let Some((ref msg, is_err)) = self.status_message {
            let col = if is_err {
                Color32::from_rgb(255, 110, 90)
            } else {
                COLOR_TERTIARY_CONTAINER
            };
            ui.colored_label(col, RichText::new(msg).monospace().size(11.5));
            ui.add_space(6.0);
        }

        let available_w = ui.available_width();
        if available_w >= 580.0 {
            ui.columns(2, |cols| {
                theme::sunken_frame().show(&mut cols[0], |ui| {
                    if self.render_color_picker(ui, profile, monitor) {
                        save_requested = true;
                    }
                });
                theme::sunken_frame().show(&mut cols[1], |ui| {
                    self.render_presets(ui, profile, monitor);
                });
            });
        } else {
            theme::sunken_frame().show(ui, |ui| {
                if self.render_color_picker(ui, profile, monitor) {
                    save_requested = true;
                }
            });
            ui.add_space(8.0);
            theme::sunken_frame().show(ui, |ui| {
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
        ui.horizontal_wrapped(|ui| {
            for slot in ModeSlot::ALL {
                let scope = RgbModeScope::from_slot(slot);
                let is_selected = self.selected_mode == scope;
                let is_hw_active = active_hw_mode == slot;

                let stick_hex = self.effective_hex_for(profile, slot, DeviceKind::Stick);
                let thr_hex = self.effective_hex_for(profile, slot, DeviceKind::Throttle);
                let stick_col = Self::hex_to_color32(&stick_hex);
                let thr_col = Self::hex_to_color32(&thr_hex);

                let mode_desc = profile
                    .bindings_for(slot)
                    .name
                    .as_deref()
                    .unwrap_or(match slot {
                        ModeSlot::M1 => "Vuelo / Navegación",
                        ModeSlot::M2 => "Combate / Ataque",
                        ModeSlot::S1 => "Operaciones / S1",
                    });

                let bg_fill = if is_selected {
                    COLOR_SURFACE_HIGH
                } else {
                    COLOR_SURFACE_LOW
                };
                let border_stroke = if is_selected {
                    Stroke::new(1.6, COLOR_PRIMARY)
                } else if is_hw_active {
                    Stroke::new(1.0, COLOR_TERTIARY_CONTAINER)
                } else {
                    Stroke::new(1.0, COLOR_OUTLINE_VARIANT)
                };

                egui::Frame::new()
                    .fill(bg_fill)
                    .stroke(border_stroke)
                    .corner_radius(3)
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Swatches de color en vivo (Stick y Throttle)
                            let (swatch_rect, _) = ui
                                .allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                            if stick_hex.eq_ignore_ascii_case(&thr_hex) {
                                ui.painter().rect_filled(swatch_rect, 4.0, stick_col);
                            } else {
                                let left_half = egui::Rect::from_min_max(
                                    swatch_rect.min,
                                    egui::pos2(swatch_rect.center().x, swatch_rect.max.y),
                                );
                                let right_half = egui::Rect::from_min_max(
                                    egui::pos2(swatch_rect.center().x, swatch_rect.min.y),
                                    swatch_rect.max,
                                );
                                ui.painter().rect_filled(left_half, 3.0, stick_col);
                                ui.painter().rect_filled(right_half, 3.0, thr_col);
                            }
                            ui.painter().rect_stroke(
                                swatch_rect,
                                4.0,
                                Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                                egui::StrokeKind::Inside,
                            );

                            let btn_title = if is_hw_active {
                                format!("{} [HW] — {}", slot.as_str(), mode_desc)
                            } else {
                                format!("{} — {}", slot.as_str(), mode_desc)
                            };

                            let hex_summary = if stick_hex.eq_ignore_ascii_case(&thr_hex) {
                                stick_hex.to_uppercase()
                            } else {
                                format!(
                                    "S:{} / T:{}",
                                    stick_hex.to_uppercase(),
                                    thr_hex.to_uppercase()
                                )
                            };

                            let btn_label = format!("{btn_title}\n{hex_summary}");
                            if theme::tactical_button(ui, &btn_label, is_selected, false)
                                .on_hover_text(format!(
                                    "Seleccionar {} para configurar su color LED RGB",
                                    slot.description()
                                ))
                                .clicked()
                            {
                                self.selected_mode = scope;
                                self.follow_hardware_mode = false;
                                self.load_picker_from_selection(profile);
                            }
                        });
                    });
            }

            // Botón para editar todos los modos a la vez
            let all_selected = self.selected_mode == RgbModeScope::AllModes;
            if theme::tactical_button(ui, &x56_core::t!("rgb-all-modes-btn"), all_selected, false)
                .on_hover_text(x56_core::t!("rgb-all-modes-tooltip"))
                .clicked()
            {
                self.selected_mode = RgbModeScope::AllModes;
                self.follow_hardware_mode = false;
                self.load_picker_from_selection(profile);
            }
        });
    }

    fn render_color_picker(
        &mut self,
        ui: &mut Ui,
        profile: &mut ProgrammingProfile,
        monitor: &ButtonMonitor,
    ) -> bool {
        let mut save_requested = false;

        let mode_label = self.selected_mode.label();
        ui.label(
            RichText::new(x56_core::t!(
                "rgb-color-picker-heading",
                "mode" => &mode_label
            ))
            .color(COLOR_PRIMARY)
            .size(12.5)
            .strong()
            .monospace(),
        );
        ui.add_space(6.0);

        let mut col32 = Color32::from_rgb(
            (self.color_rgba[0] * 255.0).round() as u8,
            (self.color_rgba[1] * 255.0).round() as u8,
            (self.color_rgba[2] * 255.0).round() as u8,
        );

        let mut color_changed = false;
        ui.horizontal(|ui| {
            if ui.color_edit_button_srgba(&mut col32).changed() {
                self.color_rgba = [
                    (col32.r() as f32) / 255.0,
                    (col32.g() as f32) / 255.0,
                    (col32.b() as f32) / 255.0,
                ];
                color_changed = true;
            }
            let hex = self.current_hex().to_uppercase();
            ui.label(
                RichText::new(format!("HEX: {hex}"))
                    .color(COLOR_ON_SURFACE)
                    .size(12.0)
                    .strong()
                    .monospace(),
            );
        });

        ui.add_space(8.0);
        let rgb = self.current_rgb_color();

        ui.label(
            RichText::new(x56_core::t!(
                "rgb-red",
                "val" => &format!("{:3}", rgb.r)
            ))
            .color(COLOR_ON_SURFACE_VARIANT)
            .size(11.0)
            .monospace(),
        );
        let r_resp = ui.add(egui::Slider::new(&mut self.color_rgba[0], 0.0..=1.0).text("R"));

        ui.label(
            RichText::new(x56_core::t!(
                "rgb-green",
                "val" => &format!("{:3}", rgb.g)
            ))
            .color(COLOR_ON_SURFACE_VARIANT)
            .size(11.0)
            .monospace(),
        );
        let g_resp = ui.add(egui::Slider::new(&mut self.color_rgba[1], 0.0..=1.0).text("G"));

        ui.label(
            RichText::new(x56_core::t!(
                "rgb-blue",
                "val" => &format!("{:3}", rgb.b)
            ))
            .color(COLOR_ON_SURFACE_VARIANT)
            .size(11.0)
            .monospace(),
        );
        let b_resp = ui.add(egui::Slider::new(&mut self.color_rgba[2], 0.0..=1.0).text("B"));

        if color_changed || r_resp.changed() || g_resp.changed() || b_resp.changed() {
            self.update_profile_color(profile, monitor);
        }
        if r_resp.drag_stopped() || g_resp.drag_stopped() || b_resp.drag_stopped() {
            self.apply_color(profile, monitor);
        }

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            let apply_lbl = format!(
                "{} {}",
                icons::PALETTE,
                x56_core::t!("rgb-apply-to", "mode" => self.selected_mode.short_label())
            );
            if theme::tactical_button(ui, &apply_lbl, false, true)
                .on_hover_text(x56_core::t!("rgb-apply-tooltip"))
                .clicked()
            {
                self.apply_color(profile, monitor);
            }

            let save_lbl = format!("{} {}", icons::SAVE, x56_core::t!("rgb-save-to-toml"));
            if theme::tactical_button(ui, &save_lbl, false, false)
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
        ui.label(
            RichText::new(x56_core::t!(
                "rgb-presets-heading",
                "mode" => self.selected_mode.short_label()
            ))
            .color(COLOR_PRIMARY)
            .size(12.5)
            .strong()
            .monospace(),
        );
        ui.label(
            RichText::new(x56_core::t!("rgb-presets-desc"))
                .color(COLOR_ON_SURFACE_VARIANT)
                .size(11.0),
        );

        ui.add_space(6.0);

        let presets = [
            (
                x56_core::t!("rgb-preset-cyan-rhino"),
                0x00,
                0xff,
                0xff,
                Color32::from_rgb(0, 255, 255),
            ),
            (
                x56_core::t!("rgb-preset-tactical-amber"),
                0xff,
                0x66,
                0x00,
                Color32::from_rgb(255, 102, 0),
            ),
            (
                x56_core::t!("rgb-preset-night-combat"),
                0xff,
                0x11,
                0x00,
                Color32::from_rgb(255, 17, 0),
            ),
            (
                x56_core::t!("rgb-preset-supercruise"),
                0x9d,
                0x4e,
                0xdd,
                Color32::from_rgb(157, 78, 221),
            ),
            (
                x56_core::t!("rgb-preset-hud-green"),
                0x00,
                0xff,
                0x44,
                Color32::from_rgb(0, 255, 68),
            ),
            (
                x56_core::t!("rgb-preset-ops-blue"),
                0x00,
                0x88,
                0xff,
                Color32::from_rgb(0, 136, 255),
            ),
            (
                x56_core::t!("rgb-preset-pure-white"),
                0xff,
                0xff,
                0xff,
                Color32::from_rgb(255, 255, 255),
            ),
            (
                x56_core::t!("rgb-preset-off"),
                0x00,
                0x00,
                0x00,
                Color32::from_rgb(40, 40, 40),
            ),
        ];

        for (name, r, g, b, preview_col) in presets {
            ui.horizontal(|ui| {
                let (rect, _response) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, preview_col);
                if theme::tactical_button(ui, &name, false, false).clicked() {
                    self.set_preset(r, g, b, profile, monitor);
                }
            });
            ui.add_space(2.0);
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        ui.label(
            RichText::new(x56_core::t!("rgb-quick-schemes"))
                .color(COLOR_OUTLINE)
                .size(10.5)
                .strong()
                .monospace(),
        );
        ui.add_space(4.0);

        if theme::tactical_button(
            ui,
            &x56_core::t!("rgb-scheme-tactical"),
            false,
            false,
        )
        .on_hover_text(
            "Configura M1=#9d4edd (Navegación), M2=#ff1100 (Combate) y S1=#0088ff (Operaciones)",
        )
        .clicked()
        {
            self.apply_tactical_tricolor_scheme("#9d4edd", "#ff1100", "#0088ff", profile, monitor);
        }

        ui.add_space(3.0);

        if theme::tactical_button(ui, &x56_core::t!("rgb-scheme-cockpit"), false, false)
            .on_hover_text(
                "Configura M1=#00ff44 (Crucero), M2=#ff6600 (Alerta) y S1=#00ffff (Aterrizaje)",
            )
            .clicked()
        {
            self.apply_tactical_tricolor_scheme("#00ff44", "#ff6600", "#00ffff", profile, monitor);
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

        // Configurar M1 en púrpura (#9d4edd)
        tab.selected_mode = RgbModeScope::M1;
        tab.target = TargetDevice::All;
        tab.color_rgba = [
            0x9d as f32 / 255.0,
            0x4e as f32 / 255.0,
            0xdd as f32 / 255.0,
        ];
        tab.update_profile_color(&mut profile, &monitor);

        // Configurar M2 en rojo (#ff1100)
        tab.selected_mode = RgbModeScope::M2;
        tab.color_rgba = [1.0, 0x11 as f32 / 255.0, 0.0];
        tab.update_profile_color(&mut profile, &monitor);

        // Configurar S1 con Stick azul (#0088ff) y Throttle ámbar (#ff6600)
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

        // Al cambiar de modo en la UI, el selector debe cargar el color guardado de ese modo
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
}
