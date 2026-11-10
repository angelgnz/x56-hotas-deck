//! Asignación de botones, selector rotativo de modo (`M1`/`M2`/`S1`), deslizador `SLD`,
//! monitor en vivo (evdev) e inyección de teclas (`uinput`) para el Logitech G X56 HOTAS.
//!
//! Los índices de botón siguen el mapa `JSIOCGBTNMAP` real del X56:
//! índices `0..15` -> `0x120 + idx`, índices `16..35` -> `0x2c0 + (idx - 16)`.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use x56_hid::{identify_device, DeviceKind, X56Device};
use x56_proto::RgbColor;

use crate::parse_color;

// ---------------------------------------------------------------------------
// Selector de modo
// ---------------------------------------------------------------------------

/// Posición del Conmutador de Modo F del Throttle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ModeSlot {
    /// Modo 1 (`0x2d1`).
    #[default]
    M1,
    /// Modo 2 (`0x2d2`).
    M2,
    /// Modo S1 (`0x2d3`).
    S1,
}

impl ModeSlot {
    /// Todos los modos, en el orden físico del selector.
    pub const ALL: [ModeSlot; 3] = [ModeSlot::M1, ModeSlot::M2, ModeSlot::S1];

    /// Nombre corto del modo.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::S1 => "S1",
        }
    }

    /// Descripción legible del modo.
    pub fn description(&self) -> &'static str {
        match self {
            Self::M1 => "Modo 1 (Navegación / Estándar)",
            Self::M2 => "Modo 2 (Combate / Ataque)",
            Self::S1 => "Modo S1 (Especial / Aterrizaje)",
        }
    }
}

// ---------------------------------------------------------------------------
// Catálogo de botones
// ---------------------------------------------------------------------------

/// Serialización de [`DeviceKind`] (definido en `x56-hid`, sin soporte serde propio).
mod device_kind_serde {
    use super::DeviceKind;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(kind: &DeviceKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match kind {
            DeviceKind::Stick => "stick",
            DeviceKind::Throttle => "throttle",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DeviceKind, D::Error> {
        let s = String::deserialize(d)?;
        match s.to_ascii_lowercase().as_str() {
            "stick" => Ok(DeviceKind::Stick),
            "throttle" => Ok(DeviceKind::Throttle),
            other => Err(serde::de::Error::custom(format!(
                "dispositivo desconocido '{other}'"
            ))),
        }
    }
}

/// Descriptor de un botón físico del X56.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonDescriptor {
    /// Identificador estable (p. ej. `"stick_trigger"`).
    pub id: String,
    /// Unidad a la que pertenece.
    #[serde(with = "device_kind_serde")]
    pub device: DeviceKind,
    /// Índice del botón (`JSIOCGBTNMAP`); `100..103` para el POV virtual.
    pub index: u8,
    /// Código Linux evdev (`0` para el POV, que se lee de `ABS_HAT0X/Y`).
    pub evdev_code: u16,
    /// Sección del manual ("A", "B", "C", "D", "F", "G").
    pub code_label: &'static str,
    /// Etiqueta legible.
    pub label: &'static str,
    /// Grupo funcional.
    pub group: &'static str,
}

/// Código evdev correspondiente a un índice del mapa de botones del X56.
const fn evdev_for_index(idx: u8) -> u16 {
    if idx < 16 {
        0x120 + idx as u16
    } else {
        0x2c0 + (idx as u16 - 16)
    }
}

const EVDEV_MODE_M1: u16 = 0x2d1;
const EVDEV_MODE_M2: u16 = 0x2d2;
const EVDEV_MODE_S1: u16 = 0x2d3;
const EVDEV_SLD: u16 = 0x2d0;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;
const BTN_JOYSTICK: u16 = 0x120;

fn btn(
    id: &str,
    device: DeviceKind,
    index: u8,
    code_label: &'static str,
    label: &'static str,
    group: &'static str,
) -> ButtonDescriptor {
    let evdev_code = if index >= 100 {
        0
    } else {
        evdev_for_index(index)
    };
    ButtonDescriptor {
        id: id.to_string(),
        device,
        index,
        evdev_code,
        code_label,
        label,
        group,
    }
}

fn stick_catalog() -> Vec<ButtonDescriptor> {
    use DeviceKind::Stick as S;
    const GD: &str = "D. Disparador y Cabezal";
    const GF: &str = "F. Controles de Meñique";
    const GC: &str = "C. Selectores Thumb HAT";
    const GA: &str = "A. Selector de Vista (POV)";

    let mut v = vec![
        btn("stick_trigger", S, 0, "D", "Gatillo (Trigger)", GD),
        btn("stick_btn_a", S, 1, "D", "Botón A (Cabezal)", GD),
        btn("stick_btn_b", S, 2, "D", "Botón B (Lateral)", GD),
        btn(
            "stick_btn_c",
            S,
            3,
            "B",
            "Botón C (Minipalanca Push)",
            "B. Minipalanca Analógica",
        ),
        btn("stick_btn_d", S, 4, "D", "Botón D (Pulgar Inferior)", GD),
        btn("stick_pinkie", S, 5, "F", "Botón Meñique (Pinkie)", GF),
    ];

    let h1 = [
        ("stick_h1_up", "HAT 1 (Castle) Arriba"),
        ("stick_h1_right", "HAT 1 (Castle) Derecha"),
        ("stick_h1_down", "HAT 1 (Castle) Abajo"),
        ("stick_h1_left", "HAT 1 (Castle) Izquierda"),
    ];
    for (i, (id, label)) in h1.into_iter().enumerate() {
        v.push(btn(id, S, 6 + i as u8, "C", label, GC));
    }
    let h2 = [
        ("stick_h2_up", "HAT 2 (Witches) Arriba"),
        ("stick_h2_right", "HAT 2 (Witches) Derecha"),
        ("stick_h2_down", "HAT 2 (Witches) Abajo"),
        ("stick_h2_left", "HAT 2 (Witches) Izquierda"),
    ];
    for (i, (id, label)) in h2.into_iter().enumerate() {
        v.push(btn(id, S, 10 + i as u8, "C", label, GC));
    }

    v.push(btn(
        "stick_flying_pinkie",
        S,
        14,
        "F",
        "Palanca Meñique (Flying Pinkie)",
        GF,
    ));

    let pov = [
        ("stick_pov_up", "POV Vista Arriba"),
        ("stick_pov_right", "POV Vista Derecha"),
        ("stick_pov_down", "POV Vista Abajo"),
        ("stick_pov_left", "POV Vista Izquierda"),
    ];
    for (i, (id, label)) in pov.into_iter().enumerate() {
        v.push(btn(id, S, 100 + i as u8, "A", label, GA));
    }
    v
}

fn throttle_catalog() -> Vec<ButtonDescriptor> {
    use DeviceKind::Throttle as T;
    const GC: &str = "C. Controles de Pulgar";
    const GCH: &str = "C. Controles de Pulgar (HATs)";
    const GA: &str = "A. Controles Giratorios";
    const GG: &str = "G. Conmutadores de Base (TGL 1-7)";

    let mut v = vec![
        btn("thr_btn_e", T, 0, "C", "Botón E (Pulgar)", GC),
        btn("thr_rty_f_push", T, 1, "A", "Giratorio F Push (RTY 1)", GA),
        btn("thr_rty_g_push", T, 2, "A", "Giratorio G Push (RTY 2)", GA),
        btn("thr_btn_i", T, 3, "C", "Botón I", GC),
        btn("thr_btn_h", T, 4, "C", "Botón H", GC),
    ];

    const SW: [(&str, &str); 14] = [
        ("thr_sw_1", "TGL 1 Arriba (SW 1)"),
        ("thr_sw_2", "TGL 1 Abajo (SW 2)"),
        ("thr_sw_3", "TGL 2 Arriba (SW 3)"),
        ("thr_sw_4", "TGL 2 Abajo (SW 4)"),
        ("thr_sw_5", "TGL 3 Arriba (SW 5)"),
        ("thr_sw_6", "TGL 3 Abajo (SW 6)"),
        ("thr_sw_7", "TGL 4 Arriba (SW 7)"),
        ("thr_sw_8", "TGL 4 Abajo (SW 8)"),
        ("thr_sw_9", "TGL 5 Arriba (SW 9)"),
        ("thr_sw_10", "TGL 5 Abajo (SW 10)"),
        ("thr_sw_11", "TGL 6 Arriba (SW 11)"),
        ("thr_sw_12", "TGL 6 Abajo (SW 12)"),
        ("thr_sw_13", "TGL 7 Arriba (SW 13)"),
        ("thr_sw_14", "TGL 7 Abajo (SW 14)"),
    ];
    for (i, (id, label)) in SW.into_iter().enumerate() {
        v.push(btn(id, T, 5 + i as u8, "G", label, GG));
    }

    let h3 = [
        ("thr_h3_up", "HAT 3 Arriba"),
        ("thr_h3_right", "HAT 3 Derecha"),
        ("thr_h3_down", "HAT 3 Abajo"),
        ("thr_h3_left", "HAT 3 Izquierda"),
    ];
    for (i, (id, label)) in h3.into_iter().enumerate() {
        v.push(btn(id, T, 19 + i as u8, "C", label, GCH));
    }
    let h4 = [
        ("thr_h4_up", "HAT 4 Arriba"),
        ("thr_h4_right", "HAT 4 Derecha"),
        ("thr_h4_down", "HAT 4 Abajo"),
        ("thr_h4_left", "HAT 4 Izquierda"),
    ];
    for (i, (id, label)) in h4.into_iter().enumerate() {
        v.push(btn(id, T, 23 + i as u8, "C", label, GCH));
    }

    v.push(btn("thr_k1_up", T, 27, "C", "Interruptor K1 Arriba", GC));
    v.push(btn("thr_k1_down", T, 28, "C", "Interruptor K1 Abajo", GC));
    v.push(btn(
        "thr_scroll_fwd",
        T,
        29,
        "A",
        "Rueda Scroll Adelante",
        GA,
    ));
    v.push(btn("thr_scroll_bck", T, 30, "A", "Rueda Scroll Atrás", GA));
    v.push(btn(
        "thr_ministick_push",
        T,
        31,
        "C",
        "Minipalanca Push (Pulgar)",
        GC,
    ));
    v.push(btn(
        "thr_sld",
        T,
        32,
        "B",
        "Control Deslizante 2 Posiciones (SLD)",
        "B. Control Deslizante (SLD)",
    ));
    v
}

/// Catálogo de botones asignables de una unidad (excluye el selector de modo F).
pub fn catalog_for_device(device: DeviceKind) -> Vec<ButtonDescriptor> {
    match device {
        DeviceKind::Stick => stick_catalog(),
        DeviceKind::Throttle => throttle_catalog(),
    }
}

/// Catálogo completo (Stick seguido de Throttle).
pub fn all_button_descriptors() -> Vec<ButtonDescriptor> {
    let mut v = catalog_for_device(DeviceKind::Stick);
    v.extend(catalog_for_device(DeviceKind::Throttle));
    v
}

// ---------------------------------------------------------------------------
// Celdas y perfiles de programación
// ---------------------------------------------------------------------------

/// Comando asignado a un botón dentro de un modo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommandCell {
    /// Identificador del botón (ver [`ButtonDescriptor::id`]).
    pub button_id: String,
    /// Nombre del comando (p. ej. "Tren de aterrizaje").
    pub name: String,
    /// Teclas a emitir, en orden de pulsación (p. ej. `["Ctrl", "G"]`).
    pub keys: Vec<String>,
}

