//! `keflsx` — CLI for first-generation KEF LSX / LS50 Wireless speakers.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

use kef_lsx::proto::Hex;
use kef_lsx::state::{self, Input, Source, Standby};
use kef_lsx::{regs, Client, Reply};

#[derive(Parser)]
#[command(name = "keflsx", version, about = "Control first-gen KEF LSX / LS50 Wireless over TCP 50001")]
struct Cli {
    /// Master speaker address.
    #[arg(short = 'H', long, env = "KEF_HOST", default_value = "192.0.2.10")]
    host: String,

    /// Socket timeout in seconds.
    #[arg(long, default_value_t = 3)]
    timeout: u64,

    /// Machine-readable output.
    #[arg(long)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show volume, source, standby, DSP and identity.
    Status,

    /// Read one register (name or hex address).
    Get { reg: String },

    /// Write raw bytes to a register. Requires --yes.
    Set {
        reg: String,
        /// Payload as hex, e.g. 2e for volume 46.
        hex: String,
        /// Confirm the write.
        #[arg(long)]
        yes: bool,
        /// Append a CRC8 to the request (unverified against hardware).
        #[arg(long)]
        crc: bool,
    },

    /// Set absolute volume, 0-100.
    Volume { percent: u8 },

    /// Mute (adds the 0x80 offset the firmware uses).
    Mute,

    /// Unmute.
    Unmute,

    /// Toggle mute.
    Toggle,

    /// Select an input: wifi, usb, bluetooth, aux, optical.
    Input {
        source: String,
        /// Auto-standby: 20, 60 or never.
        #[arg(long)]
        standby: Option<String>,
    },

    /// Power the speaker on or off.
    Power { state: String },

    /// Send a raw request frame as hex. Requires --yes if it is a write.
    Raw {
        hex: String,
        #[arg(long)]
        yes: bool,
    },

    /// Probe a register range, classifying replies. Read-only.
    Sweep {
        #[arg(long, default_value = "0x00")]
        from: String,
        #[arg(long, default_value = "0x40")]
        to: String,
        /// Append one JSON object per register to this file.
        #[arg(long)]
        jsonl: Option<PathBuf>,
    },

    /// Dump identity + current state as JSON.
    Dump {
        #[arg(long)]
        jsonl: Option<PathBuf>,
    },

    /// Read the Wi-Fi scan blob (headerless chunked read).
    Scan,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let timeout = Duration::from_secs(cli.timeout);
    let mut kef = Client::connect(&cli.host, timeout)
        .with_context(|| format!("speaker at {}", cli.host))?;

    match cli.cmd {
        Cmd::Status => status(&mut kef, cli.json),
        Cmd::Get { reg } => {
            let addr = regs::parse(&reg)?;
            let reply = kef.get(addr)?;
            emit(cli.json, addr, &reply);
            Ok(())
        }
        Cmd::Set {
            reg,
            hex,
            yes,
            crc,
        } => {
            let addr = regs::parse(&reg)?;
            if !yes {
                bail!("refusing to write without --yes (register 0x{addr:02x})");
            }
            let payload = parse_hex(&hex)?;
            let reply = kef.set(addr, &payload, crc)?;
            emit(cli.json, addr, &reply);
            Ok(())
        }
        Cmd::Volume { percent } => set_volume(&mut kef, percent),
        Cmd::Mute => set_muted(&mut kef, true),
        Cmd::Unmute => set_muted(&mut kef, false),
        Cmd::Toggle => {
            let muted = is_muted(&mut kef)?;
            set_muted(&mut kef, !muted)
        }
        Cmd::Input { source, standby } => {
            let input = Input::parse(&source)
                .with_context(|| format!("unknown input {source:?}"))?;
            set_input(&mut kef, input, standby.as_deref())
        }
        Cmd::Power { state } => set_power(&mut kef, &state),
        Cmd::Raw { hex, yes } => {
            let req = parse_hex(&hex)?;
            if req.first() == Some(&0x53) && !yes {
                bail!("refusing to send a write frame (0x53) without --yes");
            }
            let reply = kef.raw(&req)?;
            println!("{reply}");
            Ok(())
        }
        Cmd::Sweep { from, to, jsonl } => sweep(&mut kef, &from, &to, jsonl),
        Cmd::Dump { jsonl } => dump(&mut kef, jsonl),
        Cmd::Scan => scan(&mut kef),
    }
}

