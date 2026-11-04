//! Typed decoding of register payloads.
//!
//! Bit layouts follow the original `kefdsp`/`kefctl` reverse engineering, which
//! indexes bit strings most-significant-first. So "bit 0" in those docs is
//! `0x80` here, not `0x01`.

use crate::proto::Reply;

/// A slider register stores `table_index + 128`. `0xff` means the feature does
/// not exist on this model.
pub const SLIDER_BIAS: u8 = 128;
/// Marker value for "this model has no such control".
pub const UNSUPPORTED: u8 = 0xff;

/// Desk / wall boundary compensation, -6.0 dB .. 0 dB in 0.5 dB steps.
pub const DESK_WALL_DB: [&str; 13] = [
    "-6.0", "-5.5", "-5.0", "-4.5", "-4.0", "-3.5", "-3.0", "-2.5", "-2.0", "-1.5", "-1.0", "-0.5", "0",
];
/// Treble trim, -2.0 .. +2.0 dB in 0.5 dB steps. LS50 Wireless only.
pub const TREBLE_DB: [&str; 9] = [
    "-2.0", "-1.5", "-1.0", "-0.5", "0", "+0.5", "+1.0", "+1.5", "+2.0",
];
/// Sub out high-pass, 50..120 Hz in 5 Hz steps. LS50 Wireless only.
pub const HIGH_HZ: [&str; 15] = [
    "50", "55", "60", "65", "70", "75", "80", "85", "90", "95", "100", "105", "110", "115", "120",
];
/// Sub out low-pass, 40..250 Hz in 5 Hz steps. LS50 Wireless only.
pub const LOW_HZ: [&str; 43] = [
    "40", "45", "50", "55", "60", "65", "70", "75", "80", "85", "90", "95", "100", "105", "110",
    "115", "120", "125", "130", "135", "140", "145", "150", "155", "160", "165", "170", "175",
    "180", "185", "190", "195", "200", "205", "210", "215", "220", "225", "230", "235", "240",
    "245", "250",
];
/// Sub gain, -10 .. +10 dB in 1 dB steps. LS50 Wireless only.
pub const SUB_DB: [&str; 21] = [
    "-10.0", "-9.0", "-8.0", "-7.0", "-6.0", "-5.0", "-4.0", "-3.0", "-2.0", "-1.0", "0", "+1.0",
    "+2.0", "+3.0", "+4.0", "+5.0", "+6.0", "+7.0", "+8.0", "+9.0", "+10.0",
];

/// Decode a slider register value against its table.
pub fn slider(value: u8, table: &[&'static str]) -> String {
    if value == UNSUPPORTED {
        return "unsupported".into();
    }
    match table.get(value.wrapping_sub(SLIDER_BIAS) as usize) {
        Some(v) => (*v).to_string(),
        None => format!("out-of-range (raw 0x{value:02x})"),
    }
}

/// Encode a slider value by table index.
pub fn slider_encode(index: usize) -> Option<u8> {
    if index < 128 {
        Some(index as u8 + SLIDER_BIAS)
    } else {
        None
    }
}

/// Input source codes, low nibble of the `0x30` bitfield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Wifi,
    Usb,
    BluetoothPaired,
    BluetoothUnpaired,
    Aux,
    Optical,
    Unknown(u8),
}

impl Input {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x2 => Input::Wifi,
            0xc => Input::Usb,
            0x9 => Input::BluetoothPaired,
            0xf => Input::BluetoothUnpaired,
            0xa => Input::Aux,
            0xb => Input::Optical,
            other => Input::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Input::Wifi => 0x2,
            Input::Usb => 0xc,
            Input::BluetoothPaired => 0x9,
            Input::BluetoothUnpaired => 0xf,
            Input::Aux => 0xa,
            Input::Optical => 0xb,
            Input::Unknown(c) => c,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "wifi" | "wi-fi" => Input::Wifi,
            "usb" => Input::Usb,
            "bluetooth" | "bt" => Input::BluetoothPaired,
            "aux" => Input::Aux,
            "optical" | "opt" => Input::Optical,
            _ => return None,
        })
    }
}

impl std::fmt::Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Input::Wifi => "wifi",
            Input::Usb => "usb",
            Input::BluetoothPaired => "bluetooth",
            Input::BluetoothUnpaired => "bluetooth (unpaired)",
            Input::Aux => "aux",
            Input::Optical => "optical",
            Input::Unknown(c) => return write!(f, "unknown(0x{c:x})"),
        };
        f.write_str(s)
    }
}

/// Auto-standby timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standby {
    TwentyMinutes,
    SixtyMinutes,
    Never,
    /// A value the firmware accepted but that has no documented meaning.
    /// Preserved so a read-modify-write does not silently rewrite it.
    Unknown(u8),
}

impl Standby {
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0b00 => Standby::TwentyMinutes,
            0b01 => Standby::SixtyMinutes,
            0b10 => Standby::Never,
            other => Standby::Unknown(other),
        }
    }

    pub fn bits(self) -> u8 {
        match self {
            Standby::TwentyMinutes => 0b00,
            Standby::SixtyMinutes => 0b01,
            Standby::Never => 0b10,
            Standby::Unknown(v) => v & 0b11,
        }
    }
}

impl std::fmt::Display for Standby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Standby::TwentyMinutes => "20 min",
            Standby::SixtyMinutes => "60 min",
            Standby::Never => "never",
            Standby::Unknown(v) => return write!(f, "unknown(0b{v:02b})"),
        })
    }
}

