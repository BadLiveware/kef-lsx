//! Wire format for the KEF LSX / LS50 Wireless control protocol (TCP 50001).
//!
//! Everything here is empirically derived against an LSX pair on firmware
//! `system 2.7.0` / module `p20.5202205110.105243`. See `docs/protocol.md`.
//!
//! # Request framing
//!
//! ```text
//! byte 0 : 0x47 'G' (get) or 0x53 'S' (set)
//! byte 1 : register
//! byte 2 : 0x80 + payload_len
//! byte 3+: payload (only on SET)
//! ```
//!
//! The only publicly known client (`kraih/kefctl`) never sends a checksum and
//! works, so we default to the same. [`request`] can append one on request.
//!
//! # Response framing
//!
//! ```text
//! byte 0 : 0x52 'R'
//! byte 1 : register being answered
//! byte 2 : 0x80 + payload_len   <-- this is a LENGTH, not a type tag
//! byte 3+: payload
//! last   : CRC8(poly 0x07, init 0x00) over bytes 0..last
//! ```
//!
//! Two exceptions, both observed on real hardware:
//!
//! * A 3-byte `52 <reg> ff` is a **terminal reply with no payload and no CRC**.
//!   The `0xff` length byte means "no data", not "127 bytes". Two flavours are
//!   observed:
//!     * `52 11 ff` — SET acknowledged (the write was applied).
//!     * `52 12 ff` — register not supported for this operation. It is
//!       identical for every unsupported register, which is why the register
//!       byte reads as `0x12` rather than the register you asked for.
//! * Large reads (e.g. the Wi-Fi scan blob behind register `0x2f`) come back as
//!   headerless 256-byte chunks with no framing or CRC at all.

use std::fmt;

/// TCP port the control server listens on (master speaker only).
pub const PORT: u16 = 50001;

/// Request opcode: get.
pub const OP_GET: u8 = 0x47; // 'G'
/// Request opcode: set.
pub const OP_SET: u8 = 0x53; // 'S'
/// Response opcode.
pub const OP_RESP: u8 = 0x52; // 'R'

/// Byte 2 is a length byte offset by this constant, in requests *and*
/// responses: `byte2 == LEN_BIAS + payload_len`.
///
/// This unifies what looks at first like a read/write flag. A GET carries no
/// payload so byte 2 is `0x80`; a one-byte SET carries `0x81`. The public
/// `kefctl` client hardcodes those two values, which is why the pattern went
/// unnoticed.
pub const LEN_BIAS: u8 = 0x80;

/// Register byte of the fixed-shape "unsupported" reply.
pub const REG_ERROR: u8 = 0x12;
/// Value byte of the fixed-shape "unsupported" reply.
pub const VAL_ERROR: u8 = 0xff;

/// CRC8, polynomial `0x07`, initial value `0x00`, no reflection, no final XOR.
///
/// Verified against every well-framed response captured so far.
pub fn crc8(data: &[u8]) -> u8 {
    let mut c: u8 = 0;
    for &b in data {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 { (c << 1) ^ 0x07 } else { c << 1 };
        }
    }
    c
}

/// Build a request frame.
///
/// `with_crc` appends a CRC8, matching response framing. This is *unverified*
/// against the speaker: the only known-working client omits it, so the default
/// should stay `false` until someone tests it on a device they can power-cycle.
pub fn request(op: u8, reg: u8, payload: &[u8], with_crc: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(3 + payload.len() + 1);
    v.push(op);
    v.push(reg);
    v.push(LEN_BIAS.wrapping_add(payload.len() as u8));
    v.extend_from_slice(payload);
    if with_crc {
        let c = crc8(&v);
        v.push(c);
    }
    v
}

