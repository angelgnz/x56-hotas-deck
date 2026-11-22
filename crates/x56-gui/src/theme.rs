//! Calibrated Graphite + Instrument Cyan design system for `x56-gui`.
//!
//! Defines the neutral Graphite surface ramp, hairlines, three text roles,
//! single Instrument Cyan accent, off-white primary CTA, typography/spacing/radius
//! tokens, font loading, `egui::Visuals`, frames, and shared ergonomic widgets.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Stroke, Ui, Vec2,
};

// ============================================================================
// SURFACES (GRAPHITE NEUTRAL RAMP)
// ============================================================================

/// Deepest surface: sidebar, sunken plot/schematic canvases, keycap wells (`#0f0f10`).
pub const COLOR_SURFACE_LOWEST: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x10);
/// Base application surface: CentralPanel, header bar, bottom dock (`#141415`).
pub const COLOR_SURFACE: Color32 = Color32::from_rgb(0x14, 0x14, 0x15);
/// Primary card, modal, and menu surface (`#19191b`).
pub const COLOR_SURFACE_LOW: Color32 = Color32::from_rgb(0x19, 0x19, 0x1b);
/// Nested sub-cards and grouped blocks (`#1e1e21`).
pub const COLOR_SURFACE_CONTAINER: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x21);
/// Raised resting controls: active nav row, inputs, secondary buttons (`#232325`).
pub const COLOR_SURFACE_HIGH: Color32 = Color32::from_rgb(0x23, 0x23, 0x25);
/// Hover state for raised controls and slider tracks (`#2c2c30`).
pub const COLOR_SURFACE_HIGHEST: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x30);
/// Pressed state for raised controls (`#343439`).
pub const COLOR_SURFACE_BRIGHT: Color32 = Color32::from_rgb(0x34, 0x34, 0x39);

// ============================================================================
// BORDERS, HAIRLINES & GRID
// ============================================================================

/// Subtle divider (~5% primary text opacity).
pub const COLOR_BORDER_SUBTLE: Color32 = Color32::from_rgba_unmultiplied_const(237, 237, 237, 13);
/// Standard structural hairline (~8% primary text opacity).
pub const COLOR_BORDER_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(237, 237, 237, 20);
/// Emphasized hover/focus hairline (~15% primary text opacity).
pub const COLOR_BORDER_STRONG: Color32 = Color32::from_rgba_unmultiplied_const(237, 237, 237, 38);
/// Top inset highlight for raised surfaces (~9% white).
pub const COLOR_TOP_HIGHLIGHT: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 22);
/// Quiet neutral plot/schematic grid line (~5.5% primary text opacity).
pub const COLOR_GRID_HUD: Color32 = Color32::from_rgba_unmultiplied_const(237, 237, 237, 14);

// ============================================================================
// NEUTRAL TEXT ROLES (3 ROLES ONLY)
// ============================================================================

/// Primary text (`#ededed`).
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(0xed, 0xed, 0xed);
/// Secondary text and captions (`#a1a1a6`).
pub const COLOR_TEXT_SECONDARY: Color32 = Color32::from_rgb(0xa1, 0xa1, 0xa6);
/// Muted metadata and icon rest role (`#85858b`).
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(0x85, 0x85, 0x8b);

// ============================================================================
// SINGLE ACCENT & PRIMARY CTA
// ============================================================================

/// Calibrated Instrument Cyan accent (`#38bdf8`).
pub const COLOR_ACCENT: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);
/// Soft Instrument Cyan tint for active selection backgrounds.
pub const COLOR_ACCENT_SOFT: Color32 = Color32::from_rgba_unmultiplied_const(56, 189, 248, 28);
/// Instrument Cyan border for active selections and focused controls.
pub const COLOR_ACCENT_BORDER: Color32 = Color32::from_rgba_unmultiplied_const(56, 189, 248, 120);
/// Dark text over solid Instrument Cyan fills (`#0a1016`).
pub const COLOR_ON_ACCENT: Color32 = Color32::from_rgb(0x0a, 0x10, 0x16);

/// Primary CTA resting fill (`#ededed`).
pub const COLOR_CTA_BG: Color32 = Color32::from_rgb(0xed, 0xed, 0xed);
/// Primary CTA hover fill (`#ffffff`).
pub const COLOR_CTA_BG_HOVER: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
/// Primary CTA label color (`#141415`).
pub const COLOR_CTA_TEXT: Color32 = Color32::from_rgb(0x14, 0x14, 0x15);

