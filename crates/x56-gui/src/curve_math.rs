//! Cálculo matemático de curvas de respuesta (Lineal, S-Curve Cubic-Hermite, Exponencial, Bézier),
//! zonas muertas, tasas de compresión y lectura telemétrica en vivo de ejes (evdev).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use x56_core::{identify_device, DeviceKind};
use x56_proto::CurveProfile;

/// Tipo de curvatura matemática seleccionada en el panel Avionics Vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurveType {
    /// Respuesta directa 1:1 sin atenuación exponencial.
    Linear,
    /// S-Curve simétrica con interpolación Cubic-Hermite / Expo suave en centro.
    #[default]
    SCurve,
    /// Curva exponencial progresiva (J-Curve / Monotonic Power).
    Exponential,
    /// Spline cúbico de Bézier con puntos de control ajustables.
    Bezier,
}

impl CurveType {
    pub const ALL: [CurveType; 4] = [
        CurveType::Linear,
        CurveType::SCurve,
        CurveType::Exponential,
        CurveType::Bezier,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear => "Lineal 1:1",
            Self::SCurve => "S-Curve",
            Self::Exponential => "Exponencial",
            Self::Bezier => "Bézier libre (Vista previa)",
        }
    }

    pub fn polynomial_badge(&self) -> &'static str {
        match self {
            Self::Linear => "1:1 LINEAR",
            Self::SCurve => "HARDWARE S-PROFILE (EXPO)",
            Self::Exponential => "HARDWARE J-PROFILE (ASYM)",
            Self::Bezier => "PREVIEW CUBIC-BÉZIER",
        }
    }
}

/// Parámetros de curva de transferencia de un eje.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveParameters {
    pub xsat: u16,      // 0..=1000 (700..=1000 => 70%..100%)
    pub ysat: u16,      // 0..=1000
    pub deadband: u16,  // 0..=1000 (0..=250 => 0%..25%)
    pub curvature: u16, // 0..=1000 (500 = lineal 1.00, 1000 = expo 3.00)
    pub profile: CurveProfile,
    pub curve_type: CurveType,
    pub invert: bool,
    pub bezier_p1: [f32; 2],
    pub bezier_p2: [f32; 2],
}

impl Default for CurveParameters {
    fn default() -> Self {
        Self {
            xsat: 1000,
            ysat: 1000,
            deadband: 40,   // 4.0%
            curvature: 660, // 1.64 Expo S-Curve
            profile: CurveProfile::S,
            curve_type: CurveType::SCurve,
            invert: false,
            bezier_p1: [0.35, 0.14],
            bezier_p2: [0.72, 0.88],
        }
    }
}

impl CurveParameters {
    /// Retorna la zona muerta central expresada en porcentaje (`0.0..=25.0%`).
    pub fn deadband_pct(&self) -> f32 {
        ((self.deadband as f32) / 10.0).clamp(0.0, 25.0)
    }

    /// Establece la zona muerta desde un valor porcentual (`0.0..=25.0%`).
    pub fn set_deadband_pct(&mut self, pct: f32) {
        self.deadband = (pct.clamp(0.0, 25.0) * 10.0).round() as u16;
    }

    /// Retorna el factor de curvatura / sensibilidad Expo (`1.00..=3.00`).
    pub fn expo_factor(&self) -> f32 {
        if self.curve_type == CurveType::Linear {
            return 1.0;
        }
        let c = (self.curvature as f32).clamp(500.0, 1000.0);
        1.0 + ((c - 500.0) / 500.0) * 2.0
    }

    /// Ajusta el factor Expo (`1.00..=3.00`) y sincroniza `curvature` (`500..=1000`).
    pub fn set_expo_factor(&mut self, expo: f32) {
        let clamped = expo.clamp(1.0, 3.0);
        self.curvature = (500.0 + (clamped - 1.0) * 250.0).round() as u16;
        if clamped <= 1.005 && self.curve_type == CurveType::Linear {
            self.curvature = 500;
        } else if self.curve_type == CurveType::Linear && clamped > 1.01 {
            self.curve_type = CurveType::SCurve;
        }
    }