/// A decoded reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Well-framed response whose CRC verified.
    Value { reg: u8, payload: Vec<u8> },
    /// `52 <reg> ff` — terminal reply, no payload, no CRC.
    ///
    /// `52 11 ff` is the SET acknowledgement; see [`Reply::Unsupported`] for
    /// `52 12 ff`.
    Ack { reg: u8 },
    /// `52 12 ff` — the fixed "unsupported register" reply.
    Unsupported,
    /// Well-framed but the CRC did not match.
    BadCrc {
        reg: u8,
        payload: Vec<u8>,
        got: u8,
        want: u8,
    },
    /// Headerless data, e.g. a chunk of a large blob read.
    Chunk(Vec<u8>),
}

impl Reply {
    /// Payload bytes, empty for non-value replies.
    pub fn payload(&self) -> &[u8] {
        match self {
            Reply::Value { payload, .. } | Reply::BadCrc { payload, .. } => payload,
            _ => &[],
        }
    }

    /// First payload byte, if any.
    pub fn byte(&self) -> Option<u8> {
        self.payload().first().copied()
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reply::Value { reg, payload } => {
                write!(f, "reg 0x{reg:02x} = {}", Hex(payload))?;
                if let Some(text) = printable(payload) {
                    write!(f, " ({text:?})")?;
                }
                Ok(())
            }
            Reply::Ack { reg: 0x11 } => write!(f, "ack (write applied)"),
            Reply::Ack { reg } => write!(f, "ack (reg 0x{reg:02x}, no payload)"),
            Reply::Unsupported => write!(f, "unsupported register"),
            Reply::BadCrc {
                reg,
                payload,
                got,
                want,
            } => write!(
                f,
                "reg 0x{reg:02x} = {} BAD CRC (got 0x{got:02x}, want 0x{want:02x})",
                Hex(payload)
            ),
            Reply::Chunk(data) => write!(f, "chunk ({} bytes) {}", data.len(), Hex(data)),
        }
    }
}

/// Lowercase hex without separators.
pub struct Hex<'a>(pub &'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

fn printable(payload: &[u8]) -> Option<String> {
    if payload.is_empty() || payload.len() > 512 {
        return None;
    }
    if !payload.iter().all(|b| (0x20..0x7f).contains(b) || *b == 0) {
        return None;
    }
    Some(
        payload
            .iter()
            .filter(|b| **b != 0)
            .map(|b| *b as char)
            .collect(),
    )
}

/// Total byte length of a framed reply, given at least its first three bytes.
///
/// Returns `None` when the buffer does not yet hold three bytes, or when the
/// reply is headerless (a chunk) and therefore has no knowable length.
pub fn framed_len(head: &[u8]) -> Option<usize> {
    if head.len() < 3 {
        return None;
    }
    if head[0] != OP_RESP {
        return None; // headerless chunk
    }
    if head[2] == VAL_ERROR {
        // `52 <reg> ff`: terminal, no payload. Treating 0xff as a 127-byte
        // length is what makes writes look like they hang.
        return Some(3);
    }
    let payload_len = head[2].wrapping_sub(LEN_BIAS) as usize;
    Some(3 + payload_len + 1)
}