impl CommandCell {
    /// Una celda está asignada cuando define al menos una tecla a emitir.
    pub fn is_assigned(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Teclas unidas con `" + "` (cadena vacía si no hay teclas).
    pub fn keys_display(&self) -> String {
        self.keys.join(" + ")
    }
}

/// Asignaciones de un modo, indexadas por `button_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModeBindings {
    /// Nombre descriptivo opcional para este modo (ej: "Vuelo y Supercrucero").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Color LED RGB en formato hexadecimal (`#RRGGBB` o `#RGB`) para ambos dispositivos en este modo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<String>,
    /// Color LED RGB específico para la palanca (Stick) en este modo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stick_rgb: Option<String>,
    /// Color LED RGB específico para el acelerador (Throttle) en este modo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle_rgb: Option<String>,
    #[serde(default)]
    pub cells: BTreeMap<String, CommandCell>,
}

impl ModeBindings {
    /// Resuelve el color hex configurado para el dispositivo indicado en este modo.
    pub fn rgb_for(&self, device: DeviceKind) -> Option<&str> {
        match device {
            DeviceKind::Stick => self.stick_rgb.as_deref().or(self.rgb.as_deref()),
            DeviceKind::Throttle => self.throttle_rgb.as_deref().or(self.rgb.as_deref()),
        }
    }
}

/// Perfil de programación: asignaciones por cada posición del selector de modo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProgrammingProfile {
    #[serde(default)]
    pub m1: ModeBindings,
    #[serde(default)]
    pub m2: ModeBindings,
    #[serde(default)]
    pub s1: ModeBindings,
}

impl ProgrammingProfile {
    /// Asignaciones del modo indicado.
    pub fn bindings_for(&self, mode: ModeSlot) -> &ModeBindings {
        match mode {
            ModeSlot::M1 => &self.m1,
            ModeSlot::M2 => &self.m2,
            ModeSlot::S1 => &self.s1,
        }
    }

    /// Asignaciones mutables del modo indicado.
    pub fn bindings_for_mut(&mut self, mode: ModeSlot) -> &mut ModeBindings {
        match mode {
            ModeSlot::M1 => &mut self.m1,
            ModeSlot::M2 => &mut self.m2,
            ModeSlot::S1 => &mut self.s1,
        }
    }

    /// Resuelve el color hex configurado para un modo y dispositivo dado.
    pub fn rgb_for_mode(&self, mode: ModeSlot, device: DeviceKind) -> Option<&str> {
        self.bindings_for(mode).rgb_for(device)
    }