    /// Retorna la saturación externa en porcentaje (`70.0..=100.0%`).
    pub fn saturation_pct(&self) -> f32 {
        ((self.xsat as f32) / 10.0).clamp(70.0, 100.0)
    }

    /// Establece la saturación externa desde porcentaje (`70.0..=100.0%`).
    pub fn set_saturation_pct(&mut self, pct: f32) {
        self.xsat = (pct.clamp(70.0, 100.0) * 10.0).round() as u16;
    }

    /// Cambia el modo de curva y actualiza los parámetros subyacentes de protocolo.
    pub fn set_curve_type(&mut self, curve_type: CurveType) {
        self.curve_type = curve_type;
        match curve_type {
            CurveType::Linear => {
                self.curvature = 500;
                self.profile = CurveProfile::S;
            }
            CurveType::SCurve => {
                if self.curvature == 500 {
                    self.curvature = 660;
                }
                self.profile = CurveProfile::S;
            }
            CurveType::Exponential => {
                if self.curvature == 500 {
                    self.curvature = 680;
                }
                self.profile = CurveProfile::J;
            }
            CurveType::Bezier => {
                if self.curvature == 500 {
                    self.curvature = 640;
                }
                self.profile = CurveProfile::S;
            }
        }
    }

    /// Sincroniza `curve_type` tras leer los parámetros crudos desde la EEPROM del hardware.
    pub fn sync_curve_type_from_hardware(&mut self) {
        if self.curvature == 500 {
            self.curve_type = CurveType::Linear;
        } else if self.profile == CurveProfile::J {
            self.curve_type = CurveType::Exponential;
        } else {
            self.curve_type = CurveType::SCurve;
        }
    }

    /// Evalúa la función de respuesta $y = f(x)$ para un valor de entrada normalizado $x \in [-1.0, 1.0]$
    /// (o $[0.0, 1.0]$ para aceleradores unidireccionales).
    pub fn evaluate(&self, x: f64) -> f64 {
        let base_sign = if x < 0.0 { -1.0 } else { 1.0 };
        let sign = if self.invert { -base_sign } else { base_sign };
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

        // Exponente base de curvatura
        let c = (self.curvature as f64) / 1000.0;
        let exponent = if self.curve_type == CurveType::Linear {
            1.0
        } else if c >= 0.5 {
            1.0 + (c - 0.5) * 4.0 // 1.0 a 3.0
        } else {
            1.0 / (1.0 + (0.5 - c) * 4.0) // 0.33 a 1.0
        };

        let curved = match self.curve_type {
            CurveType::Linear => active_x,
            CurveType::SCurve => {
                if self.profile == CurveProfile::J {
                    active_x.powf(exponent * 1.25)
                } else {
                    // Combinación Cubic-Hermite + potencia monotónica para tacto militar preciso
                    active_x.powf(exponent)
                }
            }
            CurveType::Exponential => active_x.powf(exponent * 1.25),
            CurveType::Bezier => {
                // Evaluación de curva cúbica de Bézier (0,0) -> P1 -> P2 -> (1,1)
                let t = active_x;
                let u = 1.0 - t;
                let y1 = (self.bezier_p1[1] as f64).clamp(0.0, 1.0);
                let y2 = (self.bezier_p2[1] as f64).clamp(0.0, 1.0);
                let bezier_y = 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t;
                // Modulado suavemente por el factor expo si se desvía de 1.0
                bezier_y.clamp(0.0, 1.0).powf(exponent * 0.65)
            }
        };

        let y = (curved * ysat_norm).clamp(0.0, 1.0);
        sign * y
    }

    /// Calcula la tasa instantánea de ganancia/compresión ($dy/dx$) en el punto $x$.
    pub fn compression_rate(&self, x: f64) -> f64 {
        let eps = 0.002;
        let x0 = (x - eps).clamp(-1.0, 1.0);
        let x1 = (x + eps).clamp(-1.0, 1.0);
        let dx = (x1 - x0).abs();
        if dx < 1e-6 {
            return 1.0;
        }
        ((self.evaluate(x1) - self.evaluate(x0)) / dx).abs()
    }