/// The `0x30` source bitfield.
///
/// ```text
/// 0x80  power     1 = off
/// 0x40  invert L/R
/// 0x30  standby   00 = 20 min, 01 = 60 min, 10 = never
/// 0x0f  input     see Input
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub off: bool,
    pub invert: bool,
    pub standby: Standby,
    pub input: Input,
}

impl Source {
    pub fn decode(byte: u8) -> Self {
        Source {
            off: byte & 0x80 != 0,
            invert: byte & 0x40 != 0,
            standby: Standby::from_bits((byte & 0x30) >> 4),
            input: Input::from_code(byte & 0x0f),
        }
    }

    pub fn encode(self) -> u8 {
        (if self.off { 0x80 } else { 0 })
            | (if self.invert { 0x40 } else { 0 })
            | (self.standby.bits() << 4)
            | (self.input.code() & 0x0f)
    }

    /// Setting an input always implies powering on; the app behaves this way too.
    pub fn with_input(mut self, input: Input) -> Self {
        self.input = input;
        self.off = false;
        self
    }
}

/// The `0x27` DSP mode bitfield (LS50 Wireless; the LSX reports `0xff`).
///
/// ```text
/// 0x80  desk mode
/// 0x40  wall mode
/// 0x20  phase correction
/// 0x10  high-pass
/// 0x0c  sub extension  00 = standard, 10 = less, 01 = extra
/// 0x02  sub polarity   1 = '-', 0 = '+'
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DspMode(pub u8);

impl DspMode {
    pub fn desk(self) -> bool {
        self.0 & 0x80 != 0
    }
    pub fn wall(self) -> bool {
        self.0 & 0x40 != 0
    }
    pub fn phase_correction(self) -> bool {
        self.0 & 0x20 != 0
    }
    pub fn high_pass(self) -> bool {
        self.0 & 0x10 != 0
    }
    pub fn sub_extension(self) -> &'static str {
        match (self.0 & 0x0c) >> 2 {
            0b00 => "standard",
            0b10 => "less",
            0b01 => "extra",
            _ => "unknown",
        }
    }
    pub fn sub_polarity(self) -> char {
        if self.0 & 0x02 != 0 {
            '-'
        } else {
            '+'
        }
    }
    pub fn is_unsupported(self) -> bool {
        self.0 == UNSUPPORTED
    }
}

/// Firmware identity as reported by register `0x10`/`0x11`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Firmware {
    pub module: Option<String>,
    pub system: Option<String>,
    pub dsp: Option<String>,
}

impl Firmware {
    pub fn parse(xml: &str) -> Self {
        Firmware {
            module: tag(xml, "module"),
            system: tag(xml, "system"),
            dsp: tag(xml, "DSP"),
        }
    }
}

/// Extract the text of the first `<name>...</name>` element.
pub fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

/// Interpret a reply as a single byte.
pub fn as_byte(reply: &Reply) -> Option<u8> {
    reply.byte()
}

/// Interpret a reply as a big-endian u16.
pub fn as_u16(reply: &Reply) -> Option<u16> {
    let p = reply.payload();
    (p.len() >= 2).then(|| u16::from_be_bytes([p[0], p[1]]))
}

/// Interpret a reply as a NUL-terminated ASCII string.
pub fn as_str(reply: &Reply) -> Option<String> {
    let p = reply.payload();
    if p.is_empty() {
        return None;
    }
    let end = p.iter().position(|b| *b == 0).unwrap_or(p.len());
    Some(String::from_utf8_lossy(&p[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_bitfield_roundtrip() {
        // Observed live: 0x12 == on, not inverted, 60 min standby, wifi.
        let s = Source::decode(0x12);
        assert!(!s.off);
        assert!(!s.invert);
        assert_eq!(s.standby, Standby::SixtyMinutes);
        assert_eq!(s.input, Input::Wifi);
        assert_eq!(s.encode(), 0x12);
    }

    #[test]
    fn source_all_values_roundtrip() {
        for byte in 0u16..=255 {
            let byte = byte as u8;
            let decoded = Source::decode(byte);
            if let Input::Unknown(_) = decoded.input {
                continue; // unknown nibbles are preserved via code()
            }
            assert_eq!(decoded.encode(), byte, "0x{byte:02x}");
        }
    }

    #[test]
    fn setting_input_powers_on() {
        let s = Source::decode(0x92); // off, 60 min, wifi
        assert!(s.off);
        let s = s.with_input(Input::Optical);
        assert!(!s.off);
        assert_eq!(s.input, Input::Optical);
    }

    #[test]
    fn sliders_decode() {
        assert_eq!(slider(0x86, &DESK_WALL_DB), "-3.0"); // index 6
        assert_eq!(slider(0x80, &DESK_WALL_DB), "-6.0");
        assert_eq!(slider(0xff, &TREBLE_DB), "unsupported");
    }

    #[test]
    fn firmware_parses_observed_payload() {
        let fw = Firmware::parse(
            "<module>p20.5202205110.105243</module><system>2.7.0</system>",
        );
        assert_eq!(fw.module.as_deref(), Some("p20.5202205110.105243"));
        assert_eq!(fw.system.as_deref(), Some("2.7.0"));
        assert_eq!(fw.dsp, None);
    }

    #[test]
    fn tag_handles_missing() {
        assert_eq!(tag("<a>1</a>", "b"), None);
        assert_eq!(tag("<a> x </a>", "a").as_deref(), Some("x"));
    }

    #[test]
    fn dsp_mode_decodes() {
        let m = DspMode(0x80 | 0x10 | 0x02);
        assert!(m.desk());
        assert!(!m.wall());
        assert!(m.high_pass());
        assert_eq!(m.sub_polarity(), '-');
        assert!(DspMode(0xff).is_unsupported());
    }
}