    /// Comprueba si algún modo tiene configurada iluminación RGB propia.
    pub fn has_mode_rgb(&self) -> bool {
        self.m1.rgb.is_some()
            || self.m1.stick_rgb.is_some()
            || self.m1.throttle_rgb.is_some()
            || self.m2.rgb.is_some()
            || self.m2.stick_rgb.is_some()
            || self.m2.throttle_rgb.is_some()
            || self.s1.rgb.is_some()
            || self.s1.stick_rgb.is_some()
            || self.s1.throttle_rgb.is_some()
    }

    /// Celda de un botón en un modo, si existe.
    pub fn get_cell(&self, mode: ModeSlot, button_id: &str) -> Option<&CommandCell> {
        self.bindings_for(mode).cells.get(button_id)
    }

    /// Inserta o reemplaza la celda (clave: `cell.button_id`).
    pub fn set_cell(&mut self, mode: ModeSlot, cell: CommandCell) {
        self.bindings_for_mut(mode)
            .cells
            .insert(cell.button_id.clone(), cell);
    }

    /// Elimina y devuelve la celda de un botón en un modo.
    pub fn remove_cell(&mut self, mode: ModeSlot, button_id: &str) -> Option<CommandCell> {
        self.bindings_for_mut(mode).cells.remove(button_id)
    }

    /// Número total de celdas asignadas (ver [`CommandCell::is_assigned`]) en los 3 modos.
    pub fn total_assigned_count(&self) -> usize {
        ModeSlot::ALL
            .iter()
            .map(|m| {
                self.bindings_for(*m)
                    .cells
                    .values()
                    .filter(|c| c.is_assigned())
                    .count()
            })
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Tipos del monitor en vivo
// ---------------------------------------------------------------------------

/// Estado instantáneo de los botones, modo físico y `SLD`.
#[derive(Debug, Clone, Default)]
pub struct LiveButtonsSnapshot {
    pub active_mode: ModeSlot,
    pub slider_sld_on: bool,
    pub pressed_buttons: HashSet<String>,
    pub stick_connected: bool,
    pub throttle_connected: bool,
}

/// Registro de un comando disparado por un botón.
#[derive(Debug, Clone)]
pub struct TriggeredCommandEvent {
    /// Hora local `HH:MM:SS`.
    pub timestamp_hms: String,
    pub mode: ModeSlot,
    pub button_id: String,
    pub button_label: String,
    pub command_name: String,
    pub keys: Vec<String>,
    /// `true` si las teclas se emitieron realmente por `uinput`.
    pub uinput_emitted: bool,
}

// ---------------------------------------------------------------------------
// ioctl / evdev / uinput de bajo nivel
// ---------------------------------------------------------------------------

const IOC_NONE: u32 = 0;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, ty: u8, nr: u32, size: usize) -> libc::Ioctl {
    ((dir << 30) | ((size as u32) << 16) | ((ty as u32) << 8) | nr) as libc::Ioctl
}

/// Tamaño del bitmap de teclas evdev (`KEY_MAX = 0x2ff` -> 768 bits).
const KEY_BYTES: usize = 96;
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;

/// `EVIOCGKEY(len)`.
const EVIOCGKEY: libc::Ioctl = ioc(IOC_READ, b'E', 0x18, KEY_BYTES);

/// `EVIOCGBIT(ev, len)`.
const fn eviocgbit(ev: u16, len: usize) -> libc::Ioctl {
    ioc(IOC_READ, b'E', 0x20 + ev as u32, len)
}

/// `struct input_absinfo`.
#[repr(C)]
#[derive(Default)]
struct InputAbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// `EVIOCGABS(abs)`.
const fn eviocgabs(abs: u16) -> libc::Ioctl {
    ioc(
        IOC_READ,
        b'E',
        0x40 + abs as u32,
        std::mem::size_of::<InputAbsInfo>(),
    )
}

/// `struct input_id`.
#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

/// `struct uinput_setup`.
#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

/// `struct input_event`.
#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

const UI_DEV_CREATE: libc::Ioctl = ioc(IOC_NONE, b'U', 1, 0);
const UI_DEV_DESTROY: libc::Ioctl = ioc(IOC_NONE, b'U', 2, 0);
const UI_DEV_SETUP: libc::Ioctl = ioc(IOC_WRITE, b'U', 3, std::mem::size_of::<UinputSetup>());
const UI_SET_EVBIT: libc::Ioctl = ioc(IOC_WRITE, b'U', 100, std::mem::size_of::<libc::c_int>());
const UI_SET_KEYBIT: libc::Ioctl = ioc(IOC_WRITE, b'U', 101, std::mem::size_of::<libc::c_int>());

const BUS_USB: u16 = 0x03;
const VIRTUAL_KEYBOARD_NAME: &str = "X56 HOTAS Virtual Keyboard";
/// Duración de la pulsación simulada antes de soltar las teclas.
const TAP_HOLD: Duration = Duration::from_millis(12);

fn test_bit(buf: &[u8], bit: u16) -> bool {
    let byte = (bit / 8) as usize;
    byte < buf.len() && (buf[byte] >> (bit % 8)) & 1 == 1
}

/// Teclado virtual `uinput`.
struct VirtualKeyboard {
    file: File,
}

impl VirtualKeyboard {
    fn create() -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();

        let ctl = |req: libc::Ioctl, arg: libc::c_int| -> io::Result<()> {
            // SAFETY: `fd` es válido durante la llamada; el argumento es un entero.
            if unsafe { libc::ioctl(fd, req, arg) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        };

        ctl(UI_SET_EVBIT, EV_KEY as libc::c_int)?;
        ctl(UI_SET_EVBIT, EV_SYN as libc::c_int)?;
        for code in 1..=255 {
            ctl(UI_SET_KEYBIT, code)?;
        }

        let mut name = [0u8; 80];
        let bytes = VIRTUAL_KEYBOARD_NAME.as_bytes();
        name[..bytes.len()].copy_from_slice(bytes);
        let setup = UinputSetup {
            id: InputId {
                bustype: BUS_USB,
                vendor: 0x1209,
                product: 0x0056,
                version: 1,
            },
            name,
            ff_effects_max: 0,
        };
        // SAFETY: `setup` es `repr(C)` con el layout de `struct uinput_setup`.
        if unsafe { libc::ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: ioctl sin argumentos.
        if unsafe { libc::ioctl(fd, UI_DEV_CREATE) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { file })
    }

    fn write_event(&mut self, type_: u16, code: u16, value: i32) -> io::Result<()> {
        let ev = InputEvent {
            time: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            type_,
            code,
            value,
        };
        // SAFETY: `InputEvent` es `repr(C)` y de solo lectura durante el slice.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &ev as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        self.file.write_all(bytes)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.write_event(EV_SYN, 0, 0)
    }

    /// Pulsa las teclas en orden, las mantiene brevemente y las suelta en orden inverso.
    fn tap(&mut self, codes: &[u16]) -> io::Result<()> {
        for &c in codes {
            self.write_event(EV_KEY, c, 1)?;
            self.sync()?;
        }
        thread::sleep(TAP_HOLD);
        for &c in codes.iter().rev() {
            self.write_event(EV_KEY, c, 0)?;
            self.sync()?;
        }
        Ok(())
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        // SAFETY: ioctl sin argumentos sobre un fd propio aún abierto.
        unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY) };
    }
}

