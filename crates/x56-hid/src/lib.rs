//! Capa de transporte HID (`hidraw`) para Saitek Pro Flight X-56 Rhino y Logitech G X56.
//!
//! # Reglas de seguridad de bajo nivel
//! - **Nunca** se abre ni interactúa con la Interfaz 0 (HID Input / Joystick),
//!   garantizando que el kernel (`usbhid`/`joydev`/`evdev`) mantenga el control
//!   ininterrumpido de los ejes y botones durante el vuelo.
//! - Toda comunicación de configuración se realiza **exclusivamente** sobre la
//!   Interfaz 2 (`interface_number == 2`, Vendor Usage Page `0xFF00`).

use std::ffi::{CStr, CString};
use std::fmt;

use hidapi::{DeviceInfo, HidApi, HidDevice, HidError};
use x56_proto::{
    AxisConfig, PacketAxisConfig, PacketAxisQuery, PacketCommit, PacketRgb, ProtoError, RgbColor,
    AXIS_RESPONSE_MIN_LEN, PACKET_LEN, REPORT_ID_AXIS_RESPONSE,
};

/// Vendor ID de Saitek / Mad Catz.
pub const SAITEK_VID: u16 = 0x0738;
/// Product ID del Saitek Pro Flight X-56 Rhino Stick.
pub const SAITEK_STICK_PID: u16 = 0x2221;
/// Product ID del Saitek Pro Flight X-56 Rhino Throttle.
pub const SAITEK_THROTTLE_PID: u16 = 0xA221;

/// Vendor ID de Logitech.
pub const LOGITECH_VID: u16 = 0x046D;
/// Product ID del Logitech G X56 Stick.
pub const LOGITECH_STICK_PID: u16 = 0xC22B;
/// Product ID del Logitech G X56 Throttle.
pub const LOGITECH_THROTTLE_PID: u16 = 0xC22A;

/// Interfaz USB de entrada estándar (Joystick/Botones) — **PROHIBIDO TOCAR**.
pub const INPUT_INTERFACE_NUMBER: i32 = 0;
/// Interfaz USB de configuración propietaria del X56.
pub const VENDOR_INTERFACE_NUMBER: i32 = 2;
/// Usage Page propietario de configuración en el descriptor HID de la Interfaz 2.
pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;

/// Tipo de unidad física del HOTAS X56.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    /// Palanca de mando (Stick).
    Stick,
    /// Palanca de gases (Throttle).
    Throttle,
}

impl fmt::Display for DeviceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stick => f.write_str("Stick"),
            Self::Throttle => f.write_str("Throttle"),
        }
    }
}

/// Fabricante / revisión de hardware del X56.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceVendor {
    /// Saitek / Mad Catz Pro Flight X-56 Rhino (`0738:2221` / `0738:a221`).
    Saitek,
    /// Logitech G X56 (`046d:c22b` / `046d:c22a`).
    Logitech,
}

impl fmt::Display for DeviceVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Saitek => f.write_str("Saitek"),
            Self::Logitech => f.write_str("Logitech"),
        }
    }
}

/// Identifica si un par `(vendor_id, product_id)` pertenece a la familia X56.
pub const fn identify_device(
    vendor_id: u16,
    product_id: u16,
) -> Option<(DeviceVendor, DeviceKind)> {
    match (vendor_id, product_id) {
        (SAITEK_VID, SAITEK_STICK_PID) => Some((DeviceVendor::Saitek, DeviceKind::Stick)),
        (SAITEK_VID, SAITEK_THROTTLE_PID) => Some((DeviceVendor::Saitek, DeviceKind::Throttle)),
        (LOGITECH_VID, LOGITECH_STICK_PID) => Some((DeviceVendor::Logitech, DeviceKind::Stick)),
        (LOGITECH_VID, LOGITECH_THROTTLE_PID) => {
            Some((DeviceVendor::Logitech, DeviceKind::Throttle))
        }
        _ => None,
    }
}

/// Verifica estrictamente que la interfaz corresponda a la Interfaz 2 (Vendor Config)
/// y **nunca** a la Interfaz 0 (HID Input / Joystick).
pub const fn is_vendor_config_interface(interface_number: i32, usage_page: u16) -> bool {
    if interface_number == INPUT_INTERFACE_NUMBER {
        return false;
    }
    interface_number == VENDOR_INTERFACE_NUMBER || usage_page == VENDOR_USAGE_PAGE
}