// ============================================================================
// TONED SEMANTIC COLORS
// ============================================================================

/// Positive / synced / online state (`#4cc38a`).
pub const COLOR_POSITIVE: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
/// Warning / modified / simulation state (`#e5a93c`).
pub const COLOR_WARNING: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3c);
/// Negative / error / disconnected state (`#e5484d`).
pub const COLOR_NEGATIVE: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);

// ============================================================================
// PHYSICAL SPRINGS (FOR 8PX SWATCH DOTS ONLY)
// ============================================================================

pub const COLOR_SPRING_RED: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x52);
pub const COLOR_SPRING_GREEN: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const COLOR_SPRING_YELLOW: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3c);
pub const COLOR_SPRING_BLUE: Color32 = Color32::from_rgb(0x4f, 0x8f, 0xf7);

// ============================================================================
// TYPOGRAPHY, SPACING & RADIUS TOKENS
// ============================================================================

pub const FONT_META: f32 = 12.0;
pub const FONT_SMALL: f32 = 13.0;
pub const FONT_BODY: f32 = 14.0;
pub const FONT_HEADING: f32 = 16.0;
pub const FONT_FIGURE: f32 = 22.0;

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 20.0;
pub const SPACE_2XL: f32 = 24.0;

pub const RADIUS_KEYCAP: u8 = 4;
pub const RADIUS_CONTROL: u8 = 8;
pub const RADIUS_CARD: u8 = 12;

// ============================================================================
// PHYSICAL SPRING MODEL
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhysicalSpring {
    Red52mm,
    #[default]
    Green47mm,
    Yellow42mm,
    Blue36mm,
}

impl PhysicalSpring {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Red52mm => "red",
            Self::Green47mm => "green",
            Self::Yellow42mm => "yellow",
            Self::Blue36mm => "blue",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "red" | "rojo" | "52mm" => Some(Self::Red52mm),
            "green" | "verde" | "47mm" => Some(Self::Green47mm),
            "yellow" | "amarillo" | "42mm" => Some(Self::Yellow42mm),
            "blue" | "azul" | "36mm" => Some(Self::Blue36mm),
            _ => None,
        }
    }

    pub fn localized_label(&self) -> String {
        match self {
            Self::Red52mm => x56_core::t!("curves-spring-red-label"),
            Self::Green47mm => x56_core::t!("curves-spring-green-label"),
            Self::Yellow42mm => x56_core::t!("curves-spring-yellow-label"),
            Self::Blue36mm => x56_core::t!("curves-spring-blue-label"),
        }
    }

    pub fn localized_desc(&self) -> String {
        match self {
            Self::Red52mm => x56_core::t!("curves-spring-red-desc"),
            Self::Green47mm => x56_core::t!("curves-spring-green-desc"),
            Self::Yellow42mm => x56_core::t!("curves-spring-yellow-desc"),
            Self::Blue36mm => x56_core::t!("curves-spring-blue-desc"),
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Red52mm => COLOR_SPRING_RED,
            Self::Green47mm => COLOR_SPRING_GREEN,
            Self::Yellow42mm => COLOR_SPRING_YELLOW,
            Self::Blue36mm => COLOR_SPRING_BLUE,
        }
    }
}

// ============================================================================
// GLOBAL THEME CONFIGURATION & FONT LOADING
// ============================================================================

fn load_first_existing_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| std::fs::read(path).ok())
}