/// Traduce el nombre de una tecla (p. ej. `"Ctrl"`, `"F5"`, `"A"`) a su código Linux.
fn key_name_to_code(name: &str) -> Option<u16> {
    let up = name.trim().to_ascii_uppercase();
    let n = up.strip_prefix("KEY_").unwrap_or(&up);
    let code = match n {
        "ESC" | "ESCAPE" => 1,
        "1" => 2,
        "2" => 3,
        "3" => 4,
        "4" => 5,
        "5" => 6,
        "6" => 7,
        "7" => 8,
        "8" => 9,
        "9" => 10,
        "0" => 11,
        "-" | "MINUS" => 12,
        "=" | "EQUAL" => 13,
        "BACKSPACE" => 14,
        "TAB" => 15,
        "Q" => 16,
        "W" => 17,
        "E" => 18,
        "R" => 19,
        "T" => 20,
        "Y" => 21,
        "U" => 22,
        "I" => 23,
        "O" => 24,
        "P" => 25,
        "[" | "LEFTBRACE" => 26,
        "]" | "RIGHTBRACE" => 27,
        "ENTER" | "RETURN" => 28,
        "CTRL" | "CONTROL" | "LCTRL" | "LEFTCTRL" => 29,
        "A" => 30,
        "S" => 31,
        "D" => 32,
        "F" => 33,
        "G" => 34,
        "H" => 35,
        "J" => 36,
        "K" => 37,
        "L" => 38,
        ";" | "SEMICOLON" => 39,
        "'" | "APOSTROPHE" => 40,
        "`" | "GRAVE" => 41,
        "SHIFT" | "LSHIFT" | "LEFTSHIFT" => 42,
        "\\" | "BACKSLASH" => 43,
        "Z" => 44,
        "X" => 45,
        "C" => 46,
        "V" => 47,
        "B" => 48,
        "N" => 49,
        "M" => 50,
        "," | "COMMA" => 51,
        "." | "DOT" | "PERIOD" => 52,
        "/" | "SLASH" => 53,
        "RSHIFT" | "RIGHTSHIFT" => 54,
        "KPASTERISK" | "NUMMULTIPLY" => 55,
        "ALT" | "LALT" | "LEFTALT" => 56,
        "SPACE" | "SPACEBAR" => 57,
        "CAPSLOCK" => 58,
        "F1" => 59,
        "F2" => 60,
        "F3" => 61,
        "F4" => 62,
        "F5" => 63,
        "F6" => 64,
        "F7" => 65,
        "F8" => 66,
        "F9" => 67,
        "F10" => 68,
        "NUMLOCK" => 69,
        "SCROLLLOCK" => 70,
        "KP7" | "NUM7" | "NUMPAD7" => 71,
        "KP8" | "NUM8" | "NUMPAD8" => 72,
        "KP9" | "NUM9" | "NUMPAD9" => 73,
        "KPMINUS" | "NUMMINUS" | "NUMSUBTRACT" => 74,
        "KP4" | "NUM4" | "NUMPAD4" => 75,
        "KP5" | "NUM5" | "NUMPAD5" => 76,
        "KP6" | "NUM6" | "NUMPAD6" => 77,
        "KPPLUS" | "NUMPLUS" | "NUMADD" => 78,
        "KP1" | "NUM1" | "NUMPAD1" => 79,
        "KP2" | "NUM2" | "NUMPAD2" => 80,
        "KP3" | "NUM3" | "NUMPAD3" => 81,
        "KP0" | "NUM0" | "NUMPAD0" => 82,
        "KPDOT" | "NUMDOT" | "NUMDECIMAL" => 83,
        "F11" => 87,
        "F12" => 88,
        "KPENTER" | "NUMENTER" => 96,
        "RCTRL" | "RIGHTCTRL" => 97,
        "KPSLASH" | "NUMDIVIDE" => 98,
        "SYSRQ" | "PRINTSCREEN" | "PRTSC" => 99,
        "RALT" | "RIGHTALT" | "ALTGR" => 100,
        "HOME" => 102,
        "UP" | "ARROWUP" => 103,
        "PAGEUP" | "PGUP" => 104,
        "LEFT" | "ARROWLEFT" => 105,
        "RIGHT" | "ARROWRIGHT" => 106,
        "END" => 107,
        "DOWN" | "ARROWDOWN" => 108,
        "PAGEDOWN" | "PGDN" => 109,
        "INSERT" | "INS" => 110,
        "DELETE" | "DEL" => 111,
        "PAUSE" | "BREAK" => 119,
        "META" | "SUPER" | "WIN" | "LWIN" | "LMETA" | "LEFTMETA" | "CMD" => 125,
        "RMETA" | "RIGHTMETA" | "RWIN" => 126,
        _ => return None,
    };
    Some(code)
}

// ---------------------------------------------------------------------------
// Monitor en vivo
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(20); // 50 Hz
const RESCAN_INTERVAL: Duration = Duration::from_secs(1);
const MAX_QUEUED_EVENTS: usize = 500;

struct MonitorState {
    snapshot: LiveButtonsSnapshot,
    profile: ProgrammingProfile,
    fallback_stick_rgb: Option<String>,
    fallback_throttle_rgb: Option<String>,
    events: VecDeque<TriggeredCommandEvent>,
}

struct Shared {
    state: Mutex<MonitorState>,
    active: AtomicBool,
    uinput: AtomicBool,
    stop: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, MonitorState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Monitor en segundo plano (50 Hz) de los botones del X56 con inyección opcional de teclas.
pub struct ButtonMonitor {
    shared: Arc<Shared>,
    rgb_tx: mpsc::Sender<RgbDispatchCommand>,
    handle: Option<JoinHandle<()>>,
    rgb_handle: Option<JoinHandle<()>>,
}

impl Default for ButtonMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ButtonMonitor {
    /// Crea el monitor, el teclado virtual (si `/dev/uinput` lo permite) y lanza los hilos.
    pub fn new() -> Self {
        let keyboard = VirtualKeyboard::create().ok();
        let shared = Arc::new(Shared {
            state: Mutex::new(MonitorState {
                snapshot: LiveButtonsSnapshot::default(),
                profile: ProgrammingProfile::default(),
                fallback_stick_rgb: None,
                fallback_throttle_rgb: None,
                events: VecDeque::new(),
            }),
            active: AtomicBool::new(false),
            uinput: AtomicBool::new(keyboard.is_some()),
            stop: AtomicBool::new(false),
        });

        // Hilo dedicado desacoplado para iluminación RGB atómica y reintentos transaccionales
        let (rgb_tx, rgb_rx) = mpsc::channel();
        let rgb_dispatcher = RgbDispatcher::new(rgb_rx, Arc::clone(&shared));
        let rgb_handle = thread::Builder::new()
            .name("x56-rgb-dispatcher".into())
            .spawn(move || rgb_dispatcher.run())
            .ok();

        // Hilo de botones evdev a 50 Hz puro (cero bloqueo por USB HID)
        let worker_shared = Arc::clone(&shared);
        let mut worker = Worker::new(worker_shared, keyboard, rgb_tx.clone());
        worker.tick();
        let handle = thread::Builder::new()
            .name("x56-button-monitor".into())
            .spawn(move || worker.run())
            .ok();

        Self {
            shared,
            rgb_tx,
            handle,
            rgb_handle,
        }
    }

