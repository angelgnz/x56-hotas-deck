//! Capa de dominio y lógica de negocio para la gestión del Saitek / Logitech X56.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
pub use x56_hid::{
    identify_device, DeviceKind, DeviceVendor, HidTransportError, X56Descriptor, X56Device,
};
pub use x56_proto::{AxisConfig, CurveProfile, ProtoError, RgbColor};

pub mod buttons;
pub mod calibrate;
pub mod doctor;
pub mod i18n;
pub mod profile;

pub use buttons::{
    all_button_descriptors, catalog_for_device, detect_current_mode, ButtonDescriptor,
    ButtonMonitor, CommandCell, LiveButtonsSnapshot, ModeBindings, ModeSlot, ProgrammingProfile,
    TriggeredCommandEvent,
};
pub use calibrate::{Calibrator, HallCalibrationResult};
pub use doctor::{CheckStatus, Doctor, DoctorCheck, DoctorReport};
pub use i18n::Language;
pub use profile::{
    AxisProfileEntry, DeviceProfile, FullProfile, ProfileApplySummary, ProfileManager,
};

/// Error general de la capa de dominio `x56-core`.
#[derive(Debug)]
pub enum CoreError {
    /// Error de transporte HID.
    Transport(HidTransportError),
    /// Error de parseo o especificación de color.
    InvalidColor(String),
    /// Eje desconocido para el dispositivo indicado.
    UnknownAxis { name: String, kind: DeviceKind },
    /// Error de protocolo.
    Proto(ProtoError),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "transport error: {err}"),
            Self::InvalidColor(s) => write!(
                f,
                "invalid color '{s}', expected hex (#RRGGBB) or named color"
            ),
            Self::UnknownAxis { name, kind } => {
                write!(f, "unknown axis '{name}' for device {kind}")
            }
            Self::Proto(err) => write!(f, "protocol error: {err}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            Self::Proto(err) => Some(err),
            _ => None,
        }
    }
}

impl From<HidTransportError> for CoreError {
    fn from(err: HidTransportError) -> Self {
        Self::Transport(err)
    }
}

impl From<ProtoError> for CoreError {
    fn from(err: ProtoError) -> Self {
        Self::Proto(err)
    }
}

/// Destino de la operación (Stick, Throttle o ambos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetDevice {
    Stick,
    Throttle,
    All,
}

impl FromStr for TargetDevice {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "stick" | "s" | "joystick" => Ok(Self::Stick),
            "throttle" | "t" => Ok(Self::Throttle),
            "all" | "a" | "both" => Ok(Self::All),
            other => Err(CoreError::InvalidColor(format!("unknown target '{other}'"))),
        }
    }
}