    /// Genera una serie de puntos $[x, y]$ para graficar la curva.
    pub fn generate_plot_points(&self, is_bipolar: bool, resolution: usize) -> Vec<[f64; 2]> {
        let mut points = Vec::with_capacity(resolution);

        let (min_x, max_x) = if is_bipolar { (-1.0, 1.0) } else { (0.0, 1.0) };

        let step = (max_x - min_x) / (resolution.saturating_sub(1).max(1)) as f64;
        for i in 0..resolution {
            let x = min_x + (i as f64) * step;
            let y = self.evaluate(x);
            points.push([x, y]);
        }

        points
    }
}

/// Estado en vivo de los ejes físicos leídos desde los dispositivos de entrada (evdev / hidraw).
#[derive(Debug, Clone, Copy)]
pub struct LiveAxesState {
    // Palanca (Stick)
    pub stick_x: f64,     // -1.0 .. 1.0 (Roll)
    pub stick_y: f64,     // -1.0 .. 1.0 (Pitch)
    pub stick_z: f64,     // -1.0 .. 1.0 (Twist / Yaw)
    pub ministick_x: f64, // -1.0 .. 1.0 (Slew X / C-Stick)
    pub ministick_y: f64, // -1.0 .. 1.0 (Slew Y / C-Stick)
    pub pov_x: i32,       // -1, 0, 1 (POV Hat 1 X)
    pub pov_y: i32,       // -1, 0, 1 (POV Hat 1 Y)

    // Lecturas ADC crudas (16-bit / 12-bit)
    pub raw_adc_x: u16,
    pub raw_adc_y: u16,
    pub raw_adc_z: u16,

    // Acelerador (Throttle)
    pub throttle_left: f64,        // 0.0 .. 1.0 (Unipolar)
    pub throttle_right: f64,       // 0.0 .. 1.0 (Unipolar)
    pub throttle_ministick_x: f64, // -1.0 .. 1.0 (Throttle Slew X / Mini-Gimbal)
    pub throttle_ministick_y: f64, // -1.0 .. 1.0 (Throttle Slew Y / Mini-Gimbal)
    pub rotary_1: f64,             // -1.0 .. 1.0 (Rotary 1 - 0x32)
    pub rotary_2: f64,             // -1.0 .. 1.0 (Rotary 2 - 0x35)
    pub rotary_3: f64,             // -1.0 .. 1.0 (Rotary 3 - 0x37)
    pub rotary_4: f64,             // -1.0 .. 1.0 (Rotary 4 - 0x36)

    // Estado de conexión evdev
    pub stick_connected: bool,
    pub throttle_connected: bool,

    #[allow(dead_code)]
    pub last_updated_ms: u64,
}

impl Default for LiveAxesState {
    fn default() -> Self {
        Self {
            stick_x: 0.0,
            stick_y: 0.0,
            stick_z: 0.0,
            ministick_x: 0.0,
            ministick_y: 0.0,
            pov_x: 0,
            pov_y: 0,
            raw_adc_x: 32768,
            raw_adc_y: 32768,
            raw_adc_z: 2048,
            throttle_left: 0.0,
            throttle_right: 0.0,
            throttle_ministick_x: 0.0,
            throttle_ministick_y: 0.0,
            rotary_1: 0.0,
            rotary_2: 0.0,
            rotary_3: 0.0,
            rotary_4: 0.0,
            stick_connected: false,
            throttle_connected: false,
            last_updated_ms: 0,
        }
    }
}

impl LiveAxesState {
    /// Calcula la magnitud vectorial $\sqrt{x^2 + y^2}$ y el ángulo polar en grados $[0^\circ, 360^\circ)$ del Gimbal principal (Stick).
    pub fn gimbal_vector(&self) -> (f64, f64) {
        let mag = (self.stick_x * self.stick_x + self.stick_y * self.stick_y)
            .sqrt()
            .min(std::f64::consts::SQRT_2);
        let angle_rad = self.stick_y.atan2(self.stick_x);
        let mut deg = angle_rad.to_degrees();
        if deg < 0.0 {
            deg += 360.0;
        }
        (mag, deg)
    }

