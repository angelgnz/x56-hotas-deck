//! Cálculo matemático de curvas de respuesta (S/J), zonas muertas y lectura en vivo de ejes.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use x56_core::{identify_device, DeviceKind};
use x56_proto::CurveProfile;

/// Parámetros de curva de un eje.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveParameters {
    pub xsat: u16,      // 0..=1000
    pub ysat: u16,      // 0..=1000
    pub deadband: u16,  // 0..=1000
    pub curvature: u16, // 0..=1000 (500 = lineal)
    pub profile: CurveProfile,
}

impl Default for CurveParameters {
    fn default() -> Self {
        Self {
            xsat: 1000,
            ysat: 1000,
            deadband: 50,
            curvature: 500,
            profile: CurveProfile::S,
        }
    }
}

impl CurveParameters {
    /// Evalúa la función de respuesta $y = f(x)$ para un valor de entrada normalizado $x \in [-1.0, 1.0]$
    /// (o $[0.0, 1.0]$ para aceleradores unidireccionales).
    pub fn evaluate(&self, x: f64) -> f64 {
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let abs_x = x.abs();

        let deadband_norm = (self.deadband as f64) / 1000.0;
        let xsat_norm = ((self.xsat as f64) / 1000.0).max(deadband_norm + 0.01);
        let ysat_norm = (self.ysat as f64) / 1000.0;

        // Zona muerta central (o en ralentí para gases)
        if abs_x <= deadband_norm {
            return 0.0;
        }

        // Re-escalar rango activo entre deadband y xsat
        let active_x = ((abs_x - deadband_norm) / (xsat_norm - deadband_norm)).clamp(0.0, 1.0);

        // Exponente de curvatura: 500 -> 1.0 (lineal)
        // > 500 -> exponencial (suave al centro, agresivo al extremo)
        // < 500 -> logarítmico (agresivo al centro)
        let c = (self.curvature as f64) / 1000.0;
        let exponent = if c >= 0.5 {
            1.0 + (c - 0.5) * 4.0 // 1.0 a 3.0
        } else {
            1.0 / (1.0 + (0.5 - c) * 4.0) // 0.33 a 1.0
        };

        let curved = match self.profile {
            CurveProfile::S => active_x.powf(exponent),
            CurveProfile::J => {
                // Curva progresiva en J para gases o cabeceo asimétrico
                active_x.powf(exponent * 1.25)
            }
        };

        let y = (curved * ysat_norm).clamp(0.0, 1.0);
        sign * y
    }

    /// Genera una serie de puntos $[x, y]$ para graficar la curva en `egui_plot`.
    pub fn generate_plot_points(&self, is_bipolar: bool, resolution: usize) -> Vec<[f64; 2]> {
        let mut points = Vec::with_capacity(resolution);

        let (min_x, max_x) = if is_bipolar { (-1.0, 1.0) } else { (0.0, 1.0) };

        let step = (max_x - min_x) / (resolution - 1) as f64;
        for i in 0..resolution {
            let x = min_x + (i as f64) * step;
            let y = self.evaluate(x);
            points.push([x, y]);
        }

        points
    }
}

/// Estado en vivo de los ejes físicos leídos desde los dispositivos de entrada (evdev / hidraw).
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveAxesState {
    // Palanca (Stick)
    pub stick_x: f64, // -1.0 .. 1.0 (Roll)
    pub stick_y: f64, // -1.0 .. 1.0 (Pitch)
    pub stick_z: f64, // -1.0 .. 1.0 (Twist / Yaw)

    // Cuadrante de Gases (Throttle)
    pub throttle_left: f64,  // 0.0 .. 1.0 (Unipolar)
    pub throttle_right: f64, // 0.0 .. 1.0 (Unipolar)
    pub rotary_1: f64,       // -1.0 .. 1.0 (Rotary 1 - 0x32)
    pub rotary_2: f64,       // -1.0 .. 1.0 (Rotary 2 - 0x35)
    pub rotary_3: f64,       // -1.0 .. 1.0 (Rotary 3 - 0x37)
    pub rotary_4: f64,       // -1.0 .. 1.0 (Rotary 4 - 0x36)

    #[allow(dead_code)]
    pub last_updated_ms: u64,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct InputAbsInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

// In Linux: _IOR('E', 0x40 + abs, struct input_absinfo) = 0x80184540 + abs
const EVIOCGABS_BASE: u64 = 0x8018_4540;

pub fn query_abs_axis(file: &File, abs_code: u32) -> Option<InputAbsInfo> {
    use std::os::unix::io::AsRawFd;
    let mut info = InputAbsInfo::default();
    let request = EVIOCGABS_BASE + (abs_code as u64);
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), request, &mut info) };
    if ret == 0 {
        Some(info)
    } else {
        None
    }
}

