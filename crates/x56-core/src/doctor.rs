//! Sistema de diagnóstico integral (`x56 doctor`) para permisos, udev y conectividad.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{identify_device, DeviceKind, X56Device};

/// Estado de severidad de un chequeo de diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => write!(f, "OK"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
        }
    }
}

/// Resultado de una verificación individual de diagnóstico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub category: String,
    pub title: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

/// Informe consolidado del diagnóstico del sistema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub ok_count: usize,
    pub warn_count: usize,
    pub fail_count: usize,
}

impl DoctorReport {
    pub fn add(&mut self, check: DoctorCheck) {
        match check.status {
            CheckStatus::Ok => self.ok_count += 1,
            CheckStatus::Warn => self.warn_count += 1,
            CheckStatus::Fail => self.fail_count += 1,
        }
        self.checks.push(check);
    }

    pub fn is_healthy(&self) -> bool {
        self.fail_count == 0
    }
}

/// Motor de diagnóstico del sistema `x56 doctor`.
pub struct Doctor;

impl Doctor {
    /// Ejecuta todas las comprobaciones del sistema y genera un [`DoctorReport`].
    pub fn run() -> DoctorReport {
        let mut report = DoctorReport::default();

        Self::check_udev_rules(&mut report);
        Self::check_user_groups(&mut report);
        Self::check_connected_hardware(&mut report);
        Self::check_hidraw_permissions(&mut report);
        Self::check_interface_0_inputs(&mut report);
        Self::check_interface_2_handshake(&mut report);

        report
    }

    fn check_udev_rules(report: &mut DoctorReport) {
        let candidate_paths = [
            Path::new("/etc/udev/rules.d/70-x56.rules"),
            Path::new("/usr/lib/udev/rules.d/70-x56.rules"),
            Path::new("/lib/udev/rules.d/70-x56.rules"),
        ];

        let found_rule = candidate_paths.iter().find(|p| p.is_file());

        match found_rule {
            Some(path) => {
                let content = fs::read_to_string(path).unwrap_or_default();

                if is_udev_rule_complete(&content) {
                    report.add(DoctorCheck {
                        category: "udev".into(),
                        title: "Regla udev para acceso sin root".into(),
                        status: CheckStatus::Ok,
                        message: format!("Regla activa y completa en {}", path.display()),
                        remedy: None,
                    });
                } else {
                    report.add(DoctorCheck {
                        category: "udev".into(),
                        title: "Regla udev incompleta".into(),
                        status: CheckStatus::Warn,
                        message: format!(
                            "Regla en {} carece de soporte para uaccess, VIDs soportados (0738/046d) o regla uinput",
                            path.display()
                        ),
                        remedy: Some(
                            "Reinstala la regla moderna packaging/70-x56.rules en /etc/udev/rules.d/".into(),
                        ),
                    });
                }
            }
            None => {
                report.add(DoctorCheck {
                    category: "udev".into(),
                    title: "Regla udev no encontrada".into(),
                    status: CheckStatus::Fail,
                    message: "No se encontró el archivo 70-x56.rules en las rutas del sistema".into(),
                    remedy: Some(
                        "Copia packaging/70-x56.rules a /etc/udev/rules.d/ y ejecuta: sudo udevadm control --reload-rules && sudo udevadm trigger".into(),
                    ),
                });
            }
        }
    }

    fn check_user_groups(report: &mut DoctorReport) {
        let is_root = unsafe { libc::geteuid() == 0 };
        if is_root {
            report.add(DoctorCheck {
                category: "permisos".into(),
                title: "Ejecución como root".into(),
                status: CheckStatus::Warn,
                message: "Estás ejecutando como root. X56 HOTAS Deck está diseñado para ejecutarse sin privilegios vía uaccess".into(),
                remedy: Some("Ejecuta como usuario estándar para verificar la seguridad del sistema".into()),
            });
        }

        // Comprobar pertenencia al grupo input (red de seguridad headless)
        let in_input_group = Self::is_in_group("input");
        if in_input_group {
            report.add(DoctorCheck {
                category: "permisos".into(),
                title: "Grupo 'input'".into(),
                status: CheckStatus::Ok,
                message:
                    "El usuario actual pertenece al grupo 'input' (acceso headless garantizado)"
                        .into(),
                remedy: None,
            });
        } else {
            report.add(DoctorCheck {
                category: "permisos".into(),
                title: "Grupo 'input'".into(),
                status: CheckStatus::Ok,
                message: "El usuario no está en el grupo 'input', pero uaccess cubre sesiones locales de escritorio".into(),
                remedy: Some("Si usas SSH sin sesión de escritorio, añade tu usuario al grupo: sudo usermod -aG input $USER".into()),
            });
        }
    }

