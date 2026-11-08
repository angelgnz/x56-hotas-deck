//! Calibración del punto cero de los sensores de efecto Hall (Stick X e Y).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{identify_device, CoreError, DeviceKind, X56Device};

/// Resultado del proceso de calibración de un eje del sensor Hall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallCalibrationResult {
    pub axis_id: u8,
    pub axis_name: String,
    pub old_hall_zero: u16,
    pub sampled_hall_zero: u16,
    pub delta: i32,
    pub samples_count: usize,
    pub applied: bool,
}

/// Módulo de calibración física del HOTAS Saitek / Logitech X56.
pub struct Calibrator;

impl Calibrator {
    /// Localiza dinámicamente el nodo `hidraw` correspondiente a la Interfaz 0 (Input) del Stick.
    pub fn find_stick_input_node() -> Option<PathBuf> {
        let sys_hidraw = Path::new("/sys/class/hidraw");
        let entries = fs::read_dir(sys_hidraw).ok()?;

        for entry in entries.flatten() {
            let uevent_path = entry.path().join("device/uevent");
            if let Ok(uevent) = fs::read_to_string(&uevent_path) {
                if let Some((vid, pid)) = Self::parse_uevent_hid_id(&uevent) {
                    if let Some((_, kind)) = identify_device(vid, pid) {
                        if kind == DeviceKind::Stick {
                            if let Some(iface) = Self::find_interface_number(&entry.path()) {
                                if iface == 0 {
                                    return Some(Path::new("/dev").join(entry.file_name()));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Muestrea la posición física central del stick leyendo N paquetes desde la Interfaz 0
    /// sin desconectar el driver del kernel, calculando el promedio para X e Y.
    pub fn sample_stick_center(samples: usize) -> Result<(u16, u16, usize), CoreError> {
        let input_path = Self::find_stick_input_node().ok_or_else(|| {
            CoreError::InvalidColor(
                "no se encontró el nodo hidraw de la Interfaz 0 (Input) del Stick".into(),
            )
        })?;

        let mut file = File::open(&input_path).map_err(|err| {
            CoreError::InvalidColor(format!(
                "error abriendo {} en modo solo lectura: {err}",
                input_path.display()
            ))
        })?;

        let mut sum_x: u64 = 0;
        let mut sum_y: u64 = 0;
        let mut valid_samples: usize = 0;

        let mut buf = [0u8; 64];

        for _ in 0..samples {
            match file.read(&mut buf) {
                Ok(n) if n >= 4 => {
                    let raw_x = u16::from_le_bytes([buf[0], buf[1]]);
                    let raw_y = u16::from_le_bytes([buf[2], buf[3]]);
                    sum_x += raw_x as u64;
                    sum_y += raw_y as u64;
                    valid_samples += 1;
                }
                Ok(_) => {}
                Err(err) => {
                    return Err(CoreError::InvalidColor(format!(
                        "error leyendo muestras de posición: {err}"
                    )));
                }
            }
            thread::sleep(Duration::from_millis(2));
        }

        if valid_samples == 0 {
            return Err(CoreError::InvalidColor(
                "no se pudieron recolectar muestras válidas del sensor".into(),
            ));
        }

        let avg_x = (sum_x / valid_samples as u64) as u16;
        let avg_y = (sum_y / valid_samples as u64) as u16;

        Ok((avg_x, avg_y, valid_samples))
    }

    /// Ejecuta la calibración completa de los sensores Hall para la palanca (Stick).
    ///
    /// Si `dry_run` es `true`, realiza el muestreo y diagnóstico pero no escribe en la EEPROM.
    pub fn calibrate_stick(
        samples: usize,
        dry_run: bool,
    ) -> Result<Vec<HallCalibrationResult>, CoreError> {
        let dev = X56Device::open(DeviceKind::Stick)?;

        // Leer configuración actual de ejes X (0x30) e Y (0x31)
        let old_cfg_x = dev.get_axis_config(0x30)?;
        let old_cfg_y = dev.get_axis_config(0x31)?;

        // Muestrear posición física actual (el stick debe reposar en el centro)
        let (sampled_x, sampled_y, count) = Self::sample_stick_center(samples)?;

        let delta_x = sampled_x as i32 - old_cfg_x.hall_zero as i32;
        let delta_y = sampled_y as i32 - old_cfg_y.hall_zero as i32;

        let mut results = vec![
            HallCalibrationResult {
                axis_id: 0x30,
                axis_name: "X (Roll)".into(),
                old_hall_zero: old_cfg_x.hall_zero,
                sampled_hall_zero: sampled_x,
                delta: delta_x,
                samples_count: count,
                applied: false,
            },
            HallCalibrationResult {
                axis_id: 0x31,
                axis_name: "Y (Pitch)".into(),
                old_hall_zero: old_cfg_y.hall_zero,
                sampled_hall_zero: sampled_y,
                delta: delta_y,
                samples_count: count,
                applied: false,
            },
        ];

        if !dry_run {
            let mut new_cfg_x = old_cfg_x;
            new_cfg_x.hall_zero = sampled_x;

            let mut new_cfg_y = old_cfg_y;
            new_cfg_y.hall_zero = sampled_y;

            // Secuencia de calibración documentada:
            // 1. Aplicar nuevo hall_zero a eje X
            dev.set_axis_config(&new_cfg_x)?;
            // 2. Aplicar nuevo hall_zero a eje Y
            dev.set_axis_config(&new_cfg_y)?;

            results[0].applied = true;
            results[1].applied = true;
        }

        Ok(results)
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

    fn find_interface_number(hidraw_sys: &Path) -> Option<i32> {
        let real_dev = fs::canonicalize(hidraw_sys.join("device")).ok()?;
        let mut curr = Some(real_dev.as_path());
        while let Some(dir) = curr {
            let iface_file = dir.join("bInterfaceNumber");
            if iface_file.is_file() {
                if let Ok(text) = fs::read_to_string(iface_file) {
                    if let Ok(num) = i32::from_str_radix(text.trim(), 16) {
                        return Some(num);
                    }
                }
            }
            curr = dir.parent();
        }
        None
    }
}
