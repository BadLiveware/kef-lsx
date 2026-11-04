//! Interactive probe: exercises the control server on a **single** connection.
//!
//! Short-lived connections to port 50001 are unreliable because the server is
//! effectively single-session, so anything that needs more than one command
//! must reuse one socket. Run with:
//!
//! ```sh
//! cargo run --example probe -- 192.0.2.10
//! ```
//!
//! Pass `--write` to include the volume round-trip, which does change the
//! speaker (46 -> 47 -> 46). Everything else is read-only.

use std::time::Duration;

use anyhow::Result;
use kef_lsx::state::{self, Source};
use kef_lsx::{regs, Client, Reply};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "192.0.2.10".into());
    let write = args.any(|a| a == "--write");

    let mut kef = Client::connect(&host, Duration::from_secs(3))?;
    println!("connected to {}\n", kef.addr());

    println!("== identity ==");
    for reg in regs::REGS.iter().filter(|r| r.addr <= 0x24) {
        let reply = kef.get(reg.addr)?;
        let text = state::as_str(&reply).unwrap_or_default();
        println!(
            "  0x{:02x} {:<18} {}",
            reg.addr,
            reg.name,
            if text.is_empty() {
                format!("{reply}")
            } else {
                format!("{reply}  {text:?}")
            }
        );
    }

    println!("\n== state ==");
    let vol = kef.get(0x25)?;
    let src = kef.get(0x30)?;
    println!("  volume  {}", vol);
    println!("  source  {}", src);
    if let Some(byte) = src.byte() {
        let s = Source::decode(byte);
        println!(
            "          power={} input={} standby={} invert={}",
            if s.off { "off" } else { "on" },
            s.input,
            s.standby,
            s.invert
        );
    }

    if write {
        let before = vol.byte().expect("volume byte");
        let level = before & 0x7f;
        println!("\n== write probe (single connection) ==");
        println!("  volume starts at {level} (raw 0x{before:02x})");

        for (label, target) in [("+1", level + 1), ("back", level)] {
            let reply = kef.set(0x25, &[target], false)?;
            println!("  SET volume -> {target:<3} reply: {reply}");
            let read = kef.get(0x25)?;
            let now = read.byte().unwrap_or(0) & 0x7f;
            println!(
                "      readback {now:<3} {}",
                if now == target {
                    "WRITE APPLIED"
                } else {
                    "WRITE IGNORED"
                }
            );
            let _ = label;
        }

        // Try a CRC-tagged write too, in case framing is the issue.
        let reply = kef.set(0x25, &[level], true)?;
        println!("  SET volume (with CRC) reply: {reply}");
    }

    // Leave the connection open until the end so the server releases it once.
    drop(kef);
    println!("\ndone");
    Ok(())
}

#[allow(dead_code)]
fn kind_name(reply: &Reply) -> &'static str {
    match reply {
        Reply::Value { .. } => "value",
        Reply::Ack { .. } => "ack",
        Reply::Unsupported => "unsupported",
        Reply::BadCrc { .. } => "bad-crc",
        Reply::Chunk(_) => "chunk",
    }
}