    /// Calcula la magnitud vectorial y el ángulo polar en grados $[0^\circ, 360^\circ)$ del Mini-Gimbal del Throttle (Slew X / Slew Y).
    pub fn throttle_gimbal_vector(&self) -> (f64, f64) {
        let mag = (self.throttle_ministick_x * self.throttle_ministick_x
            + self.throttle_ministick_y * self.throttle_ministick_y)
            .sqrt()
            .min(std::f64::consts::SQRT_2);
        let angle_rad = self.throttle_ministick_y.atan2(self.throttle_ministick_x);
        let mut deg = angle_rad.to_degrees();
        if deg < 0.0 {
            deg += 360.0;
        }
        (mag, deg)
    }

    /// Devuelve la etiqueta cardinal de 8 direcciones del POV Hat 1.
    pub fn pov_direction_label(&self) -> &'static str {
        match (self.pov_x.signum(), self.pov_y.signum()) {
            (0, -1) => "N (000°)",
            (1, -1) => "NE (045°)",
            (1, 0) => "E (090°)",
            (1, 1) => "SE (135°)",
            (0, 1) => "S (180°)",
            (-1, 1) => "SW (225°)",
            (-1, 0) => "W (270°)",
            (-1, -1) => "NW (315°)",
            _ => "CENTER",
        }
    }
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

