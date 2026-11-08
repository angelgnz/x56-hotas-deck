//! Protocolo binario HID del Saitek X56 (sin dependencias externas).
//!
//! Todos los enteros de 16 bits viajan en Big-Endian.

use core::fmt;

/// Tamaño de los reports de salida / feature set.
pub const PACKET_LEN: usize = 64;

pub const REPORT_ID_COMMIT: u8 = 0x01;
pub const REPORT_ID_RGB: u8 = 0x09;
pub const REPORT_ID_AXIS: u8 = 0x0b;
pub const REPORT_ID_AXIS_RESPONSE: u8 = 0x08;

/// Valor máximo permitido para xsat/ysat/deadband/curvature.
pub const AXIS_PARAM_MAX: u16 = 1000;

/// Longitud mínima de la respuesta (Report 0x08) para contener todos los campos.
pub const AXIS_RESPONSE_MIN_LEN: usize = 38;

// Offsets en la respuesta Report 0x08 (byte 0 = Report ID).
const RESP_AXIS_ID: usize = 22;
const RESP_XSAT: usize = 27;
const RESP_YSAT: usize = 29;
const RESP_DEADBAND: usize = 31;
const RESP_CURVATURE: usize = 33;
const RESP_PROFILE: usize = 35;
const RESP_HALL_ZERO: usize = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    /// Buffer demasiado corto.
    BufferTooShort { expected: usize, got: usize },
    /// Report ID inesperado.
    UnexpectedReportId { expected: u8, got: u8 },
    /// Cabecera/constante del paquete inválida.
    InvalidHeader {
        offset: usize,
        expected: u8,
        got: u8,
    },
    /// Parámetro fuera del rango permitido.
    OutOfRange {
        field: &'static str,
        value: u16,
        max: u16,
    },
    /// Valor de perfil de curva desconocido.
    InvalidProfile(u8),
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooShort { expected, got } => {
                write!(
                    f,
                    "buffer too short: expected at least {expected} bytes, got {got}"
                )
            }
            Self::UnexpectedReportId { expected, got } => {
                write!(
                    f,
                    "unexpected report id: expected {expected:#04x}, got {got:#04x}"
                )
            }
            Self::InvalidHeader {
                offset,
                expected,
                got,
            } => write!(
                f,
                "invalid header byte at offset {offset}: expected {expected:#04x}, got {got:#04x}"
            ),
            Self::OutOfRange { field, value, max } => {
                write!(f, "{field} out of range: {value} (allowed 0..={max})")
            }
            Self::InvalidProfile(v) => write!(f, "invalid curve profile: {v:#04x}"),
        }
    }
}

impl std::error::Error for ProtoError {}

fn check_len(buf: &[u8], expected: usize) -> Result<(), ProtoError> {
    if buf.len() < expected {
        Err(ProtoError::BufferTooShort {
            expected,
            got: buf.len(),
        })
    } else {
        Ok(())
    }
}

fn check_report_id(buf: &[u8], expected: u8) -> Result<(), ProtoError> {
    check_len(buf, 1)?;
    if buf[0] != expected {
        return Err(ProtoError::UnexpectedReportId {
            expected,
            got: buf[0],
        });
    }
    Ok(())
}

fn check_range(field: &'static str, value: u16) -> Result<u16, ProtoError> {
    if value > AXIS_PARAM_MAX {
        Err(ProtoError::OutOfRange {
            field,
            value,
            max: AXIS_PARAM_MAX,
        })
    } else {
        Ok(value)
    }
}