/// Applies the Graphite + Instrument Cyan theme and system fonts to the egui context.
pub fn configure_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "lucide".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES)),
    );

    let prop_name = load_first_existing_font(&[
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/FiraSans-Regular.ttf",
        "/usr/share/fonts/cantarell/Cantarell-VF.otf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ])
    .map(|bytes| {
        fonts.font_data.insert(
            "ui_sans".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        "ui_sans".to_owned()
    });

    let mono_name = load_first_existing_font(&[
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    ])
    .map(|bytes| {
        fonts.font_data.insert(
            "ui_mono".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        "ui_mono".to_owned()
    });

    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    if let Some(ref sans) = prop_name {
        if !prop.contains(sans) {
            prop.insert(0, sans.clone());
        }
    }
    if !prop.contains(&"lucide".to_owned()) {
        prop.push("lucide".to_owned());
    }
    if !prop.contains(&"Hack".to_owned()) {
        prop.push("Hack".to_owned());
    }

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    if let Some(ref m) = mono_name {
        if !mono.contains(m) {
            mono.insert(0, m.clone());
        }
    }
    if !mono.contains(&"lucide".to_owned()) {
        mono.push("lucide".to_owned());
    }

    fonts
        .families
        .insert(FontFamily::Name("lucide".into()), vec!["lucide".to_owned()]);

    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            FontId::new(FONT_META, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(FONT_BODY, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(FONT_SMALL, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            FontId::new(FONT_HEADING, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            FontId::new(FONT_META, FontFamily::Monospace),
        );

        style.spacing.item_spacing = Vec2::new(SPACE_SM, SPACE_SM);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size = Vec2::new(32.0, 28.0);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = COLOR_SURFACE;
        visuals.window_fill = COLOR_SURFACE_LOW;
        visuals.extreme_bg_color = COLOR_SURFACE_LOWEST;
        visuals.faint_bg_color = COLOR_SURFACE_CONTAINER;
        visuals.window_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
        visuals.window_corner_radius = CornerRadius::same(RADIUS_CARD);
        visuals.menu_corner_radius = CornerRadius::same(RADIUS_CONTROL);
        visuals.slider_trailing_fill = true;

        visuals.widgets.noninteractive.bg_fill = COLOR_SURFACE_CONTAINER;
        visuals.widgets.noninteractive.weak_bg_fill = COLOR_SURFACE_CONTAINER;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, COLOR_TEXT_PRIMARY);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(RADIUS_CONTROL);

        visuals.widgets.inactive.bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, COLOR_TEXT_PRIMARY);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(RADIUS_CONTROL);

        visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, COLOR_TEXT_PRIMARY);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(RADIUS_CONTROL);

        visuals.widgets.active.bg_fill = COLOR_SURFACE_BRIGHT;
        visuals.widgets.active.weak_bg_fill = COLOR_SURFACE_BRIGHT;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_ACCENT_BORDER);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, COLOR_TEXT_PRIMARY);
        visuals.widgets.active.corner_radius = CornerRadius::same(RADIUS_CONTROL);

        visuals.widgets.open.bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.open.weak_bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, COLOR_TEXT_PRIMARY);
        visuals.widgets.open.corner_radius = CornerRadius::same(RADIUS_CONTROL);

        visuals.selection.bg_fill = COLOR_ACCENT_SOFT;
        visuals.selection.stroke = Stroke::new(1.0, COLOR_ACCENT_BORDER);
        visuals.hyperlink_color = COLOR_ACCENT;

        style.visuals = visuals;
    });
}

// ============================================================================
// FRAMES & SURFACES
// ============================================================================

/// Primary card frame (`#19191b`, 1px hairline, `r=12`, `16px` padding).
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOW)
        .stroke(Stroke::new(1.0, COLOR_BORDER_HAIRLINE))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::same(SPACE_LG as i8))
}

/// Nested sub-card frame (`#1e1e21`, 1px hairline, `r=8`, `12px` padding).
pub fn subcard_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_CONTAINER)
        .stroke(Stroke::new(1.0, COLOR_BORDER_HAIRLINE))
        .corner_radius(CornerRadius::same(RADIUS_CONTROL))
        .inner_margin(Margin::same(SPACE_MD as i8))
}

/// Recessed plot/schematic canvas frame (`#0f0f10`, 1px hairline, `r=8`, `12px` padding).
pub fn canvas_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .stroke(Stroke::new(1.0, COLOR_BORDER_HAIRLINE))
        .corner_radius(CornerRadius::same(RADIUS_CONTROL))
        .inner_margin(Margin::same(SPACE_MD as i8))
}

/// Left sidebar navigation frame (`#0f0f10`, `r=0`, `12x16` padding).
pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(Margin::symmetric(SPACE_MD as i8, SPACE_LG as i8))
}

/// Top header bar frame (`#141415`, `r=0`, `16x8` padding).
pub fn header_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE)
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(Margin::symmetric(SPACE_LG as i8, SPACE_SM as i8))
}

