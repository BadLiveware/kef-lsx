//! Register map, as far as it has been reverse-engineered from live hardware.
//!
//! Registers `0x00`-`0x0f` and a scattering of others answer with the fixed
//! `52 12 ff` "unsupported" reply. Registers marked *unsupported-on-LSX* return
//! `0xff` as their value: the register exists (the protocol answers it) but the
//! LSX hardware has no such feature. Those are the LS50 Wireless-only DSP
//! controls, which makes sense — the LSX has no subwoofer output.

/// How a register's payload should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One byte.
    U8,
    /// Two bytes, big endian.
    U16,
    /// NUL-terminated ASCII string.
    Str,
    /// ASCII, may contain XML tags.
    Xml,
    /// A packed bitfield with a documented layout.
    Bits(&'static str),
    /// Large read returned as headerless 256-byte chunks.
    Blob,
    /// Accepted by the protocol but `0xff` on this model.
    Unsupported,
    /// Observed, meaning not yet established.
    Unknown,
}

/// One register.
#[derive(Debug, Clone, Copy)]
pub struct Reg {
    pub addr: u8,
    pub name: &'static str,
    pub kind: Kind,
}

const fn r(addr: u8, name: &'static str, kind: Kind) -> Reg {
    Reg { addr, name, kind }
}

/// Registers with at least one live observation.
pub const REGS: &[Reg] = &[
    // --- identity block: no authentication of any kind ---
    r(0x10, "module_version", Kind::Xml),
    r(0x11, "module_system_dsp", Kind::Xml),
    r(0x14, "hardware_version", Kind::Xml),
    r(0x15, "serial", Kind::Xml),
    r(0x16, "serial_raw", Kind::Xml),
    r(0x17, "model", Kind::Xml),
    // --- network block ---
    r(0x20, "device_family", Kind::Str),
    r(0x21, "hostname", Kind::Str),
    r(0x22, "wifi_security", Kind::Str),
    r(0x23, "wifi_cipher", Kind::Str),
    r(0x24, "wifi_psk", Kind::Str),
    r(0x2f, "wifi_scan", Kind::Blob),
    // --- control block ---
    r(0x25, "volume", Kind::U8),
    r(0x26, "volume_related", Kind::U8),
    r(0x27, "dsp_mode", Kind::Bits("desk/wall mode bitfield")),
    r(0x28, "desk_db", Kind::U8),
    r(0x29, "wall_db", Kind::U8),
    r(0x2a, "treble_db", Kind::Unsupported),
    r(0x2b, "high_pass_hz", Kind::Unsupported),
    r(0x2c, "sub_low_pass_hz", Kind::Unsupported),
    r(0x2d, "sub_gain_db", Kind::Unsupported),
    r(0x30, "source", Kind::Bits("power/invert/standby/input")),
    r(0x31, "transport", Kind::U8),
    // --- aliases: 0x3a/0x3b/0x3c answer as 0x2f/0x30/0x31 ---
    r(0x3a, "alias_of_2f", Kind::Unknown),
    r(0x3b, "alias_of_30", Kind::Unknown),
    r(0x3c, "alias_of_31", Kind::Unknown),
];

/// Look a register up by address.
pub fn by_addr(addr: u8) -> Option<Reg> {
    REGS.iter().copied().find(|r| r.addr == addr)
}

/// Look a register up by name.
pub fn by_name(name: &str) -> Option<Reg> {
    REGS
        .iter()
        .copied()
        .find(|r| r.name.eq_ignore_ascii_case(name))
}

/// Parse a register given either a name or `0x`-prefixed / bare hex.
pub fn parse(spec: &str) -> anyhow::Result<u8> {
    if let Some(r) = by_name(spec) {
        return Ok(r.addr);
    }
    let t = spec.trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(t, 16)
        .map_err(|_| anyhow::anyhow!("not a register name or hex address: {spec:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_names_and_hex() {
        assert_eq!(parse("volume").unwrap(), 0x25);
        assert_eq!(parse("0x30").unwrap(), 0x30);
        assert_eq!(parse("30").unwrap(), 0x30);
        assert!(parse("nope").is_err());
    }

    #[test]
    fn table_is_sorted_and_unique() {
        let mut seen = Vec::new();
        for r in REGS {
            assert!(!seen.contains(&r.addr), "duplicate 0x{:02x}", r.addr);
            seen.push(r.addr);
        }
    }
}
