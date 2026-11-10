//! Sistema de diseño táctico Avionics Vector para x56-gui.
//!
//! Basado en pantallas MFD de aviónica militar y directrices ergonómicas de GNOME/Libadwaita.
//! Define la paleta de colores, geometría, configuración visual de egui y widgets reutilizables.

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

// ============================================================================
// PALETA DE COLORES - AVIONICS VECTOR
// ============================================================================

// Superficies y contenedores
pub const COLOR_SURFACE_LOWEST: Color32 = Color32::from_rgb(0x09, 0x0e, 0x16); // #090e16 - Fondo canvas más profundo
pub const COLOR_SURFACE_LOW: Color32 = Color32::from_rgb(0x17, 0x1c, 0x24); // #171c24 - Sidebar y paneles docked
pub const COLOR_SURFACE: Color32 = Color32::from_rgb(0x0f, 0x14, 0x1c); // #0f141c - Base de aplicación
pub const COLOR_SURFACE_CONTAINER: Color32 = Color32::from_rgb(0x1b, 0x20, 0x28); // #1b2028 - Tarjetas interactivas
pub const COLOR_SURFACE_HIGH: Color32 = Color32::from_rgb(0x25, 0x2a, 0x33); // #252a33 - Elementos elevados / chips
pub const COLOR_SURFACE_HIGHEST: Color32 = Color32::from_rgb(0x30, 0x35, 0x3e); // #30353e - Sliders / inputs activos
pub const COLOR_SURFACE_BRIGHT: Color32 = Color32::from_rgb(0x34, 0x39, 0x42); // #343942 - Hover brillante

// Acentos Primarios (Cian Aviónica)
pub const COLOR_PRIMARY: Color32 = Color32::from_rgb(0xdb, 0xfc, 0xff); // #dbfcff - Texto brillante cian
pub const COLOR_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff); // #00f0ff - Cian puro activo
pub const COLOR_ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(0x00, 0x69, 0x70); // #006970 - Texto sobre cian
pub const COLOR_PRIMARY_FIXED: Color32 = Color32::from_rgb(0x7d, 0xf4, 0xff); // #7df4ff - Resaltados

// Acentos Secundarios (Azul Eléctrico Logitech G)
pub const COLOR_SECONDARY: Color32 = Color32::from_rgb(0xb3, 0xc5, 0xff); // #b3c5ff - Azul suave
pub const COLOR_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0x02, 0x66, 0xff); // #0266ff - Botones primarios
pub const COLOR_ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0xf9, 0xf7, 0xff); // #f9f7ff - Texto sobre azul

// Terciarios / Fósforo HUD (Verde Neón Precisión)
pub const COLOR_TERTIARY: Color32 = Color32::from_rgb(0xdb, 0xff, 0xde); // #dbffde - Resplandor verde
pub const COLOR_TERTIARY_CONTAINER: Color32 = Color32::from_rgb(0x34, 0xf8, 0x85); // #34f885 - Punto fósforo en vivo
pub const COLOR_ON_TERTIARY_CONTAINER: Color32 = Color32::from_rgb(0x00, 0x6e, 0x35); // #006e35
pub const COLOR_TERTIARY_FIXED_DIM: Color32 = Color32::from_rgb(0x00, 0xe4, 0x75); // #00e475 - Estado sincronizado

// Alertas y Errores
pub const COLOR_ERROR: Color32 = Color32::from_rgb(0xff, 0xb4, 0xab); // #ffb4ab - Zona muerta / Clamps
pub const COLOR_ERROR_CONTAINER: Color32 = Color32::from_rgb(0x93, 0x00, 0x0a); // #93000a
pub const COLOR_WARNING_AMBER: Color32 = Color32::from_rgb(0xff, 0xbd, 0x2e); // #ffbd2e - Precaución / C-Stick

// Contornos y Líneas Estructurales
pub const COLOR_OUTLINE: Color32 = Color32::from_rgb(0x84, 0x94, 0x95); // #849495 - Bordes suaves
pub const COLOR_OUTLINE_VARIANT: Color32 = Color32::from_rgb(0x3b, 0x49, 0x4b); // #3b494b - Separadores finos
pub const COLOR_GRID_HUD: Color32 = Color32::from_rgba_premultiplied(0, 240, 255, 30); // Rejilla HUD sutil