/// Bottom action dock frame (`#141415`, `r=0`, `16x8` padding).
pub fn dock_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE)
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(Margin::symmetric(SPACE_LG as i8, SPACE_SM as i8))
}

// ============================================================================
// ERGONOMIC SHARED WIDGETS
// ============================================================================

/// Primary off-white CTA button (`#ededed` bg, `#ffffff` hover, `#141415` text, `r=8`, `min_h=32`).
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = COLOR_CTA_BG;
        visuals.widgets.inactive.bg_fill = COLOR_CTA_BG;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, COLOR_CTA_TEXT);
        visuals.widgets.hovered.weak_bg_fill = COLOR_CTA_BG_HOVER;
        visuals.widgets.hovered.bg_fill = COLOR_CTA_BG_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, COLOR_CTA_TEXT);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0xd4, 0xd4, 0xd8);
        visuals.widgets.active.bg_fill = Color32::from_rgb(0xd4, 0xd4, 0xd8);
        visuals.widgets.active.bg_stroke = Stroke::NONE;
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, COLOR_CTA_TEXT);
    }
    let rich = RichText::new(text).color(COLOR_CTA_TEXT).size(FONT_SMALL);
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 32.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;
    resp
}

/// Secondary raised surface button (`#232325`, 1px hairline, `#ededed` text, `r=8`, `min_h=32`).
pub fn secondary_button(ui: &mut Ui, text: &str) -> egui::Response {
    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.inactive.bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
        visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
        visuals.widgets.active.weak_bg_fill = COLOR_SURFACE_BRIGHT;
        visuals.widgets.active.bg_fill = COLOR_SURFACE_BRIGHT;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
    }
    let rich = RichText::new(text)
        .color(COLOR_TEXT_PRIMARY)
        .size(FONT_SMALL);
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 32.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;

    if ui.is_rect_visible(resp.rect) {
        let r = resp.rect;
        ui.painter().hline(
            (r.min.x + RADIUS_CONTROL as f32)..=(r.max.x - RADIUS_CONTROL as f32),
            r.min.y + 1.0,
            Stroke::new(1.0, COLOR_TOP_HIGHLIGHT),
        );
    }
    resp
}

/// Quiet transparent ghost button (`#a1a1a6` text, `#232325` hover fill, `r=8`, `min_h=32`).
pub fn ghost_button(ui: &mut Ui, text: &str) -> egui::Response {
    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGH;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_BORDER_SUBTLE);
        visuals.widgets.active.weak_bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.active.bg_fill = COLOR_SURFACE_HIGHEST;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
    }
    let rich = RichText::new(text)
        .color(COLOR_TEXT_SECONDARY)
        .size(FONT_SMALL);
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 32.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;
    resp
}

/// Destructive action button (`#e5484d` semantic red text and subtle tint, `r=8`, `min_h=32`).
pub fn danger_button(ui: &mut Ui, text: &str) -> egui::Response {
    let rest_bg = Color32::from_rgba_unmultiplied_const(229, 72, 77, 24);
    let hover_bg = Color32::from_rgba_unmultiplied_const(229, 72, 77, 46);
    let active_bg = Color32::from_rgba_unmultiplied_const(229, 72, 77, 68);
    let border = Color32::from_rgba_unmultiplied_const(229, 72, 77, 92);

    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = rest_bg;
        visuals.widgets.inactive.bg_fill = rest_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
        visuals.widgets.hovered.weak_bg_fill = hover_bg;
        visuals.widgets.hovered.bg_fill = hover_bg;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_NEGATIVE);
        visuals.widgets.active.weak_bg_fill = active_bg;
        visuals.widgets.active.bg_fill = active_bg;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_NEGATIVE);
    }
    let rich = RichText::new(text).color(COLOR_NEGATIVE).size(FONT_SMALL);
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 32.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;
    resp
}

/// Segmented selector button (`r=8`, `min_h=32`, single Instrument Cyan border when selected).
pub fn segmented_button(ui: &mut Ui, text: &str, selected: bool) -> egui::Response {
    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        if selected {
            visuals.widgets.inactive.weak_bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.inactive.bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_ACCENT_BORDER);
            visuals.widgets.hovered.weak_bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.hovered.bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_ACCENT);
        } else {
            visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_CONTAINER;
            visuals.widgets.inactive.bg_fill = COLOR_SURFACE_CONTAINER;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
            visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGH;
            visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGH;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
        }
    }
    let text_col = if selected {
        COLOR_TEXT_PRIMARY
    } else {
        COLOR_TEXT_SECONDARY
    };
    let rich = RichText::new(text).color(text_col).size(FONT_SMALL);
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 32.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;
    resp
}

