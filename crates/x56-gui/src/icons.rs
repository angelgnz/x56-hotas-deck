//! Iconos vectoriales Lucide para x56-gui.
//!
//! Basados en el set de iconos de Lucide (v1.71.0) cargados a través de `lucide-icons`
//! integrados en la fuente de eframe/egui.
//! Permite renderizado vectorial limpio, consistente y nítido a cualquier resolución,
//! eliminando los fallos de glyphs ('tofu') de emojis en sistemas Linux.

pub use lucide_icons::Icon;

// ============================================================================
// CONSTANTES DE GLYPHS LUCIDE (&'static str)
// ============================================================================

// Navegación, Sistema y Dispositivos
pub const PLANE: &str = "\u{e1de}";
pub const JOYSTICK: &str = "\u{e355}";
pub const GAMEPAD: &str = "\u{e0df}";
pub const SLIDERS: &str = "\u{e162}";
pub const SLIDERS_HORIZONTAL: &str = "\u{e29a}";
pub const PALETTE: &str = "\u{e1dd}";
pub const LIGHTBULB: &str = "\u{e1c2}";
pub const BOXES: &str = "\u{e2d0}";
pub const LAYERS: &str = "\u{e529}";
pub const CPU: &str = "\u{e0a9}";
pub const RADIO: &str = "\u{e142}";
pub const COMPASS: &str = "\u{e09b}";
pub const CROSSHAIR: &str = "\u{e0ac}";
pub const TARGET: &str = "\u{e180}";
pub const ROCKET: &str = "\u{e286}";

// Acciones y Edición
pub const SAVE: &str = "\u{e14d}";
pub const FOLDER: &str = "\u{e0d7}";
pub const FOLDER_OPEN: &str = "\u{e247}";
pub const ROTATE_CCW: &str = "\u{e148}";
pub const ROTATE_CW: &str = "\u{e149}";
pub const REFRESH_CW: &str = "\u{e145}";
pub const ZAP: &str = "\u{e1b4}";
pub const PENCIL: &str = "\u{e1f9}";
pub const TRASH: &str = "\u{e18e}";
pub const SEARCH: &str = "\u{e151}";
pub const TERMINAL: &str = "\u{e181}";
pub const PLUS: &str = "\u{e13d}";
pub const CHECK: &str = "\u{e06c}";
pub const X: &str = "\u{e1b2}";
pub const UPLOAD: &str = "\u{e19e}";
pub const DOWNLOAD: &str = "\u{e0b2}";

// Telemetría, Estados e Indicadores
pub const ACTIVITY: &str = "\u{e038}";
pub const SHIELD_CHECK: &str = "\u{e1ff}";
pub const SHIELD: &str = "\u{e158}";
pub const CIRCLE_CHECK: &str = "\u{e226}";
pub const CIRCLE_ALERT: &str = "\u{e077}";
pub const ALERT_TRIANGLE: &str = "\u{e193}";
pub const CIRCLE: &str = "\u{e076}";
pub const CIRCLE_DOT: &str = "\u{e345}";
pub const SQUARE: &str = "\u{e167}";
pub const PLAY: &str = "\u{e13c}";
pub const FLAME: &str = "\u{e0d2}";
pub const SPARKLES: &str = "\u{e412}";
pub const WRENCH: &str = "\u{e1b1}";
pub const SETTINGS: &str = "\u{e154}";
pub const FILE_CODE: &str = "\u{e0c3}";
pub const FILE_TEXT: &str = "\u{e0cc}";
pub const FACTORY: &str = "\u{e29f}";
pub const EYE: &str = "\u{e0ba}";
pub const EYE_OFF: &str = "\u{e0bb}";
pub const CHEVRON_LEFT: &str = "\u{e06e}";
pub const CHEVRON_RIGHT: &str = "\u{e06f}";
pub const CHEVRON_UP: &str = "\u{e070}";
pub const CHEVRON_DOWN: &str = "\u{e06d}";
pub const MENU: &str = "\u{e115}";
pub const PANEL_LEFT_CLOSE: &str = "\u{e21c}";
pub const PANEL_LEFT: &str = "\u{e12a}";
pub const CORNER_DOWN_RIGHT: &str = "\u{e0a2}";
pub const ARROW_RIGHT: &str = "\u{e049}";
pub const ARROW_LEFT: &str = "\u{e048}";
pub const ARROW_UP: &str = "\u{e04a}";
pub const ARROW_DOWN: &str = "\u{e042}";

// Mapeos Especiales y HOTAS
pub const PLANE_LANDING: &str = "\u{e3cd}";
pub const PLANE_TAKEOFF: &str = "\u{e3ce}";
pub const CHEVRONS_UP: &str = "\u{e074}";
pub const CHEVRONS_DOWN: &str = "\u{e071}";
pub const OCTAGON_ALERT: &str = "\u{e127}";
pub const PICKAXE: &str = "\u{e5c6}";

/// Genera una cadena con icono y etiqueta con espaciado consistente.
#[inline]
pub fn icon_label(icon: &str, label: &str) -> String {
    format!("{icon} {label}")
}

/// Genera una cadena con icono de variante enum `Icon`.
#[inline]
pub fn from_icon(icon: Icon, label: &str) -> String {
    format!("{} {label}", icon.unicode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lucide_icon_font() {
        assert!(!lucide_icons::LUCIDE_FONT_BYTES.is_empty());
        assert_eq!(Icon::Save.unicode(), '\u{e14d}');
        assert_eq!(Icon::Plane.unicode(), '\u{e1de}');
    }
}
