//! Cálculo matemático de curvas de respuesta (S/J), zonas muertas y lectura en vivo de ejes.

use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use x56_core::Calibrator;
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

        // Zona muerta central
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

        let (min_x, max_x) = if is_bipolar {
            (-1.0, 1.0)
        } else {
            (0.0, 1.0)
        };

        let step = (max_x - min_x) / (resolution - 1) as f64;
        for i in 0..resolution {
            let x = min_x + (i as f64) * step;
            let y = self.evaluate(x);
            points.push([x, y]);
        }

        points
    }
}

/// Estado en vivo de los ejes físicos leídos desde la Interfaz 0 (`/dev/hidraw*`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveAxesState {
    pub stick_x: f64, // -1.0 .. 1.0
    pub stick_y: f64, // -1.0 .. 1.0
    pub stick_z: f64, // -1.0 .. 1.0
    #[allow(dead_code)]
    pub last_updated_ms: u64,
}

/// Lector en segundo plano de la Interfaz 0 (solo lectura, sin detach).
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
        while running.load(Ordering::Relaxed) {
            let input_node = Calibrator::find_stick_input_node();
            let Some(node_path) = input_node else {
                thread::sleep(Duration::from_millis(500));
                continue;
            };

            let Ok(mut file) = File::open(&node_path) else {
                thread::sleep(Duration::from_millis(500));
                continue;
            };

            let mut buf = [0u8; 64];
            while running.load(Ordering::Relaxed) {
                match file.read(&mut buf) {
                    Ok(n) if n >= 6 => {
                        let raw_x = u16::from_le_bytes([buf[0], buf[1]]);
                        let raw_y = u16::from_le_bytes([buf[2], buf[3]]);
                        let raw_z = u16::from_le_bytes([buf[4], buf[5]]) & 0x0FFF;

                        // Normalizar X e Y (0..65535 centrado en ~32768)
                        let norm_x = ((raw_x as f64) - 32768.0) / 32768.0;
                        let norm_y = -(((raw_y as f64) - 32768.0) / 32768.0); // Invertir Y para cabeceo natural
                        // Normalizar Z (0..4095 centrado en ~2048)
                        let norm_z = ((raw_z as f64) - 2048.0) / 2048.0;

                        if let Ok(mut lock) = state.lock() {
                            lock.stick_x = norm_x.clamp(-1.0, 1.0);
                            lock.stick_y = norm_y.clamp(-1.0, 1.0);
                            lock.stick_z = norm_z.clamp(-1.0, 1.0);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        // Si se desconectó el dispositivo o hubo error temporal
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(8)); // ~120 Hz de muestreo
            }
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
}