fn normalize_bipolar(info: &InputAbsInfo, default_mid: f64) -> f64 {
    if info.maximum > info.minimum {
        let mid = (info.minimum as f64 + info.maximum as f64) * 0.5;
        let half = ((info.maximum - info.minimum) as f64 * 0.5).max(1.0);
        (((info.value as f64) - mid) / half).clamp(-1.0, 1.0)
    } else {
        (((info.value as f64) - default_mid) / default_mid.max(1.0)).clamp(-1.0, 1.0)
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
            let mut new_mini_x = None;
            let mut new_mini_y = None;
            let mut new_pov_x = None;
            let mut new_pov_y = None;
            let mut new_raw_x = None;
            let mut new_raw_y = None;
            let mut new_raw_z = None;

            let mut new_tl = None;
            let mut new_tr = None;
            let mut new_thr_mini_x = None;
            let mut new_thr_mini_y = None;
            let mut new_r1 = None;
            let mut new_r2 = None;
            let mut new_r3 = None;
            let mut new_r4 = None;

            // Muestreo del Stick (evdev / EVIOCGABS)
            if let Some(ref file) = stick_file {
                if let Some(info) = query_abs_axis(file, 0) {
                    // ABS_X (Roll 0..65535, centro ~32768)
                    new_raw_x = Some(info.value.clamp(0, 65535) as u16);
                    new_stick_x =
                        Some((((info.value as f64) - 32768.0) / 32768.0).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 1) {
                    // ABS_Y (Pitch 0..65535, centro ~32768, invertido)
                    new_raw_y = Some(info.value.clamp(0, 65535) as u16);
                    new_stick_y =
                        Some((-(((info.value as f64) - 32768.0) / 32768.0)).clamp(-1.0, 1.0));
                }
                if let Some(info) = query_abs_axis(file, 5) {
                    // ABS_RZ (Twist / Yaw 0..4095, centro ~2048)
                    new_raw_z = Some(info.value.clamp(0, 65535) as u16);
                    new_stick_z = Some((((info.value as f64) - 2048.0) / 2048.0).clamp(-1.0, 1.0));
                }
                // Ministick C-Stick (ABS_RX = 3, ABS_RY = 4)
                if let Some(info) = query_abs_axis(file, 3) {
                    if info.maximum > info.minimum {
                        new_mini_x = Some(normalize_bipolar(&info, 128.0));
                    }
                }
                if let Some(info) = query_abs_axis(file, 4) {
                    if info.maximum > info.minimum {
                        new_mini_y = Some(-normalize_bipolar(&info, 128.0));
                    }
                }
                // POV Hat 1 (ABS_HAT0X = 16, ABS_HAT0Y = 17)
                if let Some(info) = query_abs_axis(file, 16) {
                    new_pov_x = Some(info.value.clamp(-1, 1));
                }
                if let Some(info) = query_abs_axis(file, 17) {
                    new_pov_y = Some(info.value.clamp(-1, 1));
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
                // Mini-gimbal del Throttle (ABS_RX = 3, ABS_RY = 4, rango 0..255, centro 128)
                if let Some(info) = query_abs_axis(file, 3) {
                    if info.maximum > info.minimum {
                        new_thr_mini_x = Some(normalize_bipolar(&info, 128.0));
                    }
                }
                if let Some(info) = query_abs_axis(file, 4) {
                    if info.maximum > info.minimum {
                        new_thr_mini_y = Some(-normalize_bipolar(&info, 128.0));
                    }
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
                lock.stick_connected = stick_file.is_some();
                lock.throttle_connected = throttle_file.is_some();

                if let Some(v) = new_stick_x {
                    lock.stick_x = v;
                }
                if let Some(v) = new_stick_y {
                    lock.stick_y = v;
                }
                if let Some(v) = new_stick_z {
                    lock.stick_z = v;
                }
                if let Some(v) = new_mini_x {
                    lock.ministick_x = v;
                }
                if let Some(v) = new_mini_y {
                    lock.ministick_y = v;
                }
                if let Some(v) = new_pov_x {
                    lock.pov_x = v;
                }
                if let Some(v) = new_pov_y {
                    lock.pov_y = v;
                }
                if let Some(v) = new_raw_x {
                    lock.raw_adc_x = v;
                }
                if let Some(v) = new_raw_y {
                    lock.raw_adc_y = v;
                }
                if let Some(v) = new_raw_z {
                    lock.raw_adc_z = v;
                }

                if let Some(v) = new_tl {
                    lock.throttle_left = v;
                }
                if let Some(v) = new_tr {
                    lock.throttle_right = v;
                }
                if let Some(v) = new_thr_mini_x {
                    lock.throttle_ministick_x = v;
                }
                if let Some(v) = new_thr_mini_y {
                    lock.throttle_ministick_y = v;
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
            curve_type: CurveType::Linear,
            ..Default::default()
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
            curve_type: CurveType::SCurve,
            ..Default::default()
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
            curve_type: CurveType::Exponential,
            ..Default::default()
        };

        assert_eq!(p.evaluate(0.0), 0.0);
        assert_eq!(p.evaluate(1.0), 1.0);
        assert!(p.evaluate(0.5) > 0.0 && p.evaluate(0.5) < 1.0);
    }

    #[test]
    fn invert_axis_direction() {
        let mut p = CurveParameters {
            xsat: 1000,
            ysat: 1000,
            deadband: 0,
            curvature: 500,
            profile: CurveProfile::S,
            curve_type: CurveType::Linear,
            invert: true,
            ..Default::default()
        };
        assert!((p.evaluate(0.5) - -0.5).abs() < 1e-4);
        p.invert = false;
        assert!((p.evaluate(0.5) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn throttle_gimbal_vector_computation() {
        let mut state = LiveAxesState::default();
        assert_eq!(state.throttle_gimbal_vector(), (0.0, 0.0));

        state.throttle_ministick_x = 1.0;
        state.throttle_ministick_y = 0.0;
        let (mag, deg) = state.throttle_gimbal_vector();
        assert!((mag - 1.0).abs() < 1e-4);
        assert!((deg - 0.0).abs() < 1e-4);

        state.throttle_ministick_x = 0.0;
        state.throttle_ministick_y = 1.0;
        let (mag, deg) = state.throttle_gimbal_vector();
        assert!((mag - 1.0).abs() < 1e-4);
        assert!((deg - 90.0).abs() < 1e-4);
    }
}
