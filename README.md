# kef-lsx

Unofficial control library and CLI for **first-generation KEF LSX** and
**LS50 Wireless** speakers, over the undocumented control protocol on TCP
50001.

The point of this project is that you do not have to jailbreak anything to get
usable control of these speakers. The control server is open, unauthenticated,
and now documented.

## Status

Working against a live LSX pair (firmware `2.7.0` / module
`p20.5202205110.105243`):

"Verified" means observed on real hardware, not just implemented.

| Capability | State |
|---|---|
| Read volume, mute state | verified |
| Set volume, mute, unmute, toggle | **verified** (ack + readback round-trip) |
| Read input source, standby, power, L/R invert | verified (read) |
| Read identity (model, serial, hostname, firmware) | verified |
| Read desk/wall dB | verified |
| Set input source / standby | implemented, acks, **not confirmed** |
| Power on/off, L/R invert | implemented, acks, **not confirmed** |
| Transport (play/pause, next, prev) | sends and acks, not audibly confirmed |
| Wi-Fi scan blob | partially — trigger semantics unknown |
| DSP mode / treble / sub controls | LSX reports unsupported (LS50W only) |

> **Footgun:** switching the input away from `wifi` moves the speaker off the
> network and drops your own control channel. You then need the remote or the
> app to get it back. This is why input/power writes are marked unconfirmed —
> they were not exercised on this unit.

Not attempted: firmware extraction, UART, root. See
[`docs/findings.md`](docs/findings.md) for the route map and open questions.

## Quick start

```sh
cargo build --release

# what is it doing right now
./target/release/keflsx -H 192.0.2.10 status

./target/release/keflsx -H 192.0.2.10 volume 30
./target/release/keflsx -H 192.0.2.10 input optical
./target/release/keflsx -H 192.0.2.10 mute
./target/release/keflsx -H 192.0.2.10 power off

# research
./target/release/keflsx sweep --from 0x00 --to 0x40 --jsonl captures/sweep.jsonl
./target/release/keflsx dump
./target/release/keflsx get 0x25
./target/release/keflsx get model
```

Set `KEF_HOST` instead of passing `-H` every time.

`status` output:

```
volume      46
power       on
input       wifi
standby     60 min
invert L/R  off
desk        -3.0 dB
wall        -3.0 dB
dsp mode    unsupported on this model
model       SP3994
serial      KMLSX#############
hostname    kef-lsx-0000
firmware    module=p20.5202205110.105243 system=2.7.0 dsp=1.0
```

## Library

```rust
use kef_lsx::{Client, state::Source};
use std::time::Duration;

let mut kef = Client::connect("192.0.2.10", Duration::from_secs(3))?;

// Hold one Client for the process lifetime — see docs/protocol.md.
let vol = kef.get(0x25)?.byte().unwrap_or(0) & 0x7f;

if let Some(byte) = kef.get(0x30)?.byte() {
    let src = Source::decode(byte);
    println!("volume {vol}, input {}", src.input);
}

kef.set(0x25, &[30], false)?;
```

Typed helpers live in [`state`](src/state.rs): `Source`, `Input`, `Standby`,
`DspMode`, `Firmware`, plus the slider tables.

## Safety

Reads are harmless. Writes are real: they change the speaker, and a few
combinations are known to wedge the control server (see
[`docs/protocol.md`](docs/protocol.md#known-firmware-bug)). `keflsx` works around
the known bug and verifies writes by reading back.

`set` and raw `0x53` frames require `--yes`.

**Do not flash firmware** until you have a verified dump and a recovery route.
There is no documented recovery for a bad LSX flash; the failure mode is a dead
speaker.

## Docs

| File | Contents |
|---|---|
| [`docs/protocol.md`](docs/protocol.md) | full wire format, checksum, framing, value encodings |
| [`docs/register-map.md`](docs/register-map.md) | every known register with live values |
| [`docs/findings.md`](docs/findings.md) | dated lab notes, attack surface, open questions |
| [`docs/firmware.md`](docs/firmware.md) | the OTA image: provenance, container layout, what's inside, verification status |
| [`docs/firmware-map.md`](docs/firmware-map.md) | **reference map**: sections, subsystems, web API, config store, update path, where to poke |
| [`docs/uart-plan.md`](docs/uart-plan.md) | physical teardown and UART plan, **with a photographed step by step** of the disassembly |

## Tools

| Tool | Purpose |
|---|---|
| `tools/xref.py` | disassemble firmware sections as Thumb-2, infer the load base, resolve string cross-references |

## Prior art

* [`kraih/kefctl`](https://github.com/kraih/kefctl) — the original Perl client,
  reverse-engineered from the KEF Control app. Source of the bit layouts.
* [`basnijholt/aiokef`](https://github.com/basnijholt/aiokef),
  [`Raphiiko/hass-lsx-i`](https://github.com/Raphiiko/hass-lsx-i) — Home
  Assistant integrations for the same protocol.

The checksum, the length-byte framing, the ACK semantics, and the identity /
network register block are documented here for the first time as far as I can
tell.

## Licence

MIT OR Apache-2.0. Not affiliated with KEF.