pub fn find_evdev_node(target_kind: DeviceKind) -> Option<PathBuf> {
    let sys_input = Path::new("/sys/class/input");
    let entries = std::fs::read_dir(sys_input).ok()?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("event") {
            continue;
        }

        let dev_dir = entry.path().join("device/id");
        let vendor_path = dev_dir.join("vendor");
        let product_path = dev_dir.join("product");

        if let (Ok(v_str), Ok(p_str)) = (
            std::fs::read_to_string(&vendor_path),
            std::fs::read_to_string(&product_path),
        ) {
            if let (Ok(vid), Ok(pid)) = (
                u16::from_str_radix(v_str.trim(), 16),
                u16::from_str_radix(p_str.trim(), 16),
            ) {
                if let Some((_, kind)) = identify_device(vid, pid) {
                    if kind == target_kind {
                        return Some(Path::new("/dev/input").join(name));
                    }
                }
            }
        }
    }
    None
}

/// Lector en segundo plano de las entradas de hardware (Stick y Throttle).
pub struct LiveInputPoller {
    state: Arc<Mutex<LiveAxesState>>,
    running: Arc<AtomicBool>,
}

impl Default for LiveInputPoller {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveInputPoller {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(LiveAxesState::default()));
        let running = Arc::new(AtomicBool::new(true));

        let state_clone = Arc::clone(&state);
        let running_clone = Arc::clone(&running);

        thread::Builder::new()
            .name("x56-live-input".into())
            .spawn(move || {
                Self::polling_loop(state_clone, running_clone);
            })
            .ok();