/// Decode a complete reply buffer.
pub fn decode(buf: &[u8]) -> Reply {
    if buf.len() >= 3 && buf[0] == OP_RESP {
        if buf.len() == 3 && buf[2] == VAL_ERROR {
            return if buf[1] == REG_ERROR {
                Reply::Unsupported
            } else {
                Reply::Ack { reg: buf[1] }
            };
        }
        let reg = buf[1];
        let payload_len = buf[2].wrapping_sub(LEN_BIAS) as usize;
        let body_end = 3 + payload_len;
        if buf.len() > body_end {
            let payload = buf[3..body_end].to_vec();
            let got = buf[body_end];
            let want = crc8(&buf[..body_end]);
            return if got == want {
                Reply::Value { reg, payload }
            } else {
                Reply::BadCrc {
                    reg,
                    payload,
                    got,
                    want,
                }
            };
        }
        // Framed header but truncated body: fall through to Chunk.
    }
    Reply::Chunk(buf.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Every well-framed reply captured from the real device.
    const CAPTURED: &[&str] = &[
        "5225812e3a",       // volume = 46
        "52308112ec",       // source bitfield
        "522781ffd5",       // dsp mode (unsupported on LSX)
        "52288186fa",       // desk dB
        "5229818691",       // wall dB
        "522a81ff44",       // treble (unsupported)
        "522b81ff2f",       // high-pass (unsupported)
        "522c81ff39",       // low-pass (unsupported)
        "522d81ff52",       // sub gain (unsupported)
        "5218818300",       // unknown
        "52198200043e",     // unknown, u16
        "5220834c535815",   // "LSX"
        "523181846c",       // transport state
    ];

    #[test]
    fn captured_frames_decode_with_valid_crc() {
        for h in CAPTURED {
            match decode(&hex(h)) {
                Reply::Value { .. } => {}
                other => panic!("{h} decoded as {other:?}"),
            }
        }
    }

    #[test]
    fn length_byte_is_len_plus_bias() {
        // 5220834c535815 -> len byte 0x83 => 3 payload bytes "LSX"
        let r = decode(&hex("5220834c535815"));
        assert_eq!(r.payload(), b"LSX");
        assert_eq!(r.byte(), Some(b'L'));
    }

    #[test]
    fn three_byte_terminal_replies() {
        assert_eq!(decode(&hex("5212ff")), Reply::Unsupported);
        assert_eq!(decode(&hex("5211ff")), Reply::Ack { reg: 0x11 });
        // Both must be treated as complete, not as a 127-byte payload.
        assert_eq!(framed_len(&hex("5212ff")), Some(3));
        assert_eq!(framed_len(&hex("5211ff")), Some(3));
    }

    #[test]
    fn set_ack_is_not_mistaken_for_a_long_frame() {
        // Regression: 0xff was read as length 127, so writes appeared to hang.
        let ack = hex("5211ff");
        assert_eq!(framed_len(&ack), Some(ack.len()));
    }

    #[test]
    fn framed_len_matches_full_capture() {
        for h in CAPTURED {
            let b = hex(h);
            assert_eq!(framed_len(&b), Some(b.len()), "{h}");
        }
    }

    #[test]
    fn headerless_chunk_is_not_framed() {
        let chunk = b"<bss_list count=\"13\">";
        assert_eq!(framed_len(chunk), None);
        assert!(matches!(decode(chunk), Reply::Chunk(_)));
    }

    #[test]
    fn bad_crc_is_reported_not_hidden() {
        let mut b = hex("5225812e3a");
        b[4] ^= 0xff;
        assert!(matches!(decode(&b), Reply::BadCrc { .. }));
    }

    #[test]
    fn crc8_known_answers() {
        assert_eq!(crc8(&[0x52, 0x25, 0x81, 0x2e]), 0x3a);
        assert_eq!(crc8(&[0x52, 0x30, 0x81, 0x12]), 0xec);
        assert_eq!(crc8(&[0x52, 0x20, 0x83, 0x4c, 0x53, 0x58]), 0x15);
    }

    #[test]
    fn request_framing() {
        // Matches the frames the known-working kefctl client sends.
        assert_eq!(request(OP_GET, 0x25, &[], false), hex("472580"));
        assert_eq!(request(OP_SET, 0x25, &[0x2e], false), hex("5325812e"));
        // Transport: 53 31 81 81 (play/pause)
        assert_eq!(request(OP_SET, 0x31, &[0x81], false), hex("53318181"));
        // Multi-byte payload keeps the length byte honest.
        assert_eq!(request(OP_SET, 0x30, &[0x12, 0x00], false), hex("5330821200"));
        let with = request(OP_SET, 0x25, &[0x2e], true);
        assert_eq!(with.len(), 5);
        assert_eq!(crc8(&with[..4]), with[4]);
    }
}