fn be16(buf: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

fn put_be16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

// ---------------------------------------------------------------------------
// RGB
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Packet 5: Set RGB (Report 0x09). Debe ir seguido de [`PacketCommit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketRgb {
    pub color: RgbColor,
}

impl PacketRgb {
    pub const fn new(color: RgbColor) -> Self {
        Self { color }
    }

    pub fn encode(&self, buf: &mut [u8; PACKET_LEN]) {
        *buf = [0; PACKET_LEN];
        buf[0] = REPORT_ID_RGB;
        buf[1] = 0x00;
        buf[2] = 0x03;
        buf[3] = self.color.r;
        buf[4] = self.color.g;
        buf[5] = self.color.b;
    }

    pub fn to_bytes(&self) -> [u8; PACKET_LEN] {
        let mut b = [0; PACKET_LEN];
        self.encode(&mut b);
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtoError> {
        check_len(buf, 6)?;
        check_report_id(buf, REPORT_ID_RGB)?;
        if buf[1] != 0x00 {
            return Err(ProtoError::InvalidHeader {
                offset: 1,
                expected: 0x00,
                got: buf[1],
            });
        }
        if buf[2] != 0x03 {
            return Err(ProtoError::InvalidHeader {
                offset: 2,
                expected: 0x03,
                got: buf[2],
            });
        }
        Ok(Self {
            color: RgbColor::new(buf[3], buf[4], buf[5]),
        })
    }
}

/// Packet 4: Commit/End (Report 0x01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PacketCommit;

impl PacketCommit {
    pub fn encode(&self, buf: &mut [u8; PACKET_LEN]) {
        *buf = [0; PACKET_LEN];
        buf[0] = REPORT_ID_COMMIT;
        buf[1] = 0x01;
    }

    pub fn to_bytes(&self) -> [u8; PACKET_LEN] {
        let mut b = [0; PACKET_LEN];
        self.encode(&mut b);
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtoError> {
        check_len(buf, 2)?;
        check_report_id(buf, REPORT_ID_COMMIT)?;
        if buf[1] != 0x01 {
            return Err(ProtoError::InvalidHeader {
                offset: 1,
                expected: 0x01,
                got: buf[1],
            });
        }
        Ok(Self)
    }
}

// ---------------------------------------------------------------------------
// Ejes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CurveProfile {
    /// Curva en S.
    #[default]
    S = 0,
    /// Curva en J.
    J = 1,
}

impl CurveProfile {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for CurveProfile {
    type Error = ProtoError;

    fn try_from(v: u8) -> Result<Self, ProtoError> {
        match v {
            0 => Ok(Self::S),
            1 => Ok(Self::J),
            other => Err(ProtoError::InvalidProfile(other)),
        }
    }
}

/// Configuración de un eje. `xsat`, `ysat`, `deadband` y `curvature` están en `0..=1000`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisConfig {
    pub axis_id: u8,
    pub xsat: u16,
    pub ysat: u16,
    pub deadband: u16,
    pub curvature: u16,
    pub profile: CurveProfile,
    pub hall_zero: u16,
}

impl AxisConfig {
    /// Construye validando rangos.
    pub fn new(
        axis_id: u8,
        xsat: u16,
        ysat: u16,
        deadband: u16,
        curvature: u16,
        profile: CurveProfile,
        hall_zero: u16,
    ) -> Result<Self, ProtoError> {
        let cfg = Self {
            axis_id,
            xsat,
            ysat,
            deadband,
            curvature,
            profile,
            hall_zero,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ProtoError> {
        check_range("xsat", self.xsat)?;
        check_range("ysat", self.ysat)?;
        check_range("deadband", self.deadband)?;
        check_range("curvature", self.curvature)?;
        Ok(())
    }

    /// Decodifica la respuesta Feature Report 0x08 (Packet 6/8).
    pub fn decode(buf: &[u8]) -> Result<Self, ProtoError> {
        check_report_id(buf, REPORT_ID_AXIS_RESPONSE)?;
        check_len(buf, AXIS_RESPONSE_MIN_LEN)?;
        Self::new(
            buf[RESP_AXIS_ID],
            be16(buf, RESP_XSAT),
            be16(buf, RESP_YSAT),
            be16(buf, RESP_DEADBAND),
            be16(buf, RESP_CURVATURE),
            CurveProfile::try_from(buf[RESP_PROFILE])?,
            be16(buf, RESP_HALL_ZERO),
        )
    }
}

/// Packet 3: Set Axis Configuration (Report 0x0b, feature set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketAxisConfig {
    pub config: AxisConfig,
}

impl PacketAxisConfig {
    pub const fn new(config: AxisConfig) -> Self {
        Self { config }
    }

    /// Codifica el paquete; falla si algún parámetro está fuera de rango.
    pub fn encode(&self, buf: &mut [u8; PACKET_LEN]) -> Result<(), ProtoError> {
        let c = &self.config;
        c.validate()?;
        *buf = [0; PACKET_LEN];
        buf[0] = REPORT_ID_AXIS;
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = c.axis_id;
        buf[4] = 0x00;
        buf[5] = 0x01;
        buf[6] = 0x00;
        buf[7] = c.axis_id;
        put_be16(buf, 8, c.xsat);
        put_be16(buf, 10, c.ysat);
        put_be16(buf, 12, c.deadband);
        put_be16(buf, 14, c.curvature);
        buf[16] = c.profile.as_u8();
        put_be16(buf, 17, c.hall_zero);
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<[u8; PACKET_LEN], ProtoError> {
        let mut b = [0; PACKET_LEN];
        self.encode(&mut b)?;
        Ok(b)
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtoError> {
        check_report_id(buf, REPORT_ID_AXIS)?;
        check_len(buf, 19)?;
        for (off, expected) in [(1usize, 0x01u8), (2, 0x00), (4, 0x00), (5, 0x01), (6, 0x00)] {
            if buf[off] != expected {
                return Err(ProtoError::InvalidHeader {
                    offset: off,
                    expected,
                    got: buf[off],
                });
            }
        }
        if buf[7] != buf[3] {
            return Err(ProtoError::InvalidHeader {
                offset: 7,
                expected: buf[3],
                got: buf[7],
            });
        }
        let config = AxisConfig::new(
            buf[3],
            be16(buf, 8),
            be16(buf, 10),
            be16(buf, 12),
            be16(buf, 14),
            CurveProfile::try_from(buf[16])?,
            be16(buf, 17),
        )?;
        Ok(Self { config })
    }
}

/// Packet 7: Query Axis Config Request (Report 0x0b, subcomando 0x02).
///
/// Se envía como feature set; la respuesta se lee con Report 0x08 y se
/// decodifica con [`AxisConfig::decode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketAxisQuery {
    pub axis_id: u8,
}

impl PacketAxisQuery {
    pub const fn new(axis_id: u8) -> Self {
        Self { axis_id }
    }

    pub fn encode(&self, buf: &mut [u8; PACKET_LEN]) {
        *buf = [0; PACKET_LEN];
        buf[0] = REPORT_ID_AXIS;
        buf[1] = 0x02;
        buf[2] = 0x00;
        buf[3] = self.axis_id;
    }

    pub fn to_bytes(&self) -> [u8; PACKET_LEN] {
        let mut b = [0; PACKET_LEN];
        self.encode(&mut b);
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtoError> {
        check_report_id(buf, REPORT_ID_AXIS)?;
        check_len(buf, 4)?;
        if buf[1] != 0x02 {
            return Err(ProtoError::InvalidHeader {
                offset: 1,
                expected: 0x02,
                got: buf[1],
            });
        }
        if buf[2] != 0x00 {
            return Err(ProtoError::InvalidHeader {
                offset: 2,
                expected: 0x00,
                got: buf[2],
            });
        }
        Ok(Self { axis_id: buf[3] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feature Report 0x08 capturado en hardware real (Saitek X56 stick).
    const HW_CAPTURE: [u8; 64] = [
        0x08, 0xf1, 0x63, 0xa8, 0x2c, 0xdc, 0x67, 0x16, 0x4d, 0xaf, 0xd7, 0x67, 0x1a, 0x85, 0xe2,
        0x31, 0x50, 0x01, 0x3f, 0x0b, 0x02, 0x00, 0x30, 0x00, 0x01, 0x00, 0x30, 0x03, 0xe8, 0x03,
        0xe8, 0x00, 0x3c, 0x01, 0xf4, 0x00, 0x88, 0x59, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    fn sample() -> AxisConfig {
        AxisConfig::new(0x30, 1000, 1000, 60, 500, CurveProfile::S, 0x8859).unwrap()
    }

    #[test]
    fn decode_real_hardware_capture() {
        let cfg = AxisConfig::decode(&HW_CAPTURE).unwrap();
        assert_eq!(cfg.axis_id, 0x30);
        assert_eq!(cfg.xsat, 1000);
        assert_eq!(cfg.ysat, 1000);
        assert_eq!(cfg.deadband, 60);
        assert_eq!(cfg.curvature, 500);
        assert_eq!(cfg.profile, CurveProfile::S);
        assert_eq!(cfg.hall_zero, 0x8859);
        assert_eq!(cfg, sample());
    }

    #[test]
    fn decode_rejects_bad_report_id_and_short_buffer() {
        let mut b = HW_CAPTURE;
        b[0] = 0x07;
        assert_eq!(
            AxisConfig::decode(&b),
            Err(ProtoError::UnexpectedReportId {
                expected: 0x08,
                got: 0x07
            })
        );
        assert_eq!(
            AxisConfig::decode(&HW_CAPTURE[..37]),
            Err(ProtoError::BufferTooShort {
                expected: 38,
                got: 37
            })
        );
        assert!(matches!(
            AxisConfig::decode(&[]),
            Err(ProtoError::BufferTooShort { .. })
        ));
    }

    #[test]
    fn decode_rejects_invalid_profile() {
        let mut b = HW_CAPTURE;
        b[35] = 2;
        assert_eq!(AxisConfig::decode(&b), Err(ProtoError::InvalidProfile(2)));
        b[35] = 1;
        assert_eq!(AxisConfig::decode(&b).unwrap().profile, CurveProfile::J);
    }

    #[test]
    fn encode_rgb_bytes() {
        let bytes = PacketRgb::new(RgbColor::new(0xff, 0x80, 0x01)).to_bytes();
        assert_eq!(&bytes[..6], &[0x09, 0x00, 0x03, 0xff, 0x80, 0x01]);
        assert!(bytes[6..].iter().all(|&b| b == 0));
        assert_eq!(bytes.len(), 64);

        // encode() debe limpiar un buffer sucio.
        let mut dirty = [0xaa; 64];
        PacketRgb::new(RgbColor::new(1, 2, 3)).encode(&mut dirty);
        assert_eq!(&dirty[..6], &[0x09, 0x00, 0x03, 1, 2, 3]);
        assert!(dirty[6..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rgb_roundtrip_and_errors() {
        let p = PacketRgb::new(RgbColor::new(10, 20, 30));
        assert_eq!(PacketRgb::decode(&p.to_bytes()).unwrap(), p);
        assert!(matches!(
            PacketRgb::decode(&[0x09, 0, 3]),
            Err(ProtoError::BufferTooShort { .. })
        ));
        assert!(matches!(
            PacketRgb::decode(&[0x0a, 0, 3, 0, 0, 0]),
            Err(ProtoError::UnexpectedReportId { .. })
        ));
        assert!(matches!(
            PacketRgb::decode(&[0x09, 0, 4, 0, 0, 0]),
            Err(ProtoError::InvalidHeader { offset: 2, .. })
        ));
    }

    #[test]
    fn encode_commit_bytes() {
        let bytes = PacketCommit.to_bytes();
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[1], 0x01);
        assert!(bytes[2..].iter().all(|&b| b == 0));
        assert_eq!(PacketCommit::decode(&bytes).unwrap(), PacketCommit);
        assert!(PacketCommit::decode(&[0x01, 0x00]).is_err());
    }

    #[test]
    fn encode_axis_config_bytes() {
        let bytes = PacketAxisConfig::new(sample()).to_bytes().unwrap();
        assert_eq!(
            &bytes[..19],
            &[
                0x0b, 0x01, 0x00, 0x30, 0x00, 0x01, 0x00, 0x30, 0x03, 0xe8, 0x03, 0xe8, 0x00, 0x3c,
                0x01, 0xf4, 0x00, 0x88, 0x59
            ]
        );
        assert!(bytes[19..].iter().all(|&b| b == 0));
    }

    #[test]
    fn axis_config_roundtrip() {
        for profile in [CurveProfile::S, CurveProfile::J] {
            for &(x, y, d, c, hz) in &[
                (0u16, 0u16, 0u16, 0u16, 0u16),
                (1000, 1000, 60, 500, 0x8859),
                (1, 999, 1000, 0, 0xffff),
                (123, 456, 789, 1000, 0x0102),
            ] {
                let cfg = AxisConfig::new(0x31, x, y, d, c, profile, hz).unwrap();
                let bytes = PacketAxisConfig::new(cfg).to_bytes().unwrap();
                let decoded = PacketAxisConfig::decode(&bytes).unwrap();
                assert_eq!(decoded.config, cfg);
            }
        }
    }

    #[test]
    fn axis_packet_decode_rejects_malformed() {
        let good = PacketAxisConfig::new(sample()).to_bytes().unwrap();

        let mut b = good;
        b[7] = 0x31; // axis_id duplicado inconsistente
        assert!(matches!(
            PacketAxisConfig::decode(&b),
            Err(ProtoError::InvalidHeader { offset: 7, .. })
        ));

        let mut b = good;
        b[1] = 0x02;
        assert!(matches!(
            PacketAxisConfig::decode(&b),
            Err(ProtoError::InvalidHeader { offset: 1, .. })
        ));

        assert!(matches!(
            PacketAxisConfig::decode(&good[..18]),
            Err(ProtoError::BufferTooShort {
                expected: 19,
                got: 18
            })
        ));
    }

    #[test]
    fn axis_bounds() {
        // Límites válidos.
        assert!(AxisConfig::new(0x30, 0, 0, 0, 0, CurveProfile::S, 0).is_ok());
        assert!(AxisConfig::new(0x30, 1000, 1000, 1000, 1000, CurveProfile::J, 0xffff).is_ok());

        // Fuera de rango, campo por campo.
        let cases: [(&str, [u16; 4]); 4] = [
            ("xsat", [1001, 0, 0, 0]),
            ("ysat", [0, 1001, 0, 0]),
            ("deadband", [0, 0, 1001, 0]),
            ("curvature", [0, 0, 0, 1001]),
        ];
        for (field, [x, y, d, c]) in cases {
            assert_eq!(
                AxisConfig::new(0x30, x, y, d, c, CurveProfile::S, 0),
                Err(ProtoError::OutOfRange {
                    field,
                    value: 1001,
                    max: 1000
                })
            );
        }
        assert!(AxisConfig::new(0x30, u16::MAX, 0, 0, 0, CurveProfile::S, 0).is_err());
    }

    #[test]
    fn encode_rejects_out_of_range_fields_set_directly() {
        let mut cfg = sample();
        cfg.curvature = 1001;
        let mut buf = [0xaa; 64];
        assert_eq!(
            PacketAxisConfig::new(cfg).encode(&mut buf),
            Err(ProtoError::OutOfRange {
                field: "curvature",
                value: 1001,
                max: 1000
            })
        );
        assert_eq!(buf, [0xaa; 64], "no debe modificar el buffer ante error");
    }

    #[test]
    fn decode_rejects_out_of_range_from_wire() {
        let mut b = HW_CAPTURE;
        b[27] = 0x03;
        b[28] = 0xe9; // xsat = 1001
        assert_eq!(
            AxisConfig::decode(&b),
            Err(ProtoError::OutOfRange {
                field: "xsat",
                value: 1001,
                max: 1000
            })
        );
    }

    #[test]
    fn query_request_bytes_and_roundtrip() {
        let bytes = PacketAxisQuery::new(0x30).to_bytes();
        assert_eq!(&bytes[..4], &[0x0b, 0x02, 0x00, 0x30]);
        assert!(bytes[4..].iter().all(|&b| b == 0));
        assert_eq!(PacketAxisQuery::decode(&bytes).unwrap().axis_id, 0x30);
        assert!(PacketAxisQuery::decode(&[0x0b, 0x01, 0x00, 0x30]).is_err());
        assert!(PacketAxisQuery::decode(&[0x0b, 0x02]).is_err());
    }

    #[test]
    fn query_echo_matches_capture_header() {
        // Los bytes 19..23 de la respuesta real replican la query (0b 02 00 axis).
        let q = PacketAxisQuery::new(HW_CAPTURE[22]).to_bytes();
        assert_eq!(&HW_CAPTURE[19..23], &q[..4]);
    }

    #[test]
    fn error_display() {
        let e = ProtoError::OutOfRange {
            field: "xsat",
            value: 1001,
            max: 1000,
        };
        assert_eq!(e.to_string(), "xsat out of range: 1001 (allowed 0..=1000)");
    }
}