/// Descriptor de un dispositivo X56 detectado en la Interfaz 2 (Vendor Config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X56Descriptor {
    /// Ruta del nodo `hidraw` en el sistema (p. ej. `/dev/hidraw8`).
    pub path: CString,
    /// USB Vendor ID (`0x0738` o `0x046d`).
    pub vendor_id: u16,
    /// USB Product ID.
    pub product_id: u16,
    /// Variante del fabricante (Saitek o Logitech).
    pub vendor: DeviceVendor,
    /// Rol del dispositivo (Stick o Throttle).
    pub kind: DeviceKind,
    /// Número de interfaz USB (siempre `2` en dispositivos enumerados válidos).
    pub interface_number: i32,
    /// Usage Page reportado por el descriptor HID (`0xFF00`).
    pub usage_page: u16,
    /// Número de serie USB, si está disponible.
    pub serial_number: Option<String>,
    /// Nombre del producto reportado por el dispositivo.
    pub product_string: Option<String>,
    /// Nombre del fabricante reportado por el dispositivo.
    pub manufacturer_string: Option<String>,
}

impl X56Descriptor {
    fn from_device_info(info: &DeviceInfo) -> Option<Self> {
        let (vendor, kind) = identify_device(info.vendor_id(), info.product_id())?;
        let interface_number = info.interface_number();
        let usage_page = info.usage_page();

        if !is_vendor_config_interface(interface_number, usage_page) {
            return None;
        }

        Some(Self {
            path: info.path().to_owned(),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            vendor,
            kind,
            interface_number,
            usage_page,
            serial_number: info.serial_number().map(str::to_owned),
            product_string: info.product_string().map(str::to_owned),
            manufacturer_string: info.manufacturer_string().map(str::to_owned),
        })
    }
}

/// Errores de la capa de transporte HID del X56.
#[derive(Debug)]
pub enum HidTransportError {
    /// Error subyacente de `hidapi` / `hidraw`.
    Hid(HidError),
    /// Error de validación o codificación/decodificación de `x56_proto`.
    Proto(ProtoError),
    /// No se encontró ningún dispositivo X56 del tipo solicitado en la Interfaz 2.
    DeviceNotFound(DeviceKind),
    /// El dispositivo no pertenece a ningún par VID:PID soportado del X56.
    UnsupportedDevice { vendor_id: u16, product_id: u16 },
    /// Se intentó abrir una interfaz prohibida (p. ej. Interfaz 0 de joystick).
    ForbiddenInterface {
        interface_number: i32,
        usage_page: u16,
    },
    /// El feature report devuelto por el hardware fue más corto de lo esperado.
    ShortTransfer { expected: usize, actual: usize },
}

impl fmt::Display for HidTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hid(err) => write!(f, "HID transport error: {err}"),
            Self::Proto(err) => write!(f, "protocol error: {err}"),
            Self::DeviceNotFound(kind) => {
                write!(
                    f,
                    "X56 {kind} vendor config interface (interface 2) not found"
                )
            }
            Self::UnsupportedDevice {
                vendor_id,
                product_id,
            } => {
                write!(
                    f,
                    "unsupported USB device {vendor_id:04x}:{product_id:04x} (not an X56)"
                )
            }
            Self::ForbiddenInterface {
                interface_number,
                usage_page,
            } => {
                write!(
                    f,
                    "refusing to access interface {interface_number} (usage_page={usage_page:#06x}): only interface 2 (0xFF00) is allowed"
                )
            }
            Self::ShortTransfer { expected, actual } => {
                write!(
                    f,
                    "short HID feature report transfer: expected at least {expected} bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for HidTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Hid(err) => Some(err),
            Self::Proto(err) => Some(err),
            _ => None,
        }
    }
}

impl From<HidError> for HidTransportError {
    fn from(err: HidError) -> Self {
        Self::Hid(err)
    }
}

impl From<ProtoError> for HidTransportError {
    fn from(err: ProtoError) -> Self {
        Self::Proto(err)
    }
}

/// Dispositivo X56 abierto exclusivamente sobre la Interfaz 2 (Vendor Config).
pub struct X56Device {
    device: HidDevice,
    descriptor: X56Descriptor,
}

