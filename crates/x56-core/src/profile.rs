//! Gestión y serialización de perfiles de configuración para Saitek / Logitech X56.

use std::fs;
use std::path::{Path, PathBuf};

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

    /// Resuelve el color RGB efectivo para el modo y dispositivo indicados.
    /// Prioriza:
    /// 1. Color específico del dispositivo en ese modo (`programming.<mode>.stick_rgb` o `throttle_rgb`)
    /// 2. Color general de ese modo (`programming.<mode>.rgb`)
    /// 3. Color global del dispositivo (`[stick].rgb` o `[throttle].rgb`)
    pub fn effective_rgb_for_mode(
        &self,
        mode: crate::buttons::ModeSlot,
        device: DeviceKind,
    ) -> Option<&str> {
        if let Some(prog) = &self.programming {
            if let Some(color) = prog.rgb_for_mode(mode, device) {
                return Some(color);
            }
        }
        match device {
            DeviceKind::Stick => self.stick.as_ref().and_then(|s| s.rgb.as_deref()),
            DeviceKind::Throttle => self.throttle.as_ref().and_then(|t| t.rgb.as_deref()),
        }
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
        let current_mode =
            crate::buttons::detect_current_mode().unwrap_or(crate::buttons::ModeSlot::M1);

        // Aplicar a Stick
        if let Some(ref stick_prof) = profile.stick {
            if let Ok(dev) = X56Device::open(DeviceKind::Stick) {
                let rgb_str = profile.effective_rgb_for_mode(current_mode, DeviceKind::Stick);
                if let Some(rgb_str) = rgb_str {
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
                let rgb_str = profile.effective_rgb_for_mode(current_mode, DeviceKind::Throttle);
                if let Some(rgb_str) = rgb_str {
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

        // Si no se actualizaron ejes para alguno pero sí existen colores en [programming]:
        if !summary.rgb_updated {
            if let Some(rgb_str) = profile.effective_rgb_for_mode(current_mode, DeviceKind::Stick) {
                if let Ok(color) = parse_color(rgb_str) {
                    if let Ok(dev) = X56Device::open(DeviceKind::Stick) {
                        let _ = dev.set_rgb(color);
                        summary.rgb_updated = true;
                    }
                }
            }
            if let Some(rgb_str) =
                profile.effective_rgb_for_mode(current_mode, DeviceKind::Throttle)
            {
                if let Ok(color) = parse_color(rgb_str) {
                    if let Ok(dev) = X56Device::open(DeviceKind::Throttle) {
                        let _ = dev.set_rgb(color);
                        summary.rgb_updated = true;
                    }
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

/// Configuración persistente de sesión de usuario (`~/.config/x56linux/session.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Ruta absoluta o relativa del último perfil `.toml` cargado o guardado.
    pub last_profile_path: Option<String>,
    /// Indica si el router `uinput` debe activarse automáticamente al iniciar.
    pub uinput_active: bool,
    /// Indica si al cerrar la ventana se minimiza a la bandeja del sistema.
    pub minimize_to_tray_on_close: bool,
    /// Idioma de la interfaz (`"es-ES"` o `"en-US"`).
    pub language: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            last_profile_path: None,
            uinput_active: true,
            minimize_to_tray_on_close: true,
            language: None,
        }
    }
}

impl SessionConfig {
    /// Directorio de configuración XDG del usuario (`~/.config/x56linux`).
    pub fn config_dir() -> Option<PathBuf> {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("x56linux"));
            }
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("x56linux"))
    }

    /// Ruta del archivo de sesión persistente (`~/.config/x56linux/session.toml`).
    pub fn session_file_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("session.toml"))
    }

    /// Ruta predeterminada del perfil de usuario (`~/.config/x56linux/default_profile.toml`).
    pub fn default_user_profile_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("default_profile.toml"))
    }

    /// Carga una configuración de sesión desde una ruta explícita.
    pub fn load_from_path(path: &Path) -> Option<Self> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
    }

    /// Guarda la configuración de sesión en la ruta especificada, creando directorios padres.
    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, content)
    }

    /// Carga la configuración de sesión de usuario o devuelve los valores por defecto.
    pub fn load() -> Self {
        Self::session_file_path()
            .and_then(|p| Self::load_from_path(&p))
            .unwrap_or_default()
    }

    /// Persiste la configuración de sesión en `~/.config/x56linux/session.toml`.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = Self::session_file_path() {
            self.save_to_path(&path)
        } else {
            Ok(())
        }
    }

    /// Resuelve la ruta del perfil inicial que debe cargarse al abrir la aplicación.
    pub fn resolve_initial_profile_path(&self) -> PathBuf {
        if let Some(ref last) = self.last_profile_path {
            if !last.trim().is_empty() && Path::new(last).exists() {
                return PathBuf::from(last);
            }
        }

        for candidate in [
            "examples/elite_dangerous_profile.toml",
            "examples/combat_profile.toml",
            "examples/default_profile.toml",
        ] {
            if Path::new(candidate).exists() {
                return PathBuf::from(candidate);
            }
        }

        if let Some(user_def) = Self::default_user_profile_path() {
            if user_def.exists() {
                return user_def;
            }
            let system_example = Path::new("/usr/share/x56linux/examples/combat_profile.toml");
            if system_example.exists() {
                if let Some(parent) = user_def.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                let _ = fs::copy(system_example, &user_def);
                return user_def;
            }
            return user_def;
        }

        PathBuf::from("examples/default_profile.toml")
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

    #[test]
    fn session_config_roundtrip_and_resolution() {
        let unique = format!(
            "x56linux_session_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let temp_dir = std::env::temp_dir().join(unique);
        let session_path = temp_dir.join("session.toml");
        let profile_path = temp_dir.join("my_profile.toml");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(&profile_path, "name = \"Test\"\n").unwrap();

        let cfg = SessionConfig {
            last_profile_path: Some(profile_path.display().to_string()),
            uinput_active: false,
            minimize_to_tray_on_close: false,
            language: Some("en-US".into()),
        };
        cfg.save_to_path(&session_path).unwrap();

        let loaded = SessionConfig::load_from_path(&session_path).unwrap();
        assert_eq!(loaded, cfg);
        assert_eq!(loaded.resolve_initial_profile_path(), profile_path);

        let missing_cfg = SessionConfig {
            last_profile_path: Some(temp_dir.join("missing.toml").display().to_string()),
            ..SessionConfig::default()
        };
        assert_ne!(
            missing_cfg.resolve_initial_profile_path(),
            temp_dir.join("missing.toml")
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