/// Recessed keycap well (`#0f0f10`, 1px hairline, `r=4`, `12px` mono `#ededed`).
pub fn keycap(ui: &mut Ui, text: &str) {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::monospace(FONT_META),
        COLOR_TEXT_PRIMARY,
    );
    let pad_x = 6.0;
    let pad_y = 3.0;
    let total_size = Vec2::new(galley.size().x + pad_x * 2.0, galley.size().y + pad_y * 2.0);
    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(RADIUS_KEYCAP),
            COLOR_SURFACE_LOWEST,
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(RADIUS_KEYCAP),
            Stroke::new(1.0, COLOR_BORDER_HAIRLINE),
            egui::StrokeKind::Inside,
        );
        let pos = egui::Pos2::new(rect.min.x + pad_x, rect.center().y - galley.size().y * 0.5);
        painter.galley(pos, galley, COLOR_TEXT_PRIMARY);
    }
}

/// Tactile selector detent for M1 / M2 / S1 mode switches (`r=8`, `min_h=28`).
pub fn mode_detent(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let prev_widgets = ui.visuals().widgets.clone();
    {
        let visuals = ui.visuals_mut();
        if selected {
            visuals.widgets.inactive.weak_bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.inactive.bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_ACCENT_BORDER);
            visuals.widgets.hovered.weak_bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.hovered.bg_fill = COLOR_ACCENT_SOFT;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_ACCENT);
        } else {
            visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_HIGH;
            visuals.widgets.inactive.bg_fill = COLOR_SURFACE_HIGH;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_BORDER_HAIRLINE);
            visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGHEST;
            visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGHEST;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_BORDER_STRONG);
        }
    }
    let text_col = if selected {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_SECONDARY
    };
    let rich = RichText::new(label)
        .color(text_col)
        .size(FONT_META)
        .monospace();
    let resp = ui.add(
        egui::Button::new(rich)
            .wrap_mode(egui::TextWrapMode::Extend)
            .corner_radius(CornerRadius::same(RADIUS_CONTROL))
            .min_size(Vec2::new(0.0, 28.0)),
    );
    ui.visuals_mut().widgets = prev_widgets;

    if selected && ui.is_rect_visible(resp.rect) {
        let r = resp.rect;
        ui.painter().hline(
            (r.min.x + RADIUS_CONTROL as f32)..=(r.max.x - RADIUS_CONTROL as f32),
            r.min.y + 1.0,
            Stroke::new(1.0, COLOR_TOP_HIGHLIGHT),
        );
    }
    resp
}

/// Flat circular status dot with no glow or shadow.
pub fn status_dot(ui: &mut Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().circle_filled(rect.center(), size * 0.5, color);
    }
}

/// Quiet inline monospace key-value metadata without any surrounding pill border.
pub fn meta_kv(ui: &mut Ui, label: &str, value: &str, value_color: Color32) {
    let label_galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::monospace(FONT_META),
        COLOR_TEXT_MUTED,
    );
    let value_galley =
        ui.painter()
            .layout_no_wrap(value.to_owned(), FontId::monospace(FONT_META), value_color);

    let gap = SPACE_XS;
    let total_size = Vec2::new(
        label_galley.size().x + gap + value_galley.size().x,
        label_galley.size().y.max(value_galley.size().y),
    );

    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let label_pos = egui::Pos2::new(rect.min.x, rect.center().y - label_galley.size().y * 0.5);
        let value_pos = egui::Pos2::new(
            label_pos.x + label_galley.size().x + gap,
            rect.center().y - value_galley.size().y * 0.5,
        );
        painter.galley(label_pos, label_galley, COLOR_TEXT_MUTED);
        painter.galley(value_pos, value_galley, value_color);
    }
}

/// Clean proportional section heading in sentence case without decorative icons.
pub fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .color(COLOR_TEXT_PRIMARY)
            .size(FONT_HEADING),
    );
}
