//! Gestión y serialización de perfiles de configuración para Saitek / Logitech X56.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use x56_proto::{AxisConfig, CurveProfile, RgbColor};

use crate::{
    axis_name, parse_color, CoreError, DeviceKind, HidTransportError, ProgrammingProfile,
    TargetDevice, X56Device,
};

/// Configuración de un eje dentro de un perfil persistente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AxisProfileEntry {
    /// Nombre descriptivo del eje (ej: "X (Roll)", "Throttle Left").
    pub name: String,
    /// ID numérico del protocolo (ej: 0x30 / 48).
    pub axis_id: u8,
    /// Saturación X (0..=1000).
    pub xsat: u16,
    /// Saturación Y (0..=1000).
    pub ysat: u16,
    /// Zona muerta (0..=1000).
    pub deadband: u16,
    /// Curvatura de respuesta (0..=1000, 500 = lineal).
    pub curvature: u16,
    /// Perfil de curva ("s" simétrica o "j" asimétrica/progresiva).
    pub profile: String,
    /// Calibración de punto cero del sensor Hall (si aplica).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hall_zero: Option<u16>,
}

impl AxisProfileEntry {
    pub fn from_axis_config(cfg: &AxisConfig, kind: DeviceKind) -> Self {
        Self {
            name: axis_name(cfg.axis_id, kind).to_string(),
            axis_id: cfg.axis_id,
            xsat: cfg.xsat,
            ysat: cfg.ysat,
            deadband: cfg.deadband,
            curvature: cfg.curvature,
            profile: match cfg.profile {
                CurveProfile::S => "s".to_string(),
                CurveProfile::J => "j".to_string(),
            },
            hall_zero: if cfg.hall_zero > 0 {
                Some(cfg.hall_zero)
            } else {
                None
            },
        }
    }

    pub fn to_axis_config(&self, current_hall_zero: u16) -> Result<AxisConfig, CoreError> {
        let profile = match self.profile.to_ascii_lowercase().as_str() {
            "s" | "simetrica" | "symmetric" => CurveProfile::S,
            "j" | "asimetrica" | "asymmetric" => CurveProfile::J,
            other => {
                return Err(CoreError::Proto(x56_proto::ProtoError::InvalidProfile(
                    if other == "1" { 1 } else { 255 },
                )))
            }
        };

        let hall_zero = self.hall_zero.unwrap_or(current_hall_zero);

        let cfg = AxisConfig::new(
            self.axis_id,
            self.xsat,
            self.ysat,
            self.deadband,
            self.curvature,
            profile,
            hall_zero,
        )?;

        Ok(cfg)
    }
}

/// Perfil de configuración de un dispositivo individual (Stick o Throttle).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeviceProfile {
    /// Color de iluminación LED en formato hex (ej: "#00ffff").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rgb: Option<String>,
    /// Muelle físico instalado en el Stick ("red", "green", "yellow", "blue").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<String>,
    /// Lista de configuraciones para cada eje disponible.
    #[serde(default)]
    pub axes: Vec<AxisProfileEntry>,
}

/// Perfil completo del HOTAS Saitek / Logitech X56 en formato TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FullProfile {
    /// Versión del esquema del archivo de perfil (actual: 1).
    pub version: u32,
    /// Nombre descriptivo del perfil.
    pub name: String,
    /// Descripción o notas del usuario.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Perfil para la palanca (Stick).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stick: Option<DeviceProfile>,
    /// Perfil para el cuadrante de gases (Throttle).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle: Option<DeviceProfile>,
    /// Asignación de botones por modo (`M1`, `M2`, `S1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub programming: Option<ProgrammingProfile>,
}

impl FullProfile {
    /// Crea un perfil vacío con valores de versión estándar.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            description: None,
            stick: None,
            throttle: None,
            programming: None,
        }
    }

    /// Serializa el perfil a formato de texto TOML.
    pub fn to_toml(&self) -> Result<String, CoreError> {
        toml::to_string_pretty(self)
            .map_err(|err| CoreError::InvalidColor(format!("error serializing toml: {err}")))
    }

    /// Parsea un perfil desde una cadena TOML.
    pub fn from_toml(s: &str) -> Result<Self, CoreError> {
        toml::from_str(s)
            .map_err(|err| CoreError::InvalidColor(format!("error parsing toml profile: {err}")))
    }

    /// Guarda el perfil en un archivo en disco.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let content = self.to_toml()?;
        fs::write(path.as_ref(), content).map_err(|err| {
            CoreError::InvalidColor(format!("error writing file {:?}: {err}", path.as_ref()))
        })?;
        Ok(())
    }

    /// Carga un perfil desde un archivo en disco.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|err| {
            CoreError::InvalidColor(format!("error reading file {:?}: {err}", path.as_ref()))
        })?;
        Self::from_toml(&content)
    }
}