/// Parsea un color en formato hexadecimal (`#RRGGBB`, `RRGGBB`, `#RGB`) o por nombre común.
pub fn parse_color(input: &str) -> Result<RgbColor, CoreError> {
    let s = input.trim().to_ascii_lowercase();

    // Colores por nombre
    match s.as_str() {
        "red" | "rojo" => return Ok(RgbColor::new(255, 0, 0)),
        "green" | "verde" => return Ok(RgbColor::new(0, 255, 0)),
        "blue" | "azul" => return Ok(RgbColor::new(0, 0, 255)),
        "white" | "blanco" => return Ok(RgbColor::new(255, 255, 255)),
        "black" | "negro" | "off" | "apagado" => return Ok(RgbColor::new(0, 0, 0)),
        "yellow" | "amarillo" => return Ok(RgbColor::new(255, 255, 0)),
        "cyan" => return Ok(RgbColor::new(0, 255, 255)),
        "magenta" | "purple" | "morado" => return Ok(RgbColor::new(255, 0, 255)),
        "orange" | "naranja" => return Ok(RgbColor::new(255, 128, 0)),
        _ => {}
    }

    // Formato hexadecimal
    let hex = s.strip_prefix('#').unwrap_or(&s);
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?;
        Ok(RgbColor::new(r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?
            * 17;
        let g = u8::from_str_radix(&hex[1..2], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?
            * 17;
        let b = u8::from_str_radix(&hex[2..3], 16)
            .map_err(|_| CoreError::InvalidColor(input.to_string()))?
            * 17;
        Ok(RgbColor::new(r, g, b))
    } else {
        Err(CoreError::InvalidColor(input.to_string()))
    }
}

/// Mapeo de nombres descriptivos de ejes a IDs numéricos del protocolo.
pub fn resolve_axis_id(name: &str, kind: DeviceKind) -> Result<u8, CoreError> {
    let s = name.trim().to_ascii_lowercase();

    // Notación hexadecimal explícita (ej: 0x30, 0x31)
    if let Some(hex_part) = s.strip_prefix("0x") {
        if let Ok(id) = u8::from_str_radix(hex_part, 16) {
            return Ok(id);
        }
    }

    // Compatibilidad con especificación de documentación (30, 31, 32, 35, 36, 37 como hex)
    match s.as_str() {
        "30" => return Ok(0x30),
        "31" => return Ok(0x31),
        "32" => return Ok(0x32),
        "35" => return Ok(0x35),
        "36" => return Ok(0x36),
        "37" => return Ok(0x37),
        _ => {}
    }

    // Número decimal directo
    if let Ok(id) = s.parse::<u8>() {
        return Ok(id);
    }

    match kind {
        DeviceKind::Stick => match s.as_str() {
            "x" | "roll" => Ok(0x30),
            "y" | "pitch" => Ok(0x31),
            "z" | "yaw" | "twist" | "rudder" => Ok(0x35),
            _ => Err(CoreError::UnknownAxis {
                name: name.to_string(),
                kind,
            }),
        },
        DeviceKind::Throttle => match s.as_str() {
            "left" | "throttle_left" | "tl" | "t1" => Ok(0x30),
            "right" | "throttle_right" | "tr" | "t2" => Ok(0x31),
            "rotary1" | "r1" => Ok(0x32),
            "rotary2" | "r2" => Ok(0x35),
            "rotary3" | "r3" => Ok(0x37),
            "rotary4" | "r4" => Ok(0x36),
            _ => Err(CoreError::UnknownAxis {
                name: name.to_string(),
                kind,
            }),
        },
    }
}

/// Obtiene el nombre descriptivo estándar de un ID de eje según el tipo de dispositivo.
pub fn axis_name(axis_id: u8, kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Stick => match axis_id {
            0x30 => "X (Roll)",
            0x31 => "Y (Pitch)",
            0x35 => "Z (Twist/Yaw)",
            0x38 => "Ministick X (Slew)",
            0x39 => "Ministick Y (Slew)",
            _ => "Unknown Stick Axis",
        },
        DeviceKind::Throttle => match axis_id {
            0x30 => "Throttle Left",
            0x31 => "Throttle Right",
            0x32 => "Rotary 1",
            0x35 => "Rotary 2",
            0x36 => "Rotary 4",
            0x37 => "Rotary 3",
            0x3a => "Mini-Gimbal X (Slew)",
            0x3b => "Mini-Gimbal Y (Slew)",
            _ => "Unknown Throttle Axis",
        },
    }
}

/// Fachada de gestión de alto nivel para interactuar con los dispositivos X56.
pub struct X56Manager;

impl X56Manager {
    /// Enumera los dispositivos X56 conectados en la Interfaz 2.
    pub fn list_devices() -> Vec<X56Descriptor> {
        X56Device::enumerate()
    }

    /// Configura la iluminación LED RGB en el dispositivo o dispositivos indicados
    /// mediante una única sesión HID unificada ([`X56Device::set_rgb_unified`]).
    pub fn set_rgb(target: TargetDevice, color: RgbColor) -> Result<usize, CoreError> {
        let (stick_color, throttle_color) = match target {
            TargetDevice::Stick => (Some(color), None),
            TargetDevice::Throttle => (None, Some(color)),
            TargetDevice::All => (Some(color), Some(color)),
        };

        log::info!(
            "X56Manager::set_rgb target={:?} color=#{:02x}{:02x}{:02x}",
            target,
            color.r,
            color.g,
            color.b
        );

        let (stick_ok, throttle_ok) = X56Device::set_rgb_unified(stick_color, throttle_color)?;

        match target {
            TargetDevice::Stick if !stick_ok => {
                log::warn!("X56Manager::set_rgb failed: Stick not found or unreachable");
                Err(CoreError::Transport(HidTransportError::DeviceNotFound(
                    DeviceKind::Stick,
                )))
            }
            TargetDevice::Throttle if !throttle_ok => {
                log::warn!("X56Manager::set_rgb failed: Throttle not found or unreachable");
                Err(CoreError::Transport(HidTransportError::DeviceNotFound(
                    DeviceKind::Throttle,
                )))
            }
            _ => {
                let count = (stick_ok as usize) + (throttle_ok as usize);
                log::debug!("X56Manager::set_rgb updated {count} device(s)");
                Ok(count)
            }
        }
    }

    /// Lee la configuración de un eje específico en el dispositivo indicado.
    pub fn get_axis_config(
        kind: DeviceKind,
        axis_name_or_id: &str,
    ) -> Result<AxisConfig, CoreError> {
        let axis_id = resolve_axis_id(axis_name_or_id, kind)?;
        let dev = X56Device::open(kind)?;
        let cfg = dev.get_axis_config(axis_id)?;
        Ok(cfg)
    }

    /// Modifica la configuración de un eje en el dispositivo indicado.
    pub fn set_axis_config(
        kind: DeviceKind,
        axis_name_or_id: &str,
        xsat: Option<u16>,
        ysat: Option<u16>,
        deadband: Option<u16>,
        curvature: Option<u16>,
        profile: Option<CurveProfile>,
    ) -> Result<AxisConfig, CoreError> {
        let axis_id = resolve_axis_id(axis_name_or_id, kind)?;
        let dev = X56Device::open(kind)?;

        // Leer primero la configuración actual para preservar los campos no especificados
        let mut cfg = dev.get_axis_config(axis_id)?;

        if let Some(x) = xsat {
            cfg.xsat = x;
        }
        if let Some(y) = ysat {
            cfg.ysat = y;
        }
        if let Some(d) = deadband {
            cfg.deadband = d;
        }
        if let Some(c) = curvature {
            cfg.curvature = c;
        }
        if let Some(p) = profile {
            cfg.profile = p;
        }

        dev.set_axis_config(&cfg)?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_formats() {
        assert_eq!(parse_color("#ff0000").unwrap(), RgbColor::new(255, 0, 0));
        assert_eq!(parse_color("00ff00").unwrap(), RgbColor::new(0, 255, 0));
        assert_eq!(parse_color("#00f").unwrap(), RgbColor::new(0, 0, 255));
        assert_eq!(parse_color("red").unwrap(), RgbColor::new(255, 0, 0));
        assert_eq!(parse_color("VERDE").unwrap(), RgbColor::new(0, 255, 0));
        assert_eq!(parse_color("off").unwrap(), RgbColor::new(0, 0, 0));
        assert!(parse_color("invalid-color-123").is_err());
    }

    #[test]
    fn axis_resolution() {
        assert_eq!(resolve_axis_id("x", DeviceKind::Stick).unwrap(), 0x30);
        assert_eq!(resolve_axis_id("Y", DeviceKind::Stick).unwrap(), 0x31);
        assert_eq!(resolve_axis_id("z", DeviceKind::Stick).unwrap(), 0x35);
        assert_eq!(resolve_axis_id("0x30", DeviceKind::Stick).unwrap(), 0x30);
        assert_eq!(resolve_axis_id("30", DeviceKind::Stick).unwrap(), 0x30);

        assert_eq!(resolve_axis_id("left", DeviceKind::Throttle).unwrap(), 0x30);
        assert_eq!(
            resolve_axis_id("right", DeviceKind::Throttle).unwrap(),
            0x31
        );
        assert_eq!(resolve_axis_id("r1", DeviceKind::Throttle).unwrap(), 0x32);
        assert_eq!(
            resolve_axis_id("rotary2", DeviceKind::Throttle).unwrap(),
            0x35
        );

        assert!(resolve_axis_id("invalid", DeviceKind::Stick).is_err());
    }
}