        Self { state, running }
    }

    pub fn get_state(&self) -> LiveAxesState {
        self.state.lock().map(|s| *s).unwrap_or_default()
    }

    fn polling_loop(state: Arc<Mutex<LiveAxesState>>, running: Arc<AtomicBool>) {
        let mut stick_file: Option<File> = None;
        let mut throttle_file: Option<File> = None;

        while running.load(Ordering::Relaxed) {
            // Reintentar abrir nodos si se desconectaron o aún no estaban abiertos
            if stick_file.is_none() {
                if let Some(path) = find_evdev_node(DeviceKind::Stick) {
                    stick_file = File::open(path).ok();
                }
            }
            if throttle_file.is_none() {
                if let Some(path) = find_evdev_node(DeviceKind::Throttle) {
                    throttle_file = File::open(path).ok();
                }
            }

            let mut new_stick_x = None;
            let mut new_stick_y = None;
            let mut new_stick_z = None;

            let mut new_tl = None;
            let mut new_tr = None;
            let mut new_r1 = None;
            let mut new_r2 = None;
            let mut new_r3 = None;
            let mut new_r4 = None;

            // Muestreo del Stick (evdev / EVIOCGABS)
            if let Some(ref file) = stick_file {
                if let Some(info) = query_abs_axis(file, 0) {
                    // ABS_X (Roll 0..65535, centro ~32768)
                    new_stick_x =
                        Some((((info.value as f64) - 32768.0) / 32768.0).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 1) {
                    // ABS_Y (Pitch 0..65535, centro ~32768, invertido)
                    new_stick_y =
                        Some((-(((info.value as f64) - 32768.0) / 32768.0)).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 5) {
                    // ABS_RZ (Twist / Yaw 0..4095, centro ~2048)
                    new_stick_z = Some((((info.value as f64) - 2048.0) / 2048.0).clamp(-1.0, 1.0));
                }
            }

            // Muestreo del Throttle (evdev / EVIOCGABS)
            if let Some(ref file) = throttle_file {
                if let Some(info) = query_abs_axis(file, 0) {
                    // ABS_X -> Throttle Left (0..1023)
                    new_tl = Some(((info.value as f64) / 1023.0).clamp(0.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 1) {
                    // ABS_Y -> Throttle Right (0..1023)
                    new_tr = Some(((info.value as f64) / 1023.0).clamp(0.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 2) {
                    // ABS_Z -> Rotary 1 (0..255, centro 128)
                    new_r1 = Some((((info.value as f64) - 128.0) / 128.0).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 5) {
                    // ABS_RZ -> Rotary 2 (0..255, centro 128)
                    new_r2 = Some((((info.value as f64) - 128.0) / 128.0).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 7) {
                    // ABS_RUDDER -> Rotary 3 (0..255, centro 128)
                    new_r3 = Some((((info.value as f64) - 128.0) / 128.0).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 6) {
                    // ABS_THROTTLE -> Rotary 4 (0..255, centro 128)
                    new_r4 = Some((((info.value as f64) - 128.0) / 128.0).clamp(-1.0, 1.0));
                }
            }

            if let Ok(mut lock) = state.lock() {
                if let Some(v) = new_stick_x {
                    lock.stick_x = v;
                }
                if let Some(v) = new_stick_y {
                    lock.stick_y = v;
                }
                if let Some(v) = new_stick_z {
                    lock.stick_z = v;
                }

                if let Some(v) = new_tl {
                    lock.throttle_left = v;
                }
                if let Some(v) = new_tr {
                    lock.throttle_right = v;
                }
                if let Some(v) = new_r1 {
                    lock.rotary_1 = v;
                }
                if let Some(v) = new_r2 {
                    lock.rotary_2 = v;
                }
                if let Some(v) = new_r3 {
                    lock.rotary_3 = v;
                }
                if let Some(v) = new_r4 {
                    lock.rotary_4 = v;
                }
            }

            thread::sleep(Duration::from_millis(20)); // Muestreo constante a 50 Hz
        }
    }
}

impl Drop for LiveInputPoller {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_curve_evaluation() {
        let p = CurveParameters {
            xsat: 1000,
            ysat: 1000,
            deadband: 0,
            curvature: 500,
            profile: CurveProfile::S,
        };

        assert_eq!(p.evaluate(0.0), 0.0);
        assert!((p.evaluate(0.5) - 0.5).abs() < 1e-4);
        assert!((p.evaluate(1.0) - 1.0).abs() < 1e-4);
        assert!((p.evaluate(-0.5) - -0.5).abs() < 1e-4);
        assert!((p.evaluate(-1.0) - -1.0).abs() < 1e-4);
    }

    #[test]
    fn deadband_filtering() {
        let p = CurveParameters {
            xsat: 1000,
            ysat: 1000,
            deadband: 200, // 20%
            curvature: 500,
            profile: CurveProfile::S,
        };

        assert_eq!(p.evaluate(0.1), 0.0);
        assert_eq!(p.evaluate(-0.19), 0.0);
        assert!(p.evaluate(0.25) > 0.0);
    }

    #[test]
    fn throttle_unipolar_evaluation() {
        let p = CurveParameters {
            xsat: 1000,
            ysat: 1000,
            deadband: 0,
            curvature: 500,
            profile: CurveProfile::J,
        };

        assert_eq!(p.evaluate(0.0), 0.0);
        assert_eq!(p.evaluate(1.0), 1.0);
        assert!(p.evaluate(0.5) > 0.0 && p.evaluate(0.5) < 1.0);
    }
}