fn emit(json_out: bool, reg: u8, reply: &Reply) {
    if json_out {
        println!(
            "{}",
            json!({
                "reg": format!("0x{reg:02x}"),
                "name": regs::by_addr(reg).map(|r| r.name),
                "payload": Hex(reply.payload()).to_string(),
                "reply": format!("{reply}"),
            })
        );
    } else {
        let name = regs::by_addr(reg).map(|r| r.name).unwrap_or("?");
        println!("{name:<18} 0x{reg:02x} : {reply}");
    }
}

fn status(kef: &mut Client, json_out: bool) -> Result<()> {
    let vol_raw = byte(kef.get(0x25)?, "volume")?;
    let muted = vol_raw >= state::SLIDER_BIAS;
    let volume = vol_raw & 0x7f;

    let source = Source::decode(byte(kef.get(0x30)?, "source")?);

    let dsp_mode = state::DspMode(byte(kef.get(0x27)?, "dsp_mode")?);
    let desk = state::slider(byte(kef.get(0x28)?, "desk_db")?, &state::DESK_WALL_DB);
    let wall = state::slider(byte(kef.get(0x29)?, "wall_db")?, &state::DESK_WALL_DB);

    let fw_xml = str_payload(kef.get(0x11)?);
    let fw = state::Firmware::parse(&fw_xml);
    let serial = state::tag(&str_payload(kef.get(0x15)?), "Serial");
    let model = state::tag(&str_payload(kef.get(0x17)?), "Model");
    let hostname = state::as_str(&kef.get(0x21)?);

    if json_out {
        println!(
            "{}",
            json!({
                "volume": volume,
                "muted": muted,
                "power": if source.off { "off" } else { "on" },
                "input": source.input.to_string(),
                "standby": source.standby.to_string(),
                "invert_lr": source.invert,
                "desk_db": desk,
                "wall_db": wall,
                "dsp_mode_supported": !dsp_mode.is_unsupported(),
                "firmware": {
                    "module": fw.module,
                    "system": fw.system,
                    "dsp": fw.dsp,
                },
                "serial": serial,
                "model": model,
                "hostname": hostname,
            })
        );
        return Ok(());
    }

    println!("volume      {volume}{}", if muted { " (muted)" } else { "" });
    println!("power       {}", if source.off { "off" } else { "on" });
    println!("input       {}", source.input);
    println!("standby     {}", source.standby);
    println!("invert L/R  {}", if source.invert { "on" } else { "off" });
    println!("desk        {desk} dB");
    println!("wall        {wall} dB");
    if dsp_mode.is_unsupported() {
        println!("dsp mode    unsupported on this model");
    } else {
        println!(
            "dsp mode    desk={} wall={} phase={} highpass={} ext={} polarity={}",
            dsp_mode.desk(),
            dsp_mode.wall(),
            dsp_mode.phase_correction(),
            dsp_mode.high_pass(),
            dsp_mode.sub_extension(),
            dsp_mode.sub_polarity()
        );
    }
    if let Some(m) = &model {
        println!("model       {m}");
    }
    if let Some(s) = &serial {
        println!("serial      {s}");
    }
    if let Some(h) = &hostname {
        println!("hostname    {h}");
    }
    println!(
        "firmware    module={} system={} dsp={}",
        fw.module.as_deref().unwrap_or("?"),
        fw.system.as_deref().unwrap_or("?"),
        fw.dsp.as_deref().unwrap_or("?")
    );
    Ok(())
}

fn set_volume(kef: &mut Client, percent: u8) -> Result<()> {
    if percent > 100 {
        bail!("volume must be 0-100, got {percent}");
    }
    let muted = is_muted(kef)?;
    let byte = percent | if muted { state::SLIDER_BIAS } else { 0 };
    kef.set(0x25, &[byte], false)?;
    println!("volume {percent}{}", if muted { " (still muted)" } else { "" });
    Ok(())
}

fn is_muted(kef: &mut Client) -> Result<bool> {
    Ok(byte(kef.get(0x25)?, "volume")? >= state::SLIDER_BIAS)
}

fn set_muted(kef: &mut Client, mute: bool) -> Result<()> {
    let raw = byte(kef.get(0x25)?, "volume")?;
    let level = raw & 0x7f;
    let byte = if mute { level | state::SLIDER_BIAS } else { level };
    kef.set(0x25, &[byte], false)?;
    println!("{}", if mute { "muted" } else { "unmuted" });
    Ok(())
}