// Tipografía y Textos
pub const COLOR_ON_SURFACE: Color32 = Color32::from_rgb(0xde, 0xe2, 0xee); // #dee2ee - Texto principal
pub const COLOR_ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(0xb9, 0xca, 0xcb); // #b9cacb - Texto secundario / mono

// Muelles Físicos (X-56 Springs)
pub const COLOR_SPRING_RED: Color32 = Color32::from_rgb(0xff, 0x5f, 0x56); // 52mm Extra Fuerte
pub const COLOR_SPRING_GREEN: Color32 = Color32::from_rgb(0x34, 0xf8, 0x85); // 47mm Medio (Instalado)
pub const COLOR_SPRING_YELLOW: Color32 = Color32::from_rgb(0xff, 0xbd, 0x2e); // 42mm Ligero
pub const COLOR_SPRING_BLUE: Color32 = Color32::from_rgb(0x02, 0x66, 0xff); // 36mm Ultra Ligero

// ============================================================================
// MODELO DE MUELLES FÍSICOS
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

    pub fn label(&self) -> &'static str {
        match self {
            Self::Red52mm => "ROJO",
            Self::Green47mm => "VERDE",
            Self::Yellow42mm => "AMARILLO",
            Self::Blue36mm => "AZUL",
        }
    }

    pub fn desc(&self) -> &'static str {
        match self {
            Self::Red52mm => "Extra Fuerte (52mm)",
            Self::Green47mm => "Medio (Instalado - 47mm)",
            Self::Yellow42mm => "Ligero (42mm)",
            Self::Blue36mm => "Ultra Ligero (36mm)",
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
// CONFIGURACIÓN DE VISUALS Y ESTILO GLOBAL
// ============================================================================

/// Aplica el tema visual Avionics Vector al contexto de egui.
pub fn configure_theme(ctx: &egui::Context) {
    // Configuración de fuentes vectoriales e iconos Lucide
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "lucide".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES)),
    );
    // En Proportional: Ubuntu-Light -> lucide -> Hack (para símbolos matemáticos/geométricos)
    let prop = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    if !prop.contains(&"lucide".to_owned()) {
        prop.push("lucide".to_owned());
    }
    if !prop.contains(&"Hack".to_owned()) {
        prop.push("Hack".to_owned());
    }

    // En Monospace: Hack -> lucide -> Ubuntu-Light
    let mono = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    if !mono.contains(&"lucide".to_owned()) {
        mono.push("lucide".to_owned());
    }
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();

    // Fondos de paneles y ventanas
    visuals.panel_fill = COLOR_SURFACE;
    visuals.window_fill = COLOR_SURFACE_LOW;
    visuals.extreme_bg_color = COLOR_SURFACE_LOWEST;
    visuals.faint_bg_color = COLOR_SURFACE_LOW;

    // Redondeo táctico militar (4px widgets, 8px contenedores)
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);

    // Controles no interactivos
    visuals.widgets.noninteractive.bg_fill = COLOR_SURFACE_CONTAINER;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COLOR_OUTLINE_VARIANT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, COLOR_ON_SURFACE);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    // Controles inactivos (estado reposo)
    visuals.widgets.inactive.bg_fill = COLOR_SURFACE_CONTAINER;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_OUTLINE_VARIANT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, COLOR_ON_SURFACE);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    // Hover (brillo táctico)
    visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGH;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY_CONTAINER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    // Activo / Presionado
    visuals.widgets.active.bg_fill = COLOR_SECONDARY_CONTAINER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, COLOR_PRIMARY_CONTAINER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, COLOR_ON_SECONDARY_CONTAINER);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    // Abierto / Seleccionado
    visuals.widgets.open.bg_fill = COLOR_SURFACE_HIGH;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY_CONTAINER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.open.corner_radius = CornerRadius::same(4);

    // Selección de texto y enlaces
    visuals.selection.bg_fill = COLOR_SECONDARY_CONTAINER;
    visuals.selection.stroke = Stroke::new(1.0, COLOR_PRIMARY_CONTAINER);
    visuals.hyperlink_color = COLOR_PRIMARY_CONTAINER;

    ctx.set_visuals(visuals);
}

