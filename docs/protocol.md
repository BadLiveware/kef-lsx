# Control protocol — TCP 50001

Reverse-engineered from a live LSX pair (system firmware `2.7.0`, module
`p20.5202205110.105243`) and cross-checked against the publicly known client
[`kraih/kefctl`](https://github.com/kraih/kefctl), which was derived by sniffing
the KEF Control app.

No authentication, no session, no rate limiting, no documented spec.

## Connecting

```
TCP, port 50001, master speaker only
```

**Hold one connection open.** The server is effectively single-session. Opening
a connection, sending one command, closing, then immediately reconnecting yields
`ECONNREFUSED` — and worse, can return a *stale reply belonging to the previous
request*, which silently corrupts read-modify-write logic. Reuse one socket for
the whole conversation.

There is no handshake and no greeting. The server speaks only when spoken to.

## Request

```
byte 0 : 0x47 'G'  get
         0x53 'S'  set
byte 1 : register
byte 2 : 0x80 + payload_len
byte 3+: payload (SET only)
```

Byte 2 is a **length byte**, not a read/write flag. A GET carries no payload so
it is `0x80`; a one-byte SET is `0x81`. This looks like a flag until you notice
a two-byte SET uses `0x82`.

```
GET volume      47 25 80
SET volume = 46 53 25 81 2e
PLAY/PAUSE      53 31 81 81
```

Requests are accepted **without** a trailing checksum — that is what the
known-working client sends. Appending a CRC8 also appears to be accepted, but
no behavioural difference has been observed, so it is not sent by default.

## Response

```
byte 0 : 0x52 'R'
byte 1 : register being answered
byte 2 : 0x80 + payload_len
byte 3+: payload
last   : CRC8 over bytes 0..last
```

CRC8: polynomial `0x07`, initial value `0x00`, no input/output reflection, no
final XOR. This is the classic CRC-8/SMBUS/"CRC-8" variant.

```
52 25 81 2e 3a   -> volume = 0x2e = 46
52 30 81 12 ec   -> source bitfield = 0x12
52 20 83 4c 53 58 15 -> "LSX"
```

### Three-byte terminal replies

A 3-byte reply ending in `0xff` carries **no payload and no CRC**. The `0xff`
means "no data", *not* length 127 — misreading it as a length makes every write
appear to hang for the full timeout.

| Bytes | Meaning |
|---|---|
| `52 11 ff` | SET acknowledged; the write was applied |
| `52 12 ff` | register not supported for this operation |

`52 12 ff` is a **constant**. It does not echo the register you asked for, so
any code that trusts `byte 1` will mis-attribute the error to register `0x12`.

### Headerless chunked reads

Large payloads — the Wi-Fi scan list behind reg `0x2f` — arrive as raw
256-byte chunks with **no header and no CRC**. There is no documented
terminator. Read until the socket goes idle.

Observed once: ~1.9 KB of XML across several chunks, starting
`<bss_list count="13">`. A later standalone read of `0x2f` returned the normal
5-byte framed form instead, so the blob appears to be served only when a scan is
pending. Trigger semantics unknown.

## Writes

A SET is acknowledged with `52 11 ff` and **the new value is not echoed**. Always
read the register back to confirm.

```
GET 0x25 -> 30        volume 48
SET 0x25 <- 31        ack
GET 0x25 -> 31        volume 49   applied
```

## Value encodings

### Sliders (regs `0x28`–`0x2d`)

`value = table_index + 128`. `0xff` means the control does not exist on this
model.

| Register | Table | Range | Step |
|---|---|---|---|
| `0x28` desk | `DESK_WALL_DB` | −6.0 … 0 dB | 0.5 |
| `0x29` wall | `DESK_WALL_DB` | −6.0 … 0 dB | 0.5 |
| `0x2a` treble | `TREBLE_DB` | −2.0 … +2.0 dB | 0.5 |
| `0x2b` high-pass | `HIGH_HZ` | 50 … 120 Hz | 5 |
| `0x2c` sub low-pass | `LOW_HZ` | 40 … 250 Hz | 5 |
| `0x2d` sub gain | `SUB_DB` | −10 … +10 dB | 1.0 |

So `0x86` = index 6 = **−3.0 dB**, not 0 dB. Index 12 is 0 dB.

### Volume (reg `0x25`)

Bits 0–6 are the level 0–100; **bit 7 (`0x80`) is the mute flag**. So `0x2e` is
46, `0xae` is 46 *muted*. Muting does not zero the level, which is why mute can
be toggled without losing the volume.

### Source (reg `0x30`)

```
0x80  power     1 = off
0x40  invert L/R
0x30  standby   00 = 20 min, 01 = 60 min, 10 = never, 11 = unknown
0x0f  input     2 = wifi, c = usb, 9 = bluetooth (paired),
                f = bluetooth (unpaired), a = aux, b = optical
```

Selecting an input also powers the speaker on; the app behaves the same way.
Preserve unrecognised standby bits on read-modify-write rather than normalising
them.

### DSP mode (reg `0x27`, LS50 Wireless only)

```
0x80  desk mode      0x40  wall mode
0x20  phase correction
0x10  high-pass
0x0c  sub extension  00 = standard, 10 = less, 01 = extra
0x02  sub polarity   1 = '-', 0 = '+'
```

The LSX reports `0xff` here (unsupported) but still answers `0x28`/`0x29`.

## Known firmware bug

Powering off while auto-standby is set to **20 minutes** wedges the control
server. Work around it by moving standby to 60 minutes first — that is what
`keflsx power off` does. A server that wedges on a legal command sequence is a
poorly hardened process, which is worth remembering if you go looking for memory
corruption.
