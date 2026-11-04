# Register map

Observed on an LSX gen 1 (master), system firmware `2.7.0`.
`—` means the register answered `52 12 ff` (unsupported) or was not probed.

## Identity

| Reg | Name | Payload | Live value |
|---|---|---|---|
| `0x10` | module_version | XML | `<module>p20.5202205110.105243</module><system>2.7.0</system>` |
| `0x11` | module_system_dsp | XML | same as `0x10` plus `<DSP>1.0</DSP>` |
| `0x14` | hardware_version | XML + raw | `<Hardware>\x08.</Hardware>` (contains raw bytes) |
| `0x15` | serial | XML | `<Serial>KMLSX#############</Serial>` |
| `0x16` | serial_raw | XML | `<Serial>` + serial + `0xff`×16 + `</Serial>` |
| `0x17` | model | XML | `<Model>SP3994</Model>` |

## The map is state-dependent (wired vs wireless)

**Correction.** These registers change meaning with the active network interface.
Measured on the same unit before and after switching from Wi-Fi to Ethernet:

| Reg | On Wi-Fi | On Ethernet |
|---|---|---|
| `0x20` | `LSX` | `LSX 000000` |
| `0x21` | `kef-lsx-0000` (hostname) | `FALSE` |
| `0x22` | `WPA2PSK` | `NONE` |
| `0x23` | `AES` | `open` |
| `0x24` | *the WPA2 passphrase* | `open` |

So `0x20` is the **DMP / device name** (it matches the DRC title on port 10100),
and `0x21`-`0x24` are **Wi-Fi state**: enabled flag, security mode, cipher, and
passphrase. The passphrase is only readable while the unit is on Wi-Fi. That is
still a plaintext credential exposure with no authentication, but it is
interface-dependent, not unconditional.

`0x2f` (Wi-Fi scan blob) likewise returns `0xff` on Ethernet, since there is no
scan to report.

## Network

| Reg | Name | Payload | Live value |
|---|---|---|---|
| `0x20` | device_family | string | `LSX` |
| `0x21` | hostname | string | `kef-lsx-0000` |
| `0x22` | wifi_security | string | `WPA2PSK` |
| `0x23` | wifi_cipher | string | `AES` |
| `0x24` | wifi_psk | string | **WPA2 PSK in plaintext — redacted** |
| `0x2f` | wifi_scan | blob | `<bss_list count="13">…` headerless chunks |

Strings are NUL-terminated; the NUL is included in the length.

## Control

| Reg | Name | Payload | Live value |
|---|---|---|---|
| `0x25` | volume | u8 | `0x2e` = 46; bit 7 = mute |
| `0x26` | volume_related | u8 | `0x9e` = 158 — meaning unknown |
| `0x27` | dsp_mode | bitfield | `0xff` unsupported on LSX |
| `0x28` | desk_db | slider | `0x86` = −3.0 dB |
| `0x29` | wall_db | slider | `0x86` = −3.0 dB |
| `0x2a` | treble_db | slider | `0xff` unsupported |
| `0x2b` | high_pass_hz | slider | `0xff` unsupported |
| `0x2c` | sub_low_pass_hz | slider | `0xff` unsupported |
| `0x2d` | sub_gain_db | slider | `0xff` unsupported |
| `0x30` | source | bitfield | `0x12` = on, wifi, 60 min standby |
| `0x31` | transport | u8 | `0x84` — see below |

### Transport register `0x31`

Writable with a one-byte payload:

| Payload | Action |
|---|---|
| `0x81` | play / pause (toggle) |
| `0x82` | next track |
| `0x83` | previous track |

The read value (`0x84` observed) is a state code whose mapping is not yet known.

## Unsupported values are not a capability map

`0xff` means "not currently exposed", **not** "this model lacks the feature".
The DSP block shifts across power cycles — see the correction in
[`findings.md`](findings.md). Do not cache `0xff` as a capability, and do not
trust a single sweep.

## Full 256-register sweep (Ethernet)

A complete read-only sweep of `0x00`-`0xff` found 32 answering registers; every
other register returns `52 12 ff`. Newly observed:

| Reg | Value | Note |
|---|---|---|
| `0x2e` | `52 13 ff` | third terminal reply shape (`0x13`), command register |
| `0x3d` | `0x64` | unknown |
| `0x3e` | `0x00` | unknown |
| `0x40` | `0x81` | unknown |
| `0x41` | `0x82` | unknown |
| `0x42` | `0x82` | unknown |
| `0x43` | `0x80` | unknown |

Terminal reply register bytes seen so far: `0x11` (write ack), `0x12`
(unsupported), `0x13` (command ack).

**No register exposes the CnE store generically, and none exposes memory.**
The remote-register tunnel is a fixed small surface, not a generic read/write
primitive.

## Unsupported / unassigned

`0x00`–`0x0f`, `0x12`, `0x13`, `0x1a`–`0x1f`, `0x3d`–`0x3f` all answer the
constant `52 12 ff`.

## Aliases

| Reg | Answers as |
|---|---|
| `0x3a` | `0x2f` |
| `0x3b` | `0x30` |
| `0x3c` | `0x31` |

Unexplained. Not a simple constant offset from the target registers.

## Probing notes

* `0x2e` returned the *SET ack* shape (`52 11 ff`) in response to a GET, so it
  may be a command register rather than a data register.
* `0x19` returns a two-byte value `0x0004`.
* Register `0x26` sits next to volume and returns `0x9e`; possibly a
  balance/bass control, unverified.

Reproduce the whole map with:

```sh
keflsx sweep --from 0x00 --to 0x40 --jsonl captures/sweep.jsonl
```

Redact `0x15`, `0x16`, `0x21`–`0x24`, `0x2f` before sharing.