/// Resumen de aplicación de un perfil al hardware.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProfileApplySummary {
    pub stick_axes_updated: usize,
    pub throttle_axes_updated: usize,
    pub rgb_updated: bool,
}

/// Gestor de operaciones de alto nivel sobre perfiles.
pub struct ProfileManager;

impl ProfileManager {
    /// Lista de ejes canónicos consultados para el Stick.
    pub const STICK_AXES: &'static [u8] = &[0x30, 0x31, 0x35];

    /// Lista de ejes canónicos consultados para el Throttle.
    pub const THROTTLE_AXES: &'static [u8] = &[0x30, 0x31, 0x32, 0x35, 0x36, 0x37];

    /// Vuelca el estado actual del hardware conectado en un [`FullProfile`].
    pub fn dump_current(
        profile_name: &str,
        stick_rgb: Option<&str>,
        throttle_rgb: Option<&str>,
    ) -> Result<FullProfile, CoreError> {
        let mut profile = FullProfile::new(profile_name);

        // Volcar Stick si está conectado
        if let Ok(dev) = X56Device::open(DeviceKind::Stick) {
            let mut stick_profile = DeviceProfile {
                rgb: stick_rgb.map(str::to_string),
                spring: None,
                axes: Vec::new(),
            };
            for &axis_id in Self::STICK_AXES {
                if let Ok(cfg) = dev.get_axis_config(axis_id) {
                    stick_profile
                        .axes
                        .push(AxisProfileEntry::from_axis_config(&cfg, DeviceKind::Stick));
                }
            }
            profile.stick = Some(stick_profile);
        }

        // Volcar Throttle si está conectado
        if let Ok(dev) = X56Device::open(DeviceKind::Throttle) {
            let mut throttle_profile = DeviceProfile {
                rgb: throttle_rgb.map(str::to_string),
                spring: None,
                axes: Vec::new(),
            };
            for &axis_id in Self::THROTTLE_AXES {
                if let Ok(cfg) = dev.get_axis_config(axis_id) {
                    throttle_profile
                        .axes
                        .push(AxisProfileEntry::from_axis_config(
                            &cfg,
                            DeviceKind::Throttle,
                        ));
                }
            }
            profile.throttle = Some(throttle_profile);
        }

        Ok(profile)
    }

    /// Aplica un perfil completo a los dispositivos físicos conectados.
    pub fn apply(profile: &FullProfile) -> Result<ProfileApplySummary, CoreError> {
        let mut summary = ProfileApplySummary::default();

        // Aplicar a Stick
        if let Some(ref stick_prof) = profile.stick {
            if let Ok(dev) = X56Device::open(DeviceKind::Stick) {
                if let Some(ref rgb_str) = stick_prof.rgb {
                    if let Ok(color) = parse_color(rgb_str) {
                        let _ = dev.set_rgb(color);
                        summary.rgb_updated = true;
                    }
                }

                for axis_entry in &stick_prof.axes {
                    let current_hall = dev
                        .get_axis_config(axis_entry.axis_id)
                        .map(|c| c.hall_zero)
                        .unwrap_or(0);
                    let cfg = axis_entry.to_axis_config(current_hall)?;
                    dev.set_axis_config(&cfg)?;
                    summary.stick_axes_updated += 1;
                }
            }
        }

        // Aplicar a Throttle
        if let Some(ref thr_prof) = profile.throttle {
            if let Ok(dev) = X56Device::open(DeviceKind::Throttle) {
                if let Some(ref rgb_str) = thr_prof.rgb {
                    if let Ok(color) = parse_color(rgb_str) {
                        let _ = dev.set_rgb(color);
                        summary.rgb_updated = true;
                    }
                }

                for axis_entry in &thr_prof.axes {
                    let current_hall = dev
                        .get_axis_config(axis_entry.axis_id)
                        .map(|c| c.hall_zero)
                        .unwrap_or(0);
                    let cfg = axis_entry.to_axis_config(current_hall)?;
                    dev.set_axis_config(&cfg)?;
                    summary.throttle_axes_updated += 1;
                }
            }
        }

        Ok(summary)
    }