    fn check_connected_hardware(report: &mut DoctorReport) {
        let devices = X56Device::enumerate();
        if devices.is_empty() {
            report.add(DoctorCheck {
                category: "hardware".into(),
                title: "Detección de dispositivos USB".into(),
                status: CheckStatus::Warn,
                message: "No se detectaron periféricos X56 conectados o sus nodos hidraw no son visibles".into(),
                remedy: Some("Verifica la conexión física del Stick y Throttle a puertos USB 2.0/3.0 estables".into()),
            });
            return;
        }

        let has_stick = devices.iter().any(|d| d.kind == DeviceKind::Stick);
        let has_throttle = devices.iter().any(|d| d.kind == DeviceKind::Throttle);

        let summary = format!(
            "Detectados: Stick: {}, Throttle: {}",
            if has_stick {
                "CONECTADO"
            } else {
                "NO DETECTADO"
            },
            if has_throttle {
                "CONECTADO"
            } else {
                "NO DETECTADO"
            }
        );

        report.add(DoctorCheck {
            category: "hardware".into(),
            title: "Periféricos HOTAS detectados".into(),
            status: CheckStatus::Ok,
            message: summary,
            remedy: None,
        });
    }

    fn check_hidraw_permissions(report: &mut DoctorReport) {
        let sys_hidraw = Path::new("/sys/class/hidraw");
        if !sys_hidraw.is_dir() {
            report.add(DoctorCheck {
                category: "kernel".into(),
                title: "Soporte hidraw en el kernel".into(),
                status: CheckStatus::Fail,
                message: "No se encontró el directorio /sys/class/hidraw".into(),
                remedy: Some(
                    "Asegúrate de que el módulo 'hidraw' esté cargado en el kernel".into(),
                ),
            });
            return;
        }

        let mut checked_nodes = 0;
        let mut denied_nodes = Vec::new();

        if let Ok(entries) = fs::read_dir(sys_hidraw) {
            for entry in entries.flatten() {
                let dev_name = entry.file_name();
                let dev_path = Path::new("/dev").join(&dev_name);
                let uevent_path = entry.path().join("device/uevent");

                if let Ok(uevent) = fs::read_to_string(&uevent_path) {
                    if let Some((vid, pid)) = Self::parse_uevent_hid_id(&uevent) {
                        if identify_device(vid, pid).is_some() {
                            checked_nodes += 1;
                            // Intentar abrir R/W
                            match fs::OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open(&dev_path)
                            {
                                Ok(_) => {}
                                Err(err) => {
                                    denied_nodes
                                        .push((dev_path.display().to_string(), err.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if checked_nodes == 0 {
            return;
        }

        if denied_nodes.is_empty() {
            report.add(DoctorCheck {
                category: "hidraw".into(),
                title: "Permisos de lectura y escritura".into(),
                status: CheckStatus::Ok,
                message: format!("Acceso concedido sin root en todos los nodos hidraw ({checked_nodes} nodos verificados)"),
                remedy: None,
            });
        } else {
            report.add(DoctorCheck {
                category: "hidraw".into(),
                title: "Permisos insuficientes en hidraw".into(),
                status: CheckStatus::Fail,
                message: format!("Permiso denegado en: {:?}", denied_nodes),
                remedy: Some(
                    "Verifica la regla udev 70-x56.rules y reconecta los dispositivos USB".into(),
                ),
            });
        }
    }

    fn check_interface_0_inputs(report: &mut DoctorReport) {
        let input_dir = Path::new("/dev/input");
        let has_js = fs::read_dir(input_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("js"))
            })
            .unwrap_or(false);

        if has_js {
            report.add(DoctorCheck {
                category: "simulacion".into(),
                title: "Driver de joystick del kernel (Interfaz 0)".into(),
                status: CheckStatus::Ok,
                message: "Nodos /dev/input/js* presentes; simuladores de vuelo (DCS, X-Plane) recibirán ejes continuamente".into(),
                remedy: None,
            });
        } else {
            report.add(DoctorCheck {
                category: "simulacion".into(),
                title: "Driver de joystick del kernel (Interfaz 0)".into(),
                status: CheckStatus::Warn,
                message: "No se detectaron nodos /dev/input/js*".into(),
                remedy: Some("Asegúrate de que el módulo 'joydev' del kernel esté cargado si usas juegos clásicos".into()),
            });
        }
    }

    fn check_interface_2_handshake(report: &mut DoctorReport) {
        let devices = X56Device::enumerate();
        let mut handshake_ok = 0;

        for desc in &devices {
            if let Ok(dev) = X56Device::open_path(&desc.path) {
                if dev.get_axis_config(0x30).is_ok() {
                    handshake_ok += 1;
                }
            }
        }

        if handshake_ok > 0 {
            report.add(DoctorCheck {
                category: "protocolo".into(),
                title: "Handshake con firmware X56 (Interfaz 2)".into(),
                status: CheckStatus::Ok,
                message: format!(
                    "Comunicación bidireccional confirmada en {handshake_ok} dispositivo(s)"
                ),
                remedy: None,
            });
        }
    }

    fn parse_uevent_hid_id(uevent: &str) -> Option<(u16, u16)> {
        for line in uevent.lines() {
            if let Some(rest) = line.strip_prefix("HID_ID=") {
                let parts: Vec<&str> = rest.split(':').collect();
                if parts.len() == 3 {
                    let vid = u16::from_str_radix(parts[1], 16).ok()?;
                    let pid = u16::from_str_radix(parts[2], 16).ok()?;
                    return Some((vid, pid));
                }
            }
        }
        None
    }

    fn is_in_group(group_name: &str) -> bool {
        let ngroups = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        if ngroups <= 0 {
            return false;
        }

        let mut groups = vec![0 as libc::gid_t; ngroups as usize];
        let count = unsafe { libc::getgroups(ngroups, groups.as_mut_ptr()) };
        if count <= 0 {
            return false;
        }

        let c_group = std::ffi::CString::new(group_name).unwrap();
        let grp = unsafe { libc::getgrnam(c_group.as_ptr()) };
        if grp.is_null() {
            return false;
        }

        let target_gid = unsafe { (*grp).gr_gid };
        groups.contains(&target_gid)
    }
}

fn is_udev_rule_complete(content: &str) -> bool {
    let has_uaccess = content.contains("uaccess");
    let has_saitek = content.contains("0738");
    let has_logitech = content.contains("046d");
    let has_uinput = content.contains("uinput");
    has_uaccess && has_saitek && has_logitech && has_uinput
}

#[cfg(test)]
mod tests {
    use super::is_udev_rule_complete;

    #[test]
    fn udev_rule_completeness_requires_uaccess_vids_and_uinput() {
        let complete = r#"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0738", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", TAG+="uaccess"
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess"
"#;
        assert!(is_udev_rule_complete(complete));

        let missing_uinput = r#"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0738", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", TAG+="uaccess"
"#;
        assert!(!is_udev_rule_complete(missing_uinput));

        let missing_uaccess = r#"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0738", MODE="0666"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", MODE="0666"
KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0666"
"#;
        assert!(!is_udev_rule_complete(missing_uaccess));

        let missing_logitech = r#"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0738", TAG+="uaccess"
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess"
"#;
        assert!(!is_udev_rule_complete(missing_logitech));

        assert!(!is_udev_rule_complete(""));
    }
}