// ============================================================================
// FRAMES Y CONTENEDORES PREDEFINIDOS
// ============================================================================

/// Frame base de tarjeta de control táctica.
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOW)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
}

/// Frame para bloques hundidos (Canvas de gráficos, visualizadores HUD).
pub fn sunken_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
}

/// Frame para la barra de navegación lateral izquierda.
pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOW)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .inner_margin(Margin::symmetric(12, 14))
}

/// Frame para la HeaderBar superior táctica.
pub fn header_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOWEST)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .inner_margin(Margin::symmetric(14, 8))
}

/// Frame para el Dock inferior de acciones globales.
pub fn dock_frame() -> Frame {
    Frame::new()
        .fill(COLOR_SURFACE_LOW)
        .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 10))
}

// ============================================================================
// WIDGETS AUXILIARES
// ============================================================================

/// Dibuja un pill o badge informativo telemétrico monoespaciado.
/// Calcula su tamaño exacto antes de reservar espacio para que `horizontal_wrapped`
/// haga salto de línea limpiamente sin desbordar jamás el borde derecho de su contenedor.
pub fn telemetry_badge(ui: &mut Ui, label: &str, value: &str, val_color: Color32) {
    let label_galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::monospace(10.0),
        COLOR_OUTLINE,
    );
    let value_galley =
        ui.painter()
            .layout_no_wrap(value.to_owned(), egui::FontId::monospace(10.5), val_color);

    let pad_x = 6.0;
    let pad_y = 3.5;
    let inner_gap = 4.0;
    let content_w = label_galley.size().x + inner_gap + value_galley.size().x;
    let content_h = label_galley.size().y.max(value_galley.size().y);
    let total_size = egui::Vec2::new(content_w + pad_x * 2.0, content_h + pad_y * 2.0);

    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), COLOR_SURFACE_LOWEST);
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
            egui::StrokeKind::Inside,
        );

        let label_pos = egui::Pos2::new(
            rect.min.x + pad_x,
            rect.center().y - label_galley.size().y * 0.5,
        );
        let value_pos = egui::Pos2::new(
            label_pos.x + label_galley.size().x + inner_gap,
            rect.center().y - value_galley.size().y * 0.5,
        );

        painter.galley(label_pos, label_galley, COLOR_OUTLINE);
        painter.galley(value_pos, value_galley, val_color);
    }
}

/// Dibuja un badge de estado con contorno coloreado que envuelve atómicamente en `horizontal_wrapped`.
pub fn outline_badge(ui: &mut Ui, text: &str, text_color: Color32, border_color: Color32) {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::monospace(10.0), text_color);

    let pad_x = 8.0;
    let pad_y = 3.5;
    let total_size = egui::Vec2::new(galley.size().x + pad_x * 2.0, galley.size().y + pad_y * 2.0);

    let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(4), COLOR_SURFACE_LOWEST);
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, border_color),
            egui::StrokeKind::Inside,
        );
        let pos = egui::Pos2::new(rect.min.x + pad_x, rect.center().y - galley.size().y * 0.5);
        painter.galley(pos, galley, text_color);
    }
}

/// Botón táctico con acento de color opcional.
pub fn tactical_button(
    ui: &mut Ui,
    text: &str,
    is_active: bool,
    primary_accent: bool,
) -> egui::Response {
    let (bg, text_col, border_col) = if is_active {
        (
            COLOR_PRIMARY_CONTAINER,
            COLOR_ON_PRIMARY_CONTAINER,
            COLOR_PRIMARY,
        )
    } else if primary_accent {
        (
            COLOR_SECONDARY_CONTAINER,
            COLOR_ON_SECONDARY_CONTAINER,
            COLOR_PRIMARY_CONTAINER,
        )
    } else {
        (
            COLOR_SURFACE_CONTAINER,
            COLOR_ON_SURFACE,
            COLOR_OUTLINE_VARIANT,
        )
    };

    let rich = RichText::new(text)
        .color(text_col)
        .size(11.0)
        .strong()
        .monospace();

    ui.add(
        egui::Button::new(rich)
            .fill(bg)
            .stroke(Stroke::new(1.0, border_col))
            .corner_radius(CornerRadius::same(4)),
    )
}