    /// Estado actual de botones, modo y `SLD`.
    pub fn snapshot(&self) -> LiveButtonsSnapshot {
        self.shared.lock().snapshot.clone()
    }

    /// Define colores de iluminación de reserva para cuando un modo no especifique su propio RGB.
    pub fn set_fallback_rgb(&self, stick_rgb: Option<String>, throttle_rgb: Option<String>) {
        {
            let mut st = self.shared.lock();
            st.fallback_stick_rgb = stick_rgb;
            st.fallback_throttle_rgb = throttle_rgb;
        }
        let _ = self.rgb_tx.send(RgbDispatchCommand::ForceUpdate);
    }

    /// Reemplaza el perfil de programación usado para resolver comandos y actualiza la iluminación.
    pub fn set_profile(&self, profile: ProgrammingProfile) {
        {
            let mut st = self.shared.lock();
            st.profile = profile;
        }
        let _ = self.rgb_tx.send(RgbDispatchCommand::ForceUpdate);
    }

    /// Perfil de programación vigente.
    pub fn get_profile(&self) -> ProgrammingProfile {
        self.shared.lock().profile.clone()
    }

    /// Activa o desactiva la emisión real de teclas ("Diana Azul").
    pub fn set_active(&self, active: bool) {
        self.shared.active.store(active, Ordering::SeqCst);
    }

    /// Indica si la emisión de teclas está activada.
    pub fn is_active(&self) -> bool {
        self.shared.active.load(Ordering::SeqCst)
    }

    /// Indica si el teclado virtual `uinput` está disponible.
    pub fn uinput_available(&self) -> bool {
        self.shared.uinput.load(Ordering::SeqCst)
    }

    /// Extrae (y vacía) la cola de comandos disparados desde la última llamada.
    pub fn drain_events(&self) -> Vec<TriggeredCommandEvent> {
        self.shared.lock().events.drain(..).collect()
    }
}

impl Drop for ButtonMonitor {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = self.rgb_tx.send(RgbDispatchCommand::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.rgb_handle.take() {
            let _ = h.join();
        }
    }
}

/// Dispositivo evdev abierto.
struct InputNode {
    file: File,
    /// `false` hasta la primera lectura tras abrirlo (evita disparos espurios).
    primed: bool,
}

/// Lectura cruda de un dispositivo en un instante.
struct RawState {
    keys: [u8; KEY_BYTES],
    hat_x: i32,
    hat_y: i32,
}

impl InputNode {
    fn read(&self, with_hat: bool) -> io::Result<RawState> {
        let fd = self.file.as_raw_fd();
        let mut keys = [0u8; KEY_BYTES];
        // SAFETY: `keys` mide exactamente `KEY_BYTES`, el tamaño codificado en `EVIOCGKEY`.
        if unsafe { libc::ioctl(fd, EVIOCGKEY, keys.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let read_abs = |axis: u16| -> i32 {
            let mut info = InputAbsInfo::default();
            // SAFETY: `info` es `repr(C)` con el layout de `struct input_absinfo`.
            if unsafe { libc::ioctl(fd, eviocgabs(axis), &mut info as *mut InputAbsInfo) } < 0 {
                0
            } else {
                info.value
            }
        };
        let (hat_x, hat_y) = if with_hat {
            (read_abs(ABS_HAT0X), read_abs(ABS_HAT0Y))
        } else {
            (0, 0)
        };
        Ok(RawState { keys, hat_x, hat_y })
    }
}

/// Busca el nodo `/dev/input/event*` de joystick correspondiente a la unidad indicada.
fn find_event_node(kind: DeviceKind) -> Option<PathBuf> {
    let mut entries: Vec<_> = std::fs::read_dir("/sys/class/input")
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let read_hex = |p: PathBuf| -> Option<u16> {
        u16::from_str_radix(std::fs::read_to_string(p).ok()?.trim(), 16).ok()
    };

    for entry in entries {
        let dir = entry.path().join("device/id");
        let (Some(vid), Some(pid)) = (read_hex(dir.join("vendor")), read_hex(dir.join("product")))
        else {
            continue;
        };
        if !matches!(identify_device(vid, pid), Some((_, k)) if k == kind) {
            continue;
        }
        let path = PathBuf::from("/dev/input").join(entry.file_name());
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        else {
            continue;
        };
        // Debe exponer botones de joystick (BTN_JOYSTICK = 0x120).
        let mut bits = [0u8; KEY_BYTES];
        // SAFETY: `bits` mide `KEY_BYTES`, el tamaño codificado en la petición.
        let ok = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                eviocgbit(EV_KEY, KEY_BYTES),
                bits.as_mut_ptr(),
            )
        } >= 0;
        if ok && test_bit(&bits, BTN_JOYSTICK) {
            return Some(path);
        }
    }
    None
}

fn open_node(kind: DeviceKind) -> Option<InputNode> {
    let path = find_event_node(kind)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    Some(InputNode {
        file,
        primed: false,
    })
}

/// Detecta pasivamente la posición actual del selector físico de modo (M1, M2, S1)
/// leyendo el estado actual mediante ioctl EVIOCGKEY sin lanzar hilos ni alterar el kernel.
pub fn detect_current_mode() -> Option<ModeSlot> {
    let node = open_node(DeviceKind::Throttle)?;
    let raw = node.read(false).ok()?;
    if test_bit(&raw.keys, EVDEV_MODE_M1) {
        Some(ModeSlot::M1)
    } else if test_bit(&raw.keys, EVDEV_MODE_M2) {
        Some(ModeSlot::M2)
    } else if test_bit(&raw.keys, EVDEV_MODE_S1) {
        Some(ModeSlot::S1)
    } else {
        None
    }
}

/// Hora local `HH:MM:SS`.
fn now_hms() -> String {
    // SAFETY: `time`/`localtime_r` con punteros a valores locales válidos.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return "--:--:--".to_string();
        }
        format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
    }
}

/// Botones presionados de una unidad según su lectura cruda.
fn pressed_ids(descriptors: &[ButtonDescriptor], raw: &RawState) -> HashSet<String> {
    descriptors
        .iter()
        .filter(|d| {
            if d.evdev_code != 0 {
                return test_bit(&raw.keys, d.evdev_code);
            }
            match d.id.as_str() {
                "stick_pov_up" => raw.hat_y < 0,
                "stick_pov_down" => raw.hat_y > 0,
                "stick_pov_left" => raw.hat_x < 0,
                "stick_pov_right" => raw.hat_x > 0,
                _ => false,
            }
        })
        .map(|d| d.id.clone())
        .collect()
}

struct Worker {
    shared: Arc<Shared>,
    keyboard: Option<VirtualKeyboard>,
    stick: Option<InputNode>,
    throttle: Option<InputNode>,
    stick_desc: Vec<ButtonDescriptor>,
    throttle_desc: Vec<ButtonDescriptor>,
    prev_pressed: HashSet<String>,
    mode: ModeSlot,
    candidate_mode: Option<ModeSlot>,
    candidate_mode_since: Instant,
    rgb_tx: mpsc::Sender<RgbDispatchCommand>,
    last_scan: Instant,
    last_keyboard_try: Instant,
}