    /// Restaura los valores de fábrica predeterminados del hardware.
    pub fn reset_defaults(target: TargetDevice) -> Result<(), CoreError> {
        let default_rgb = RgbColor::new(0, 255, 255); // Cyan de fábrica

        let targets = match target {
            TargetDevice::Stick => vec![DeviceKind::Stick],
            TargetDevice::Throttle => vec![DeviceKind::Throttle],
            TargetDevice::All => vec![DeviceKind::Stick, DeviceKind::Throttle],
        };

        for kind in targets {
            match kind {
                DeviceKind::Stick => {
                    if let Ok(dev) = X56Device::open(DeviceKind::Stick) {
                        let _ = dev.set_rgb(default_rgb);
                        // Ejes de fábrica del Stick:
                        // X (0x30): xsat 1000, ysat 1000, deadband 60, curvatura 500, S
                        // Y (0x31): xsat 1000, ysat 1000, deadband 50, curvatura 500, S
                        // Z (0x35): xsat 1000, ysat 1000, deadband 0, curvatura 500, S
                        let stick_specs = [(0x30u8, 60u16), (0x31, 50), (0x35, 0)];
                        for (axis_id, deadband) in stick_specs {
                            if let Ok(mut cfg) = dev.get_axis_config(axis_id) {
                                cfg.xsat = 1000;
                                cfg.ysat = 1000;
                                cfg.deadband = deadband;
                                cfg.curvature = 500;
                                cfg.profile = CurveProfile::S;
                                let _ = dev.set_axis_config(&cfg);
                            }
                        }
                    } else if target != TargetDevice::All {
                        return Err(CoreError::Transport(HidTransportError::DeviceNotFound(
                            DeviceKind::Stick,
                        )));
                    }
                }
                DeviceKind::Throttle => {
                    if let Ok(dev) = X56Device::open(DeviceKind::Throttle) {
                        let _ = dev.set_rgb(default_rgb);
                        // Ejes de fábrica del Throttle:
                        // Left (0x30) y Right (0x31): deadband 0, curve 500, J
                        // Rotaries (0x32, 0x35, 0x36, 0x37): deadband 0, curve 500, S
                        let throttle_specs = [
                            (0x30u8, CurveProfile::J),
                            (0x31, CurveProfile::J),
                            (0x32, CurveProfile::S),
                            (0x35, CurveProfile::S),
                            (0x36, CurveProfile::S),
                            (0x37, CurveProfile::S),
                        ];
                        for (axis_id, profile) in throttle_specs {
                            if let Ok(mut cfg) = dev.get_axis_config(axis_id) {
                                cfg.xsat = 1000;
                                cfg.ysat = 1000;
                                cfg.deadband = 0;
                                cfg.curvature = 500;
                                cfg.profile = profile;
                                let _ = dev.set_axis_config(&cfg);
                            }
                        }
                    } else if target != TargetDevice::All {
                        return Err(CoreError::Transport(HidTransportError::DeviceNotFound(
                            DeviceKind::Throttle,
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_profile_roundtrip() {
        let mut prof = FullProfile::new("DCS F/A-18C Hornet");
        prof.description = Some("Perfil de alta precisión para combate aéreo".into());

        let stick = DeviceProfile {
            rgb: Some("#00ffff".into()),
            spring: Some("green".into()),
            axes: vec![
                AxisProfileEntry {
                    name: "X (Roll)".into(),
                    axis_id: 0x30,
                    xsat: 1000,
                    ysat: 1000,
                    deadband: 60,
                    curvature: 500,
                    profile: "s".into(),
                    hall_zero: Some(0x8859),
                },
                AxisProfileEntry {
                    name: "Y (Pitch)".into(),
                    axis_id: 0x31,
                    xsat: 1000,
                    ysat: 1000,
                    deadband: 50,
                    curvature: 500,
                    profile: "s".into(),
                    hall_zero: Some(0x83f7),
                },
            ],
        };

        prof.stick = Some(stick);

        let toml_str = prof.to_toml().unwrap();
        assert!(toml_str.contains("DCS F/A-18C Hornet"));
        assert!(toml_str.contains("0x30") || toml_str.contains("48"));

        let loaded = FullProfile::from_toml(&toml_str).unwrap();
        assert_eq!(prof, loaded);
    }
}