fn set_input(kef: &mut Client, input: Input, standby: Option<&str>) -> Result<()> {
    let mut source = Source::decode(byte(kef.get(0x30)?, "source")?).with_input(input);
    if let Some(s) = standby {
        source.standby = match s.to_ascii_lowercase().as_str() {
            "20" => Standby::TwentyMinutes,
            "60" => Standby::SixtyMinutes,
            "never" | "0" | "off" => Standby::Never,
            other => bail!("standby must be 20, 60 or never, got {other:?}"),
        };
    }
    kef.set(0x30, &[source.encode()], false)?;
    println!("input {input}, standby {}", source.standby);
    Ok(())
}

fn set_power(kef: &mut Client, want: &str) -> Result<()> {
    let off = match want.to_ascii_lowercase().as_str() {
        "on" => false,
        "off" | "standby" => true,
        other => bail!("power takes on or off, got {other:?}"),
    };

    let mut source = Source::decode(byte(kef.get(0x30)?, "source")?);

    // The firmware has a known bug: powering off while auto-standby is set to
    // 20 minutes wedges the control server. Move to 60 first. See docs.
    if off && source.standby == Standby::TwentyMinutes {
        source.standby = Standby::SixtyMinutes;
        kef.set(0x30, &[source.encode()], false)?;
    }

    source.off = off;
    kef.set(0x30, &[source.encode()], false)?;
    println!("power {}", if off { "off" } else { "on" });
    Ok(())
}

fn sweep(kef: &mut Client, from: &str, to: &str, jsonl: Option<PathBuf>) -> Result<()> {
    let lo = regs::parse(from)?;
    let hi = regs::parse(to)?;
    let mut capture = Capture::open(jsonl.as_ref())?;

    println!("{:>4}  {:<20} {:<10} reply", "reg", "name", "kind");
    for addr in lo..=hi {
        let reply = kef.get(addr)?;
        let name = regs::by_addr(addr).map(|r| r.name).unwrap_or("");
        let kind = match reply {
            Reply::Ack { .. } => "ack",
            Reply::Unsupported => "unsupported",
            Reply::Value { .. } => "value",
            Reply::BadCrc { .. } => "BAD CRC",
            Reply::Chunk(_) => "chunk",
        };
        println!(
            "0x{addr:02x}  {name:<20} {kind:<10} {}",
            truncate(&format!("{reply}"), 90)
        );
        capture.write(&json!({
            "reg": format!("0x{addr:02x}"),
            "name": name,
            "payload": Hex(reply.payload()).to_string(),
            "reply": format!("{reply}"),
            "kind": kind,
        }))?;
    }
    Ok(())
}

fn dump(kef: &mut Client, jsonl: Option<PathBuf>) -> Result<()> {
    let mut out = serde_json::Map::new();
    let mut capture = Capture::open(jsonl.as_ref())?;

    for reg in regs::REGS {
        let reply = kef.get(reg.addr)?;
        let entry = json!({
            "name": reg.name,
            "payload": Hex(reply.payload()).to_string(),
            "text": state::as_str(&reply),
        });
        capture.write(&json!({
            "reg": format!("0x{:02x}", reg.addr),
            "name": reg.name,
            "payload": Hex(reply.payload()).to_string(),
            "reply": format!("{reply}"),
        }))?;
        out.insert(format!("0x{:02x}", reg.addr), entry);
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn scan(kef: &mut Client) -> Result<()> {
    let blob = kef.read_blob(0x2f, 64 * 1024)?;
    if blob.is_empty() {
        bail!("no scan data returned (the app may need to trigger a scan first)");
    }
    let text = String::from_utf8_lossy(&blob);
    println!("{text}");
    Ok(())
}

fn byte(reply: Reply, what: &str) -> Result<u8> {
    reply
        .byte()
        .with_context(|| format!("no value returned for {what}: {reply}"))
}

fn str_payload(reply: Reply) -> String {
    String::from_utf8_lossy(reply.payload()).into_owned()
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !t.len().is_multiple_of(2) {
        bail!("hex must have an even number of digits: {s:?}");
    }
    (0..t.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&t[i..i + 2], 16)
                .with_context(|| format!("bad hex byte {:?}", &t[i..i + 2]))
        })
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Optional JSONL capture sink, used to accumulate research data.
struct Capture(Option<std::fs::File>);

impl Capture {
    fn open(path: Option<&PathBuf>) -> Result<Self> {
        match path {
            None => Ok(Capture(None)),
            Some(p) => {
                let f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .with_context(|| format!("open capture file {}", p.display()))?;
                Ok(Capture(Some(f)))
            }
        }
    }

    fn write(&mut self, value: &serde_json::Value) -> Result<()> {
        use std::io::Write;
        if let Some(f) = self.0.as_mut() {
            writeln!(f, "{value}")?;
        }
        Ok(())
    }
}