impl Worker {
    fn new(
        shared: Arc<Shared>,
        keyboard: Option<VirtualKeyboard>,
        rgb_tx: mpsc::Sender<RgbDispatchCommand>,
    ) -> Self {
        let now = Instant::now();
        Self {
            shared,
            keyboard,
            stick: None,
            throttle: None,
            stick_desc: catalog_for_device(DeviceKind::Stick),
            throttle_desc: catalog_for_device(DeviceKind::Throttle),
            prev_pressed: HashSet::new(),
            mode: ModeSlot::default(),
            candidate_mode: None,
            candidate_mode_since: now,
            rgb_tx,
            // Fuerza un escaneo inmediato.
            last_scan: now.checked_sub(RESCAN_INTERVAL).unwrap_or(now),
            last_keyboard_try: now,
        }
    }

    fn run(mut self) {
        while !self.shared.stop.load(Ordering::SeqCst) {
            let started = Instant::now();
            self.tick();
            if let Some(rest) = POLL_INTERVAL.checked_sub(started.elapsed()) {
                thread::sleep(rest);
            }
        }
    }

    fn tick(&mut self) {
        if self.last_scan.elapsed() >= RESCAN_INTERVAL {
            self.last_scan = Instant::now();
            if self.stick.is_none() {
                self.stick = open_node(DeviceKind::Stick);
            }
            if self.throttle.is_none() {
                self.throttle = open_node(DeviceKind::Throttle);
            }
        }
        if self.keyboard.is_none() && self.last_keyboard_try.elapsed() >= Duration::from_secs(2) {
            self.last_keyboard_try = Instant::now();
            self.keyboard = VirtualKeyboard::create().ok();
            self.shared
                .uinput
                .store(self.keyboard.is_some(), Ordering::SeqCst);
        }

        // Throttle: modo físico, SLD y botones.
        let mut sld = false;
        let mut pressed: HashSet<String> = HashSet::new();
        let mut new_presses: Vec<(String, bool)> = Vec::new(); // (id, descriptor del throttle)

        if let Some(node) = self.throttle.as_mut() {
            match node.read(false) {
                Ok(raw) => {
                    let raw_mode = if test_bit(&raw.keys, EVDEV_MODE_M1) {
                        Some(ModeSlot::M1)
                    } else if test_bit(&raw.keys, EVDEV_MODE_M2) {
                        Some(ModeSlot::M2)
                    } else if test_bit(&raw.keys, EVDEV_MODE_S1) {
                        Some(ModeSlot::S1)
                    } else {
                        None
                    };

                    if let Some(m) = raw_mode {
                        if self.candidate_mode.is_none() {
                            self.candidate_mode = Some(m);
                            self.candidate_mode_since = Instant::now();
                            self.mode = m;
                            let _ = self.rgb_tx.send(RgbDispatchCommand::ModeChanged(m));
                        } else if self.candidate_mode != Some(m) {
                            self.candidate_mode = Some(m);
                            self.candidate_mode_since = Instant::now();
                        } else if self.candidate_mode_since.elapsed() >= Duration::from_millis(50)
                            && self.mode != m
                        {
                            self.mode = m;
                            let _ = self.rgb_tx.send(RgbDispatchCommand::ModeChanged(m));
                        }
                    }

                    sld = test_bit(&raw.keys, EVDEV_SLD);
                    let ids = pressed_ids(&self.throttle_desc, &raw);
                    if node.primed {
                        new_presses.extend(
                            ids.iter()
                                .filter(|id| !self.prev_pressed.contains(*id))
                                .map(|id| (id.clone(), true)),
                        );
                    }
                    node.primed = true;
                    pressed.extend(ids);
                }
                Err(_) => self.throttle = None,
            }
        }

        if let Some(node) = self.stick.as_mut() {
            match node.read(true) {
                Ok(raw) => {
                    let ids = pressed_ids(&self.stick_desc, &raw);
                    if node.primed {
                        new_presses.extend(
                            ids.iter()
                                .filter(|id| !self.prev_pressed.contains(*id))
                                .map(|id| (id.clone(), false)),
                        );
                    }
                    node.primed = true;
                    pressed.extend(ids);
                }
                Err(_) => self.stick = None,
            }
        }

        let mode = self.mode;
        let (stick_connected, throttle_connected) = (self.stick.is_some(), self.throttle.is_some());

        // Actualiza snapshot y resuelve celdas asignadas bajo un único lock.
        let mut triggers: Vec<(String, String, CommandCell)> = Vec::new();
        {
            let mut st = self.shared.lock();
            st.snapshot = LiveButtonsSnapshot {
                active_mode: mode,
                slider_sld_on: sld,
                pressed_buttons: pressed.clone(),
                stick_connected,
                throttle_connected,
            };
            new_presses.sort();
            for (id, is_throttle) in &new_presses {
                let Some(cell) = st.profile.get_cell(mode, id).filter(|c| c.is_assigned()) else {
                    continue;
                };
                let desc = if *is_throttle {
                    &self.throttle_desc
                } else {
                    &self.stick_desc
                };
                let label = desc
                    .iter()
                    .find(|d| d.id == *id)
                    .map(|d| d.label.to_string())
                    .unwrap_or_else(|| id.clone());
                triggers.push((id.clone(), label, cell.clone()));
            }
        }
        self.prev_pressed = pressed;

        // Emisión real fuera del lock (incluye una breve pausa de pulsación).
        for (id, label, cell) in triggers {
            let mut emitted = false;
            if self.shared.active.load(Ordering::SeqCst) {
                if let Some(kb) = self.keyboard.as_mut() {
                    let codes: Vec<u16> = cell
                        .keys
                        .iter()
                        .filter_map(|k| key_name_to_code(k))
                        .collect();
                    if !codes.is_empty() {
                        emitted = kb.tap(&codes).is_ok();
                    }
                }
            }
            let event = TriggeredCommandEvent {
                timestamp_hms: now_hms(),
                mode,
                button_id: id,
                button_label: label,
                command_name: cell.name.clone(),
                keys: cell.keys.clone(),
                uinput_emitted: emitted,
            };
            let mut st = self.shared.lock();
            if st.events.len() >= MAX_QUEUED_EVENTS {
                st.events.pop_front();
            }
            st.events.push_back(event);
        }
    }
}

/// Comandos despachados al hilo dedicado de sincronización RGB.
enum RgbDispatchCommand {
    ModeChanged(ModeSlot),
    ForceUpdate,
    Shutdown,
}

/// Hilo dedicado e independiente para el despacho de iluminación LED RGB.
///
/// Implementa reintentos transaccionales y comunicación atómica unificada
/// mediante [`X56Device::set_rgb_unified`] para evitar congelamientos en evdev.
struct RgbDispatcher {
    rx: mpsc::Receiver<RgbDispatchCommand>,
    shared: Arc<Shared>,
    current_mode: ModeSlot,
    applied_stick_color: Option<RgbColor>,
    applied_throttle_color: Option<RgbColor>,
}