impl fmt::Debug for X56Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X56Device")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl X56Device {
    /// Enumera todos los dispositivos X56 disponibles en el sistema filtrando
    /// **exclusivamente** la Interfaz 2 (Vendor Config, UsagePage `0xFF00`).
    ///
    /// Nunca incluye nodos correspondientes a la Interfaz 0 (Joystick).
    pub fn enumerate() -> Vec<X56Descriptor> {
        Self::try_enumerate().unwrap_or_default()
    }

    /// Versión falible de [`Self::enumerate`] que propaga errores de inicialización de `hidapi`.
    pub fn try_enumerate() -> Result<Vec<X56Descriptor>, HidTransportError> {
        let api = HidApi::new()?;
        let descriptors = api
            .device_list()
            .filter_map(X56Descriptor::from_device_info)
            .collect();
        Ok(descriptors)
    }

    /// Abre el primer dispositivo X56 del tipo indicado (`Stick` o `Throttle`)
    /// conectando exclusivamente a su Interfaz 2.
    pub fn open(kind: DeviceKind) -> Result<Self, HidTransportError> {
        let api = HidApi::new()?;
        let descriptor = api
            .device_list()
            .filter_map(X56Descriptor::from_device_info)
            .find(|d| d.kind == kind)
            .ok_or(HidTransportError::DeviceNotFound(kind))?;

        let device = api.open_path(descriptor.path.as_c_str())?;
        Ok(Self { device, descriptor })
    }

    /// Abre un nodo `hidraw` específico por su ruta (p. ej. `/dev/hidraw8`),
    /// verificando **antes y después** que sea un dispositivo X56 válido y que
    /// pertenezca exclusivamente a la Interfaz 2 (nunca a la Interfaz 0).
    pub fn open_path(path: &CStr) -> Result<Self, HidTransportError> {
        let api = HidApi::new()?;

        // Si el nodo aparece en la lista de enumeración, validamos ANTES de hacer open()
        // para jamás abrir el file descriptor de la Interfaz 0.
        if let Some(info) = api.device_list().find(|d| d.path() == path) {
            Self::validate_device_info(info)?;
        }

        let device = api.open_path(path)?;
        let info = device.get_device_info()?;
        let descriptor = Self::validate_device_info(&info)?;

        Ok(Self { device, descriptor })
    }

    /// Devuelve el descriptor asociado al dispositivo abierto.
    pub fn descriptor(&self) -> &X56Descriptor {
        &self.descriptor
    }

    /// Devuelve el tipo de dispositivo (`Stick` o `Throttle`).
    pub fn kind(&self) -> DeviceKind {
        self.descriptor.kind
    }

    /// Devuelve el fabricante del dispositivo (`Saitek` o `Logitech`).
    pub fn vendor(&self) -> DeviceVendor {
        self.descriptor.vendor
    }

    /// Configura el color de iluminación LED RGB enviando [`PacketRgb`] (Report `0x09`)
    /// seguido de [`PacketCommit`] (Report `0x01`) como Feature Reports.
    pub fn set_rgb(&self, color: RgbColor) -> Result<(), HidTransportError> {
        let rgb_pkt = PacketRgb::new(color).to_bytes();
        let commit_pkt = PacketCommit.to_bytes();

        self.device.send_feature_report(&rgb_pkt)?;
        self.device.send_feature_report(&commit_pkt)?;
        Ok(())
    }

    /// Obtiene la configuración actual de un eje (`axis_id`).
    ///
    /// Envía primero [`PacketAxisQuery`] (Report `0x0b`, subcomando `0x02`) para seleccionar
    /// el eje y a continuación solicita el Feature Report `0x08` ([`REPORT_ID_AXIS_RESPONSE`]),
    /// decodificando la respuesta con [`AxisConfig::decode`].
    pub fn get_axis_config(&self, axis_id: u8) -> Result<AxisConfig, HidTransportError> {
        let query_pkt = PacketAxisQuery::new(axis_id).to_bytes();
        self.device.send_feature_report(&query_pkt)?;

        let mut buf = [0u8; PACKET_LEN];
        buf[0] = REPORT_ID_AXIS_RESPONSE;
        let len = self.device.get_feature_report(&mut buf)?;
        if len < AXIS_RESPONSE_MIN_LEN {
            return Err(HidTransportError::ShortTransfer {
                expected: AXIS_RESPONSE_MIN_LEN,
                actual: len,
            });
        }

        let cfg = AxisConfig::decode(&buf[..len])?;
        Ok(cfg)
    }

    /// Aplica la configuración de un eje enviando [`PacketAxisConfig`] (Report `0x0b`)
    /// seguido de [`PacketCommit`] (Report `0x01`) como Feature Reports.
    pub fn set_axis_config(&self, config: &AxisConfig) -> Result<(), HidTransportError> {
        let axis_pkt = PacketAxisConfig::new(*config).to_bytes()?;
        let commit_pkt = PacketCommit.to_bytes();

        self.device.send_feature_report(&axis_pkt)?;
        self.device.send_feature_report(&commit_pkt)?;
        Ok(())
    }

    fn validate_device_info(info: &DeviceInfo) -> Result<X56Descriptor, HidTransportError> {
        let (vendor, kind) = identify_device(info.vendor_id(), info.product_id()).ok_or(
            HidTransportError::UnsupportedDevice {
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
            },
        )?;

        let interface_number = info.interface_number();
        let usage_page = info.usage_page();
        if !is_vendor_config_interface(interface_number, usage_page) {
            return Err(HidTransportError::ForbiddenInterface {
                interface_number,
                usage_page,
            });
        }

        Ok(X56Descriptor {
            path: info.path().to_owned(),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            vendor,
            kind,
            interface_number,
            usage_page,
            serial_number: info.serial_number().map(str::to_owned),
            product_string: info.product_string().map(str::to_owned),
            manufacturer_string: info.manufacturer_string().map(str::to_owned),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_identification_matrix() {
        assert_eq!(
            identify_device(0x0738, 0x2221),
            Some((DeviceVendor::Saitek, DeviceKind::Stick))
        );
        assert_eq!(
            identify_device(0x0738, 0xa221),
            Some((DeviceVendor::Saitek, DeviceKind::Throttle))
        );
        assert_eq!(
            identify_device(0x046d, 0xc22b),
            Some((DeviceVendor::Logitech, DeviceKind::Stick))
        );
        assert_eq!(
            identify_device(0x046d, 0xc22a),
            Some((DeviceVendor::Logitech, DeviceKind::Throttle))
        );
        assert_eq!(identify_device(0x0738, 0x0000), None);
        assert_eq!(identify_device(0x1234, 0x5678), None);
    }

    #[test]
    fn interface_filter_never_allows_interface_0() {
        // Interfaz 0 siempre rechazada, incluso si usage_page fuese 0xFF00.
        assert!(!is_vendor_config_interface(0, 0x0001));
        assert!(!is_vendor_config_interface(0, 0xFF00));
        assert!(!is_vendor_config_interface(0, 0x0000));

        // Interfaz 2 o UsagePage 0xFF00 (con interfaz != 0) permitidas.
        assert!(is_vendor_config_interface(2, 0xFF00));
        assert!(is_vendor_config_interface(2, 0x0000));
        assert!(is_vendor_config_interface(-1, 0xFF00));

        // Otras interfaces sin UsagePage 0xFF00 rechazadas.
        assert!(!is_vendor_config_interface(1, 0x0001));
        assert!(!is_vendor_config_interface(-1, 0x0001));
    }

    #[test]
    fn hardware_enumerate_and_read_if_present() {
        let devices = X56Device::enumerate();
        if devices.is_empty() {
            eprintln!(
                "[skip] No se detectaron dispositivos X56 en Interfaz 2; omitiendo test de hardware."
            );
            return;
        }

        for desc in &devices {
            // Regla crítica 1 y 2: jamás debe colarse la Interfaz 0.
            assert_ne!(
                desc.interface_number, INPUT_INTERFACE_NUMBER,
                "Interfaz 0 nunca debe aparecer en X56Device::enumerate()"
            );
            assert!(
                is_vendor_config_interface(desc.interface_number, desc.usage_page),
                "Descriptor inválido en enumerate(): {desc:?}"
            );

            match X56Device::open_path(desc.path.as_c_str()) {
                Ok(dev) => {
                    assert_eq!(dev.kind(), desc.kind);
                    assert_eq!(dev.vendor(), desc.vendor);

                    // En el Stick consultamos el eje X (0x30) de solo lectura.
                    if dev.kind() == DeviceKind::Stick {
                        let cfg = dev
                            .get_axis_config(0x30)
                            .expect("falló get_axis_config(0x30) en Stick presente");
                        assert!(cfg.xsat <= 1000);
                        assert!(cfg.ysat <= 1000);
                        assert!(cfg.deadband <= 1000);
                        assert!(cfg.curvature <= 1000);
                    }
                }
                Err(HidTransportError::Hid(err)) => {
                    eprintln!(
                        "[skip] Dispositivo {:?} detectado en {:?} pero sin permisos de apertura ({err}); omitiendo prueba de E/S.",
                        desc.kind, desc.path
                    );
                }
                Err(other) => panic!("Error inesperado abriendo {:?}: {other}", desc.path),
            }
        }
    }

    #[test]
    fn open_path_rejects_interface_0_if_hardware_present() {
        let Ok(api) = HidApi::new() else {
            return;
        };

        for info in api.device_list() {
            if identify_device(info.vendor_id(), info.product_id()).is_some()
                && info.interface_number() == INPUT_INTERFACE_NUMBER
            {
                let res = X56Device::open_path(info.path());
                assert!(
                    matches!(
                        res,
                        Err(HidTransportError::ForbiddenInterface {
                            interface_number: 0,
                            ..
                        })
                    ),
                    "open_path debe rechazar tajantemente la Interfaz 0: {res:?}"
                );
            }
        }
    }
}