impl RgbDispatcher {
    fn new(rx: mpsc::Receiver<RgbDispatchCommand>, shared: Arc<Shared>) -> Self {
        Self {
            rx,
            shared,
            current_mode: ModeSlot::default(),
            applied_stick_color: None,
            applied_throttle_color: None,
        }
    }

    fn run(mut self) {
        while !self.shared.stop.load(Ordering::SeqCst) {
            let cmd = match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(cmd) => cmd,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.reconcile_colors(false);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

            match cmd {
                RgbDispatchCommand::ModeChanged(mode) => {
                    self.current_mode = mode;
                    // Drenar comandos subsiguientes rápidos si la perilla se giró rápido
                    while let Ok(newer_cmd) = self.rx.try_recv() {
                        if let RgbDispatchCommand::ModeChanged(newer_mode) = newer_cmd {
                            self.current_mode = newer_mode;
                        }
                    }
                    info!(
                        "RgbDispatcher: mode switch detected -> {}",
                        self.current_mode.as_str()
                    );
                    self.reconcile_colors(true);
                }
                RgbDispatchCommand::ForceUpdate => {
                    debug!("RgbDispatcher: forced RGB reconciliation requested");
                    self.applied_stick_color = None;
                    self.applied_throttle_color = None;
                    self.reconcile_colors(true);
                }
                RgbDispatchCommand::Shutdown => {
                    debug!("RgbDispatcher: shutting down");
                    break;
                }
            }
        }
    }

    fn reconcile_colors(&mut self, retry: bool) {
        let (target_stick, target_throttle) = {
            let st = self.shared.lock();
            let mode = self.current_mode;
            let s_str = st
                .profile
                .rgb_for_mode(mode, DeviceKind::Stick)
                .map(str::to_string)
                .or_else(|| st.fallback_stick_rgb.clone());
            let t_str = st
                .profile
                .rgb_for_mode(mode, DeviceKind::Throttle)
                .map(str::to_string)
                .or_else(|| st.fallback_throttle_rgb.clone());

            let s_col = s_str.and_then(|s| parse_color(&s).ok());
            let t_col = t_str.and_then(|t| parse_color(&t).ok());
            (s_col, t_col)
        };

        if self.applied_stick_color == target_stick
            && self.applied_throttle_color == target_throttle
        {
            return;
        }

        if target_stick.is_none() {
            self.applied_stick_color = None;
        }
        if target_throttle.is_none() {
            self.applied_throttle_color = None;
        }

        let max_attempts = if retry { 4 } else { 1 };
        for attempt in 0..max_attempts {
            if self.shared.stop.load(Ordering::SeqCst) {
                break;
            }

            let stick_to_send = if self.applied_stick_color != target_stick {
                target_stick
            } else {
                None
            };
            let throttle_to_send = if self.applied_throttle_color != target_throttle {
                target_throttle
            } else {
                None
            };

            if stick_to_send.is_none() && throttle_to_send.is_none() {
                break;
            }

            match X56Device::set_rgb_unified(stick_to_send, throttle_to_send) {
                Ok((stick_ok, throttle_ok)) => {
                    if stick_ok {
                        self.applied_stick_color = target_stick;
                    }
                    if throttle_ok {
                        self.applied_throttle_color = target_throttle;
                    }
                    debug!(
                        "RgbDispatcher attempt {}/{}: stick_ok={}, throttle_ok={}",
                        attempt + 1,
                        max_attempts,
                        stick_ok,
                        throttle_ok
                    );
                }
                Err(err) => {
                    error!(
                        "RgbDispatcher attempt {}/{} failed calling set_rgb_unified: {err}",
                        attempt + 1,
                        max_attempts
                    );
                }
            }

            if self.applied_stick_color == target_stick
                && self.applied_throttle_color == target_throttle
            {
                break;
            }

            if attempt + 1 < max_attempts {
                warn!(
                    "RgbDispatcher: partial RGB update (stick_synced={}, throttle_synced={}), retrying in 35ms...",
                    self.applied_stick_color == target_stick,
                    self.applied_throttle_color == target_throttle
                );
                thread::sleep(Duration::from_millis(35));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(v: &'a [ButtonDescriptor], id: &str) -> &'a ButtonDescriptor {
        v.iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("falta {id}"))
    }

    #[test]
    fn mode_slot_basics() {
        assert_eq!(ModeSlot::default(), ModeSlot::M1);
        assert_eq!(ModeSlot::ALL.map(|m| m.as_str()), ["M1", "M2", "S1"]);
        assert_eq!(ModeSlot::M2.description(), "Modo 2 (Combate / Ataque)");
        assert_eq!(
            ModeSlot::S1.description(),
            "Modo S1 (Especial / Aterrizaje)"
        );
    }

    #[test]
    fn evdev_index_mapping() {
        assert_eq!(evdev_for_index(0), 0x120);
        assert_eq!(evdev_for_index(15), 0x12f);
        assert_eq!(evdev_for_index(16), 0x2c0);
        assert_eq!(evdev_for_index(32), 0x2d0);
        assert_eq!(evdev_for_index(33), 0x2d1);
        assert_eq!(evdev_for_index(35), 0x2d3);
    }

    #[test]
    fn stick_catalog_matches_spec() {
        let c = catalog_for_device(DeviceKind::Stick);
        assert_eq!(c.len(), 19);
        assert!(c.iter().all(|d| d.device == DeviceKind::Stick));
        let t = find(&c, "stick_trigger");
        assert_eq!((t.index, t.evdev_code, t.code_label), (0, 0x120, "D"));
        assert_eq!(t.label, "Gatillo (Trigger)");
        let cb = find(&c, "stick_btn_c");
        assert_eq!((cb.evdev_code, cb.code_label), (0x123, "B"));
        assert_eq!(cb.group, "B. Minipalanca Analógica");
        assert_eq!(find(&c, "stick_flying_pinkie").evdev_code, 0x12e);
        assert_eq!(find(&c, "stick_h1_up").evdev_code, 0x126);
        assert_eq!(find(&c, "stick_h1_left").evdev_code, 0x129);
        assert_eq!(find(&c, "stick_h2_up").evdev_code, 0x12a);
        assert_eq!(find(&c, "stick_h2_left").evdev_code, 0x12d);
        let pov = find(&c, "stick_pov_left");
        assert_eq!((pov.index, pov.evdev_code, pov.code_label), (103, 0, "A"));
        assert_eq!(find(&c, "stick_pov_up").index, 100);
    }

    #[test]
    fn throttle_catalog_matches_spec() {
        let c = catalog_for_device(DeviceKind::Throttle);
        assert_eq!(c.len(), 33);
        assert_eq!(find(&c, "thr_btn_e").evdev_code, 0x120);
        assert_eq!(find(&c, "thr_btn_h").evdev_code, 0x124);
        assert_eq!(find(&c, "thr_sw_1").evdev_code, 0x125);
        assert_eq!(find(&c, "thr_sw_11").evdev_code, 0x12f);
        assert_eq!(find(&c, "thr_sw_12").evdev_code, 0x2c0);
        assert_eq!(find(&c, "thr_sw_14").evdev_code, 0x2c2);
        assert_eq!(find(&c, "thr_sw_14").label, "TGL 7 Abajo (SW 14)");
        assert_eq!(find(&c, "thr_h3_up").evdev_code, 0x2c3);
        assert_eq!(find(&c, "thr_h4_left").evdev_code, 0x2ca);
        assert_eq!(find(&c, "thr_k1_up").evdev_code, 0x2cb);
        assert_eq!(find(&c, "thr_scroll_bck").evdev_code, 0x2ce);
        assert_eq!(find(&c, "thr_ministick_push").evdev_code, 0x2cf);
        let sld = find(&c, "thr_sld");
        assert_eq!((sld.evdev_code, sld.code_label), (0x2d0, "B"));
        assert_eq!(sld.group, "B. Control Deslizante (SLD)");
        // Los índices de modo (0x2d1..0x2d3) no son botones asignables.
        assert!(c.iter().all(|d| !(0x2d1..=0x2d3).contains(&d.evdev_code)));
    }

    #[test]
    fn catalog_ids_unique_and_complete() {
        let all = all_button_descriptors();
        assert_eq!(all.len(), 52);
        let ids: HashSet<_> = all.iter().map(|d| d.id.clone()).collect();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn descriptor_serializes() {
        let d = find(&catalog_for_device(DeviceKind::Throttle), "thr_sld").clone();
        let s = toml::to_string(&d).unwrap();
        assert!(s.contains("device = \"throttle\""));
    }

    #[test]
    fn cell_and_profile_ops() {
        let mut p = ProgrammingProfile::default();
        assert_eq!(p.total_assigned_count(), 0);
        assert!(!CommandCell::default().is_assigned());

        let cell = CommandCell {
            button_id: "stick_trigger".into(),
            name: "Disparar".into(),
            keys: vec!["Ctrl".into(), "F".into()],
        };
        assert!(cell.is_assigned());
        assert_eq!(cell.keys_display(), "Ctrl + F");

        p.set_cell(ModeSlot::M2, cell.clone());
        p.set_cell(
            ModeSlot::S1,
            CommandCell {
                button_id: "thr_sld".into(),
                ..Default::default()
            },
        );
        assert_eq!(p.total_assigned_count(), 1);
        assert_eq!(p.get_cell(ModeSlot::M2, "stick_trigger"), Some(&cell));
        assert!(p.get_cell(ModeSlot::M1, "stick_trigger").is_none());
        assert_eq!(p.remove_cell(ModeSlot::M2, "stick_trigger"), Some(cell));
        assert_eq!(p.total_assigned_count(), 0);
        p.bindings_for_mut(ModeSlot::M1)
            .cells
            .insert("x".into(), CommandCell::default());
        assert_eq!(p.bindings_for(ModeSlot::M1).cells.len(), 1);
    }

    #[test]
    fn programming_profile_toml_roundtrip() {
        let mut p = ProgrammingProfile::default();
        p.set_cell(
            ModeSlot::S1,
            CommandCell {
                button_id: "thr_sw_1".into(),
                name: "Tren".into(),
                keys: vec!["G".into()],
            },
        );
        let s = toml::to_string(&p).unwrap();
        assert_eq!(toml::from_str::<ProgrammingProfile>(&s).unwrap(), p);
        // Campos ausentes -> valores por defecto.
        assert_eq!(
            toml::from_str::<ProgrammingProfile>("").unwrap(),
            ProgrammingProfile::default()
        );
    }

    #[test]
    fn mode_rgb_configuration_and_resolution() {
        let toml_str = r##"
[m1]
name = "Vuelo y Supercrucero"
rgb = "#9d4edd"

[m2]
name = "Combate y Ataque"
rgb = "#ff1100"
stick_rgb = "#ff0000"

[s1]
name = "Minería y Operaciones"
rgb = "#0088ff"
"##;
        let p: ProgrammingProfile = toml::from_str(toml_str).unwrap();
        assert!(p.has_mode_rgb());
        assert_eq!(p.m1.name.as_deref(), Some("Vuelo y Supercrucero"));
        assert_eq!(
            p.rgb_for_mode(ModeSlot::M1, DeviceKind::Stick),
            Some("#9d4edd")
        );
        assert_eq!(
            p.rgb_for_mode(ModeSlot::M1, DeviceKind::Throttle),
            Some("#9d4edd")
        );
        assert_eq!(
            p.rgb_for_mode(ModeSlot::M2, DeviceKind::Stick),
            Some("#ff0000")
        );
        assert_eq!(
            p.rgb_for_mode(ModeSlot::M2, DeviceKind::Throttle),
            Some("#ff1100")
        );
        assert_eq!(
            p.rgb_for_mode(ModeSlot::S1, DeviceKind::Stick),
            Some("#0088ff")
        );
    }

    #[test]
    #[allow(clippy::unnecessary_cast)]
    fn ioctl_numbers_match_linux() {
        if cfg!(target_arch = "x86_64") {
            assert_eq!(EVIOCGKEY as u64, 0x8060_4518);
            assert_eq!(eviocgabs(0x10) as u64, 0x8018_4550);
            assert_eq!(UI_DEV_CREATE as u64, 0x5501);
            assert_eq!(UI_DEV_DESTROY as u64, 0x5502);
            assert_eq!(UI_SET_EVBIT as u64, 0x4004_5564);
            assert_eq!(UI_SET_KEYBIT as u64, 0x4004_5565);
            assert_eq!(UI_DEV_SETUP as u64, 0x405c_5503);
        }
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<InputAbsInfo>(), 24);
    }

    #[test]
    fn key_names_resolve() {
        assert_eq!(key_name_to_code("ctrl"), Some(29));
        assert_eq!(key_name_to_code("Shift"), Some(42));
        assert_eq!(key_name_to_code("KEY_A"), Some(30));
        assert_eq!(key_name_to_code("F12"), Some(88));
        assert_eq!(key_name_to_code("Enter"), Some(28));
        assert_eq!(key_name_to_code("0"), Some(11));
        assert_eq!(key_name_to_code("nonsense"), None);
    }

    #[test]
    fn pressed_ids_decodes_keys_and_pov() {
        let mut raw = RawState {
            keys: [0; KEY_BYTES],
            hat_x: 1,
            hat_y: -1,
        };
        let code = 0x126u16; // stick_h1_up
        raw.keys[(code / 8) as usize] |= 1 << (code % 8);
        let ids = pressed_ids(&catalog_for_device(DeviceKind::Stick), &raw);
        let expected: HashSet<String> = ["stick_h1_up", "stick_pov_up", "stick_pov_right"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn monitor_lifecycle() {
        let m = ButtonMonitor::new();
        assert!(!m.is_active());
        m.set_active(true);
        assert!(m.is_active());
        m.set_active(false); // evita inyectar teclas reales durante el test
        assert!(!m.is_active());
        let mut p = ProgrammingProfile::default();
        p.set_cell(
            ModeSlot::M1,
            CommandCell {
                button_id: "stick_trigger".into(),
                name: "x".into(),
                keys: vec!["A".into()],
            },
        );
        m.set_profile(p.clone());
        assert_eq!(m.get_profile(), p);
        let _ = m.snapshot();
        let _ = m.uinput_available();
        assert!(m.drain_events().is_empty());
        drop(m);
    }
}
