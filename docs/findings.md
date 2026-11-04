# Findings log

Knowledge accumulated against the real hardware, newest last. Redactions are
marked; see [Handling captured data](#handling-captured-data).

Test unit: KEF LSX (gen 1) pair, master at `192.0.2.10`, MAC
`aa:bb:cc:dd:ee:ff`.

---

## 2026-11-03 — first contact

### Identity

| Field | Value | Source |
|---|---|---|
| Model | `SP3994` | reg `0x17` |
| Device family | `LSX` | reg `0x20` |
| Serial | `KMLSX#############` | reg `0x15` |
| Hostname | `kef-lsx-0000` | reg `0x21` |
| Module firmware | `p20.5202205110.105243` | reg `0x10`/`0x11` |
| System firmware | `2.7.0` | reg `0x10`/`0x11` |
| DSP firmware | `1.0` | reg `0x11` |

Three separately versioned images (module / system / DSP) confirms the box is
not a single firmware blob — see [Architecture](#architecture-notes).

### Open ports

| Port | Service | Notes |
|---|---|---|
| 50001 | control protocol | **no authentication** |
| 80 | GoAhead-Webs | unauthenticated web UI, `index.asp` |
| 443 | TLS | not yet inspected |
| 8080 | UPnP event endpoint | `Allow: SUBSCRIBE, UNSUBSCRIBE` (GENA) |
| 1900 | SSDP | multicast; unicast `M-SEARCH` returned nothing |
| 554 | — | closed (no RTSP, so not a plain AirPlay 1 renderer) |
| 4000, 9100 | — | closed |

### The web UI is real and unauthenticated

```
GET /            -> 302 to /index.asp
GET /index.asp   -> 200, ~26 KB
GET /goform/     -> 200 "Form  is not defined"   <-- handler exists
GET /airplayRelJavScr.js -> 200, 107 KB
GET /css/style.css       -> 200, 20 KB
```

`index.asp` contains a firmware-update confirm flow (*"The update process cannot
be interrupted"*), an `isNetworkBridge` flag, and an `isScanEnabled` flag. A
working `/goform/` handler means the firmware-update surface is reachable over
HTTP — this is the most promising route to obtaining a firmware image without
opening the speaker. **Not yet explored**; do not brute-force it on a speaker
you care about.

The `Date:` header is always `Sat, 01 Jan 2000 00:00:00 GMT` — there is no RTC,
so timestamps in any logs the device produces are meaningless.

### Protocol: framing solved

See [`protocol.md`](protocol.md) for the full spec. Headlines:

* Byte 2 is a **length byte** (`0x80 + payload_len`) in both directions, not a
  read/write flag. The public `kefctl` client hardcodes `0x80` for GET and
  `0x81` for SET, which is why this went unnoticed.
* Responses carry a **CRC8, poly `0x07`, init `0x00`** over header+payload.
* A 3-byte `52 <reg> ff` is a **terminal no-payload reply**, not a 127-byte
  frame. `52 11 ff` = SET acknowledged; `52 12 ff` = unsupported.
* Large reads (the Wi-Fi scan blob) are **headerless 256-byte chunks** with no
  framing and no CRC.

### Writes work

Verified on a single connection:

```
GET 0x25 -> 30  (volume 48)
SET 0x25 <- 31  -> ack (write applied)
GET 0x25 -> 31  (volume 49)     <-- applied
SET 0x25 <- 30  -> ack
GET 0x25 -> 30  (volume 48)     <-- restored
```

A CRC appended to a SET is accepted (or ignored); it makes no observable
difference. The speaker **does not echo the new value**, so always read back.

### Unconfirmed writes

Only volume has been round-tripped end to end. `0x30` writes (input, power,
invert, standby) return the SET ack but were deliberately not exercised:
switching input away from `wifi` takes the speaker off the network and destroys
the control channel, requiring the remote or app to recover. Treat those as
"framing is right, effect is unverified".

### Traps encountered (worth remembering)

1. **Short-lived connections are unreliable.** Connect → one command → close →
   reconnect gets `ECONNREFUSED`, or worse, a *stale reply from the previous
   request*. This produced several false conclusions before it was spotted.
   Hold one socket; that is what `Client` does.
2. **`0xff` as a length byte means "no payload".** Reading it as length 127
   makes every write look like it hangs.
3. **`52 12 ff` does not carry the register you asked for** — it is a constant
   reply, so any code that trusts `buf[1]` will mis-attribute errors to
   register `0x12`.

### Security observations

Nothing here is novel for consumer audio gear, but it is worth writing down:

* **Reg `0x24` returns the WPA2 pre-shared key in plaintext**, with no
  authentication. Reg `0x22`/`0x23` give the security mode and cipher.
* The serial number, model, hostname and firmware versions are all readable
  without auth on 50001, and again from the unauthenticated web UI.
* No session, no token, no rate limit on 50001.
* KEF locked this down on later models: LSX II firmware 4.0 password-protected
  the web UI. The gen-1 openness is a historical accident.

### Architecture notes

Three firmware components are versioned independently, which matches the
hardware split:

1. **Network/streaming module** — answers 50001 and port 80. The `p20.` module
   version string and the GoAhead web server are both consistent with a
   StreamSDK-style vendor stack, but **the exact SoC is unconfirmed** and no
   teardown of an LSX gen 1 has been published.
2. **MCU** — inputs, IR, standby, LED, master/slave link.
3. **DSP + amp** — crossover, EQ, DAC, Class-D.

Root on (1) does not imply control of (3). Plan around that.

### Model-specific capability map

The LSX answers the LS50 Wireless DSP registers but reports `0xff`
("unsupported") for the subwoofer-related ones. That is consistent with the LSX
having no sub out.

| Register | LS50W | LSX |
|---|---|---|
| `0x27` DSP mode | supported | **`0xff` unsupported** |
| `0x28` desk dB | supported | supported (`0x86` = −3.0 dB) |
| `0x29` wall dB | supported | supported (`0x86` = −3.0 dB) |
| `0x2a` treble | supported | `0xff` unsupported |
| `0x2b` high-pass | supported | `0xff` unsupported |
| `0x2c` sub low-pass | supported | `0xff` unsupported |
| `0x2d` sub gain | supported | `0xff` unsupported |

Odd asymmetry: `0x27` is unsupported but `0x28`/`0x29` are not. Desk/wall dB can
be read even though the mode switch cannot. Unexplained.

---

## Open questions

Ordered by how much they would unlock.

1. **Firmware acquisition.** Does `/goform/` expose the update handler? Can the
   KEF Control app's OTA URL be captured with mitmproxy? Is the image signed?
   This is the gate on everything at level 1.
2. **Wi-Fi scan blob semantics (reg `0x2f`).** One sweep returned ~1.9 KB of
   `<bss_list>` XML across headerless chunks; a later standalone read returned a
   5-byte `52 2f 81 83 f7` instead. The blob is real — SSIDs, channels, RSSI,
   security — but the trigger and terminator are unknown.
3. **The `0x3a`/`0x3b`/`0x3c` aliases.** These answer as `0x2f`/`0x30`/`0x31`
   respectively. Deliberate alias block, register-space wrap, or something else?
4. **Reg `0x14` hardware version.** Payload is `<Hardware>\x08.</Hardware>` —
   contains a raw `0x08` and `0x2e` byte. Probably two packed version nibbles
   rendered as binary by a sloppy formatter.
5. **Reg `0x16` serial_raw.** Same serial padded with `0xff` — looks like a
   fixed-width field in uninitialised flash.
6. **Reg `0x19`.** Two-byte value `0x0004`. Unknown.
7. **Reg `0x26`.** `0x9e` (158). Sits next to volume. Balance? Bass?
8. **Reg `0x2e`.** Returned `52 11 ff` in the sweep — i.e. the *SET ack* shape in
   response to a GET. Possibly a command register rather than a data register.
9. **Does the speaker validate the SET checksum at all?** CRC and non-CRC writes
   behave identically so far.
10. **Slave speaker.** Does the slave listen on 50001 too, or only the master?
    All testing so far hit the master only.

---

## Handling captured data

`--jsonl` captures and any `dump` output **contain the WPA2 PSK** (reg `0x24`)
and the serial number. Treat captures as secrets: they are gitignored by
default, and should be redacted before being shared in an issue or writeup.

Registers worth redacting before publishing a capture: `0x15`, `0x16`, `0x21`,
`0x22`, `0x23`, `0x24`, `0x2f`.

---

## 2026-11-03 — web UI internals, and what standby looks like

### The web UI speaks its own delimited protocol

Handlers under `/goform/` are **unauthenticated** and answer with a
`qwhgpstgriz`-separated field list terminated by `EndRes`. First field is a
response type (`tor`): `4` = internet update, `5` = USB update, `9` = connection
status.

```
GET /goform/aformHandlerObtainConnStatus
9qwhgpstgriz2qwhgpstgriz1qwhgpstgriz1qwhgpstgrizexcellentzirgtspghwq0x00000zirgtspghwq192.0.2.10qwhgpstgrizEndRes
```

That is: tor=9, then `1`, `1`, signal `excellent`, error code `0x00000`, IP.
A second separator `zirgtspghwq` appears inside individual fields.

Handler inventory recovered from `airplayRelJavScr.js`:

| Handler | Purpose |
|---|---|
| `aformHandlerObtainConnStatus` | network status, signal quality, IP |
| `aformHandlerObtainUsbBcdList` | firmware packages found on a USB stick |
| `aformHandlerObtainUsbFwUpStatus` | USB update state |
| `aformHandlerObtainInternetFwUpStatus` | internet update check (`updateStatus=0/1`) |
| `aformNetFwHandler` | **confirm and start the update** |
| `aformHandlerSetNetFwUpdate` | select update method |
| `aformFwUpdateProgessHandler` | update progress polling |
| `aformHandlerConfigureSSIDList` | Wi-Fi config |
| `aformHandlerConfigureAirplayPassword` | AirPlay password |
| `aformHandlerConfigureStandbyMode` | auto-standby |
| `aformHandlerRestartNetworkSettings` | network restart |
| `aformHandlerSpotify*` | Spotify account handling |

Update states from the JS: `Idle → started → downloading →
downloadComplete → Verifying image → Download Update flash → done / failed`.

### Firmware update is USB or internet-download, not browser upload

The device either reads a firmware package off a USB stick or downloads one
from KEF. There is no upload form. So the acquisition routes are:

1. capture the OTA URL (DNS/network observation while a check runs), or
2. obtain a USB update package, or
3. UART.

### KEF publishes no firmware binaries

`assets.kef.com/pm/pm_firmware/release_notes.html` contains release notes only —
zero links to images, only anchors and a jQuery CDN reference. The gen-1 LSX has
no public download. (Later models' updates also come via the KEF Connect app.)

### Standby looks like this

After the auto-standby timer (60 min) fired, the device reported:

| | On | Standby |
|---|---|---|
| ping | yes | yes |
| port 80 `/` | 302 → `/index.asp` | **302 (still answers)** |
| port 80 `/index.asp` | 200 | **404** |
| `/goform/*` | data | "Form ... is not defined" |
| 50001 | open | **closed** |
| 1900 / 8080 / 443 / 7000 | open | **closed** |

So: **in standby the web server survives in a minimal form while the streaming,
control and UPnP subsystems shut down.** An earlier reading of this as "I
crashed the web server with traversal probes" was wrong — it was the standby
timer. Do not confuse the two; the tell is that `/` still answers 302.

Practical consequence: **set auto-standby to `never` before long research
sessions** (`keflsx input wifi --standby never`).

### Path traversal: none

`../etc/passwd`, `..%2f..%2f`, `%2e%2e/`, `....//`, `/goform/../../` — all 404,
or connection-reset by GoAhead. No file-read primitive found.

---

## 2026-11-03 — OTA check answers, but never completes

With the speaker awake, `aformHandlerObtainInternetFwUpStatus` works again:

```
POST updateStatus=0  ->  4<sep>0<sep>0zirgtspghwq$$$$$$$$$$$$<sep>EndRes
poll updateStatus=1  ->  4<sep>0<sep>1zirgtspghwq$$$$$$$$$$$$<sep>EndRes   (forever)
```

Field layout: `tor=4`, then `0`, then `curStatus` + `zirgtspghwq` + a
`$`-padded placeholder where the version/URL would appear.

Per the UI's own state machine, `curStatus` is `0` = search started,
`1` = searching, `2` = "Latest firmware present. CurFw:…". **It never leaves
`1`.** So the speaker never gets an answer from the update service — either it
has no outbound internet, or KEF has retired the gen-1 OTA endpoint.

Consequence: DNS/packet capture is only worth doing if the device actually emits
queries. Capture first, conclude second — don't assume the URL exists.

## Correction: the DSP block is NOT a fixed capability map

An earlier note in this log claimed `0x2a`/`0x2b`/`0x2c`/`0x2d` are "unsupported
on LSX" because they returned `0xff`. **That was wrong** — it was one snapshot,
and the block is state-dependent. After the speaker was power-cycled (to clear
standby), the same registers on the same unit read:

| Reg | Before power-cycle | After | Decoded after |
|---|---|---|---|
| `0x27` dsp_mode | `0xff` | `0x84` | desk=on, ext=extra, polarity=+ |
| `0x28` desk_db | `0x86` (−3.0 dB) | `0xff` | unsupported |
| `0x29` wall_db | `0x86` (−3.0 dB) | `0xff` | unsupported |
| `0x2a` treble_db | `0xff` | `0x84` | 0 dB |
| `0x2b` high_pass_hz | `0xff` | `0xff` | unsupported |
| `0x2c` sub_low_pass_hz | `0xff` | `0x88` | 80 Hz |
| `0x2d` sub_gain_db | `0xff` | `0x8a` | 0 dB |

Every post-reset value decodes to a **neutral default** (`0` dB, `80` Hz), which
fits a DSP re-initialisation. But the `0xff` mask moved *between* registers
rather than clearing, which a plain reset would not explain.

Rules for anyone building on this:

* `0xff` means "not currently exposed", **not** "this model lacks the feature".
  Never cache it as a capability.
* Do not trust a single sweep. Sample across power states (cold boot, standby,
  wake, each input) before writing a register map.
* The LS50-Wireless-vs-LSX capability story in `register-map.md` is therefore
  **unverified**. It may still be true; one snapshot cannot show it.

---

## 2026-11-03 — we have the firmware image

Full writeup: [`firmware.md`](firmware.md). Summary:

* The KEF Control APK (`com.kef.remote`) hardcodes
  `https://assets.kef.com/pm/pm_firmware/lsx/LSXUpdate.json`. Unauthenticated.
* That manifest points at `KEF_LSX_V520_S270_D100.fw` — 10 MB, also
  unauthenticated. Downloaded, hashed, analysed.
* **No packet capture, no UART, no root needed.** The app's strings were enough.
  The OTA-capture plan is obsolete; so is the `curStatus=1` dead end.
* The image is **plaintext** (entropy 6.0–7.5), so no key extraction problem.
* Build number at header `+60` is `105243`, matching the speaker's
  `p20.5202205110.105243`. Our unit is already on the latest published firmware.

### The big one: it is not Linux

`LSX Microchip MCU Application V5.2.0`, `SMSC`, `Microchip Digital Media
Adapter`, `VEN_14E7`, `ThreadX` + 136 `tx_*`, `ARM9`, `Thumb`, and no `Linux`,
`busybox`, `/bin/sh`, `squashfs`, `u-boot` anywhere.

The streaming module is a **Microchip/SMSC DMP (Digital Media Processor) — ARM9
running ThreadX**, with GoAhead, a UPnP stack, AirPlay/Spotify, and our TCP 50001
server.

The image also contains `qwhgpstgriz` (27 hits) — the delimiter we derived from
wire captures — which closes the loop between the protocol work and the binary.

### What this kills, and what it opens

Killed:

* "Get a shell and run our own control software." No POSIX, no linker, no
  process model. ThreadX + a single static application image.
* "UART gives us root." Expect a debug console, not a shell.
* The TV-jailbreak analogy generally. Those targets have an OS with a userland.

Opened:

* "Run our own software" = **patch this image**. Plaintext, ARM926, static. That
  is a firmware-patching project — tractable, and now the main line of work.
* The gating question is no longer *acquisition* but **verification**: does the
  device check a CRC32, or a real signature, before accepting an image?

---

## 2026-11-03 — firmware mapped; two builds, a shell, and an XMODEM dump

Full reference: [`firmware-map.md`](firmware-map.md). Headlines:

* **The `.fw` holds two distinct builds**, not one: section 1 (245,760 B) and
  section 2 (9,777,152 B) share only **19%** of their strings.
* **Load base is `0x400000`** — 5,490 of 8,320 `bl` targets cluster there.
  Strings are `adr`-relative, which is why three earlier approaches failed.
* **There is a shell.** `fburn shell commands not loaded. Load flashBurn module
  first` implies a command interpreter with loadable modules.
* **There is an XMODEM flash dump**: `Read flash out. Start xmodem now!` behind
  `Are you sure [y/n]?`. That is the recovery/dump route, in firmware, already.
* The `Sirius/` config namespace holds `Certificate` and `Keyblob`, loaded by a
  credential loader at `0x42c8b4`.
* The web surface is 25 handlers and 11 `.asp` pages, unauthenticated — including
  `aformHandlerTestHandler`.

Verification is still unresolved, but the search space is now small and named.

### Correction to earlier notes in this log

Earlier entries treated section 1 as "bootloader + recovery UI" and implied the
whole image was one program. Both are wrong in detail: the file carries two
separate builds, and the update progress state machine we saw as JS lives in
section 2's **embedded resource blob** (~8.4–9.0 MB), not in C code.

---

## 2026-11-03 — probing `aformHandlerTestHandler`

**Result: it is a benign self-test, not a backdoor.**

```
GET /goform/aformHandlerTestHandler
13qwhgpstgriz0qwhgpstgrizTested and sending backqwhgpstgrizEndRes
```

`tor=13`, then `0`, then the literal string `Tested and sending back` — which is
the string at file offset `0x24258` that the image references from code at
`0x2421e`. So the handler does exactly what its string says and nothing more.

Side effects worth knowing:

* `POST` with an empty body does **not** answer; it hangs until the client times
  out (~8 s observed).
* Immediately after that, the port-50001 control server accepted connections and
  then reset them (`Connection reset by peer`). It recovered on its own within
  ~5 s. So a stuck POST to a form handler can transiently disturb the control
  server — consistent with the single-session, single-threaded behaviour already
  documented for 50001.

Nothing here is exploitable, and nothing here was worth the RE effort that went
into trying to resolve its function pointer.

### Correction: the handler-table literals are unresolved

I claimed the form-handler table yields function pointers resolvable under load
base `0x400000`. That is **wrong**. The registration code at `0x20ad0` is a
repeating `ldr r1,=<value>; adr r0,=<name>; bl 0x41c420` sequence, and the
`<value>` literals are `0x4002xxxx` — an order of magnitude larger than the
`0x41xxxx` call targets that *do* resolve under `0x400000`.

No interpretation of those literals lands on a Thumb prologue: subtracting
`0x400000`, subtracting `0x40000000`, masking to 20 or 18 bits, swapping halves,
shifting — all 0/20 hits. `0x4002a134` sits inside the string
`"…nnectionServ…"`.

So the base for **pointer-valued data** is still open, even though the base for
**call targets** is solid. Recorded as unresolved rather than guessed at. The
practical consequence is small: the table still yields the *names* of all 25
handlers, which is what we actually wanted.

---

## 2026-11-03 — section 2 is ARM, and its data is relocated

Structural findings while chasing the verify chain offline:

* **Section 2 is ARM, not Thumb-2.** Section 1 is Thumb. Two instruction sets in
  one file reinforces that these are separate builds.
* **Section 2's pointers are RAM/relocated addresses**, not file offsets. Code
  at `0x58f000` loads `0x40c14350`, which maps into nothing under any base
  tried. The `adr`-based xref technique that works beautifully on section 1
  simply does not apply to section 2.
* The section-2 compiler **inlines short strings as immediates**
  (`0x20232323` = `"### "`), which is why some strings appear as data.

**Crypto located by constant**, which is the useful part:

| Primitive | Offsets |
|---|---|
| SHA-256 IV | S1 `0x15698`; S2 `0x2d393c`, `0x65af04`, `0x65b3e4` |
| SHA-1/MD5 IV | S1 `0x15498`; S2 `0x231a24`, `0x23e4c0`, `0x241240`, `0x24b844`, `0x287d44` |
| AES S-box | S2 `0x78081c` |
| RSA e=65537 | S1 `0x382c7`; S2 `0x282777`, `0x70bae7`, `0x70c1f9`, `0x71f6bd`, `0x722fd9` |
| **PKCS#1 v1.5 padding** | S2 `0x75a320`, `0x7fbb16`, `0x80405e`, `0x80407e`, `0x97a5bc` |

The PKCS#1 padding pair at `0x80405e`/`0x80407e` is the best lead for the
signature verify routine.

Also from this round, before the speaker went offline: all `.asp` pages except
`index.asp` return 404, so the recovery pages are not served by the running app;
and `formIos_HandlerAdvancedSettings` is live and 302s to a **`/1000/`**
namespace never mentioned by the shipped JS — an unexplored route tree.

---

## 2026-11-03 — REA works; PKCS#1 lead was a false positive

### REA is genuinely useful, via a trick

REA's Ghidra provider rejects the raw section (`unsupported binary format`).
Wrapping it in a minimal ELF32 ARM header fixes that:

```python
# 52-byte ehdr (ET_EXEC, EM_ARM, EABI5) + one PT_LOAD at vaddr 0 + the raw code
# base 0 keeps addresses equal to file offsets
```

That gave a working Ghidra session on section 2 (ARM), which is exactly what
headless Ghidra could not do. `open_binary` accepted it, `analyze_function`,
`procedure_pseudo_code`, `list_procedures` and `binary_overview` all work.
Wrappers are `captures/s2.elf` and `captures/s2-entry.elf`.

Caveats observed:

* A bare ELF with one PT_LOAD and no symbols gives Ghidra **nothing to seed
  analysis from**. It discovers no procedures until the entry point is a real
  function. Setting `e_entry` to a candidate prologue is the lever.
* `list_procedures` blew the MCP response budget on a 9.7 MB image; use
  `inspect_analysis_view` on the retained evidence, or CLI JSON.
* Choosing the wrong prologue yields a plausible-looking but meaningless
  decompile. Verify the bytes with objdump before trusting any C output.

### Correction: the PKCS#1 padding hits are false positives

I reported `0x80405e`/`0x80407e` as "the strongest lead for signature
verification". Wrong. That address range is the **embedded web resource blob**
(section 2, `0x700000`-`0x940000`), and objdump shows JavaScript:
`resObj.js`, `resObj.valueOf`, `alert(msg)`, `if(msg=="1")`. The `00 01 FF`
bytes are inside an HTML/JS asset.

Rule for this image: **filter byte-pattern hits by region before believing
them.** Anything in `0x700000`-`0x940000` is web asset text.

The code-region constants remain valid leads: SHA-256 IV at S2 `0x2d393c`,
`0x65af04`, `0x65b3e4`; RSA `e=65537` at S2 `0x282777`; SHA-256 IV at S1
`0x15698`.

---

## 2026-11-03 — REA raw-binary mode is the tool we needed

**The unlock.** REA can decompile a headerless blob directly; no ELF wrapper and
no function-boundary hunting:

```sh
rea decompile captures/s2.bin \
  --target-format raw-binary --architecture arm \
  --entry-point 0x58f000 --file-offset 0x2d393c \
  --provider ghidra --json
```

`--entry-point` is the **load base** (`address = base + file_offset`) and
`--file-offset` decompiles the containing function, so the "is this a prologue?"
problem disappears.

Verified: `0x58f000` → AirPlay/HomeKit pairing code (`s_X_Apple_PD`,
`s_X_Apple_HKP`); `0x2d393c` → flash erase routine calling
`FUN_0096c828(s_Erasing_...)`.

### Kuna is installed but not wired in

* `/usr/bin/kuna`, version **1.989** (REA requires >= 1.833, so fine).
* The `rea` MCP server in `~/.pi/agent/mcp.json` has only
  `GHIDRA_INSTALL_DIR` and `JAVA_HOME` in its `env` — no `REA_KUNA_COMMAND`, so
  the server reports `provider_unavailable` / `not_configured`.
* Workaround used: set `REA_KUNA_COMMAND=/usr/bin/kuna` for a single **CLI**
  invocation. No config change, no restart.
* To use Kuna from MCP tools in-session, add
  `"REA_KUNA_COMMAND": "/usr/bin/kuna"` to that server's `env` and restart the
  session — MCP env is read when the server is spawned.
* Kuna needs function seeds too: on a bare image it discovered only the ELF
  entry point. Its value is the cached whole-program export and exact-address
  xrefs, which pay off only once a base with real symbols/entries is supplied.

### What section 2 actually contains near the crypto

The SHA-256 IV at `0x2d393c` sits inside a **flash erase/address-computation**
function, not a hash routine — the constant is adjacent data. Treat constant hits
as leads to a neighbourhood, not to a function.

---

## 2026-11-03 — MAJOR correction: all verification leads were false positives

Kuna is now wired into the MCP config and works (`provider_id: "kuna"`, v1.989),
but it is seedless on a bare image — `xrefs` returned `examined_rows: 0`.

While testing it I re-checked the *surrounding strings* of every "verification"
hit instead of trusting the match. Every one was from a different subsystem:

| Hit | Actual context | Belongs to |
|---|---|---|
| `%##s: bad signature` | `unknown tsig error`, `bad key`, `bad time` | **DNS TSIG** |
| `no memory to check signature` | `src/login4.c`, `Spotify-Unavailable-For` | **Spotify login** |
| `Wrong Checksum` | `iPodSerialComm.cpp`, `telegram payload` | **iPod serial** |
| `Sirius`/`Keyblob`/`Certificate` | `SiriusService.cpp`, `SiriusXMFixedKey`, `/cfg/Sirius` | **SiriusXM DRM** |
| PKCS#1 padding | `resObj.js`, `alert(msg)` | **web JS** |

So the "secure boot" narrative in earlier entries was wrong on every count.
`Sirius` is SiriusXM satellite radio; section 2 has 267 such strings versus one
in section 1, and the loader at `0x2c8b4` reads SiriusXM DRM credentials from
the CnE store. It has no callers in section 1 at all.

**The only real firmware-integrity evidence** is `checksum failed` in section 1,
sitting beside `target=%x, FlashAddr=%x, size=%x` in the flash programming code.

Revised assessment: integrity is *probably* a checksum, not a signature — which
would make repacking feasible. That is a hypothesis, not a result.

Method lesson, now the third of its kind tonight: **read the neighbouring
strings before believing a match.** This image is dense with unrelated crypto
and "signature"/"checksum" vocabulary from DNS, WPA2, AirPlay, Spotify, iPod and
SiriusXM.

Also noted: `C:\BuildAgent\work\23b444ec...` appears in the firmware — the DMP
SDK was built on a Windows build agent.

---

## 2026-11-03 — ANSWERED: it is a Fletcher-32 checksum, not a signature

`rizin` + `rz-ghidra` (installed by the user) handles Thumb, which neither REA's
Ghidra provider nor headless Ghidra could. That unblocked section 1:

```sh
rizin -a arm -b 16 -m 0x400000 -A -q -c 'axt @ 0x401114' -c 'pdg @ 0x401000' -c 'q' captures/s1.bin
```

`axt @ 0x401114` (the `checksum failed` string) resolved to `fcn.00401000`, the
boot-time image verifier at file offset `0x1000`.

It reads a descriptor (target, FlashAddr, size, expected checksum), prints
`target=%x, FlashAddr=%x, size=%x`, reads the image out of flash, computes a
checksum over `size/2` words, compares, then either prints `checksum failed` and
hangs, or prints `read done` and **jumps into the loaded image**.

The checksum function `fcn.00400744` is **Fletcher-32** (init 0xFFFF/0xFFFF,
360-word blocks, folded 16-bit sums).

### Conclusion

**Firmware integrity on this path is a non-cryptographic checksum.** There is no
public key, no RSA and no ECDSA involved. Fletcher-32 can be recomputed for any
modified image, so **repacking is feasible in principle**.

Two caveats that still matter:

1. This is the **boot** verify. The OTA/USB **update** path may add checks not yet
   examined.
2. The expected value is reached via `[0x40da08]` → `0xe060b100`, a
   peripheral-looking address rather than a plain flash pointer. Whether that
   descriptor lives in **patchable flash** or in **OTP/eFuse** is unresolved, and
   it is the last thing standing between "feasible in principle" and "actually
   works".

Tooling note: the Thumb gap was the whole blocker. REA raw-binary is ARM-only
(`6502|x86|x86_64|arm|arm64|m68k`); rizin+rz-ghidra covers Thumb. Keep both.

---

## 2026-11-03 — section 1 is mixed ARM/Thumb (root cause of several dead ends)

`0x401000` (the flash verifier) is **ARM**: bytes `f0 41 2d e9` are
`push {r4,r5,r6,r7,r8,lr}`. `0x42c8b4` (the `Sirius` loader) is **Thumb**.

So section 1 interworks, and every earlier Thumb-only pass over it silently
produced garbage for the ARM half. In particular `tools/xref.py` reported zero
references to `checksum failed` **because the referencing code at `0x1000` is
ARM and the tool decoded it as Thumb**. The string was never unreferenced — my
tool just could not see the reference.

That also explains why the literal-pool search for `0x401114` failed: the
`adr r0, 0x401114` I was looking for does not exist. The ARM code uses
`add r1, pc, 0x88` from a different instruction encoding entirely.

Method note: pick the mode from the call graph (`bl` stays, `blx` interworks,
target bit 0 set = Thumb) before disassembling any region of section 1.

### On "can we just dump it all and walk through it?"

Yes, mechanically. Measured:

| | Instructions | Functions | ASM text |
|---|---|---|---|
| Section 1 | 102,544 | 192 | 3.8 MB / 102,552 lines |
| Section 2 | 1,834,193 | many | ~70 MB |

Section 1 is genuinely small enough to read in full — but only after the mode
problem is solved per function. Section 2 is a *search* target, not a reading
target.

The real obstacles are not volume:

1. **Mixed ARM/Thumb** — a single-mode dump is about half wrong.
2. **Code vs data** — strings, literal pools and jump tables decode as garbage.
3. **Relocated data in section 2** — pointers are RAM addresses
   (`0x40c14350`), so the call/reference graph does not resolve statically.

Targeted function reads beat exhaustive walks: the Fletcher-32 answer came from
`axt` on one string, not from reading 102k lines.

---

## 2026-11-03 — the expected checksum is hardware-provided, not in the image

Searched both sections for the descriptor address `0xe060b100`:

* **Section 1: exactly one occurrence**, at file offset `0xda08` — the literal
  pool slot itself.
* **Section 2: zero occurrences.**
* ARM-mode `axt @ 0x40da08` returns eight references, **all inside
  `fcn.00401000`** (the verifier itself).

So nothing in the firmware writes, maps or initialises that block. It appears
next to `0xb700002c` and `[0xe0007000]+0x50`, which the same function writes —
MMIO addresses. `0xe060b100` is therefore a **memory-mapped register block**
supplied by hardware/ROM at runtime, holding
`{target, FlashAddr, size, expected_checksum}`.

### What that means

* We **cannot** simply recompute Fletcher-32 and write it back into the image —
  the expected value is not in the image.
* If the block is a flash controller shadowing a header stored in flash, the
  header is patchable and repacking still works.
* If it is an OTP/eFuse window, repacking is blocked on this path.

**Static analysis is exhausted here.** The distinction requires reading
`0xe060b100` at runtime — UART or JTAG. That is now the single blocking
experiment for the whole "run our own firmware" ambition.

Worth recording plainly: we answered the original question (checksum, not
signature — Fletcher-32) from the image. The *next* question provably cannot be
answered from the image, and I'd have kept guessing at it if I hadn't checked
where that address actually comes from.

---

## 2026-11-03 — KEF shipped the platform's debug interfaces intact

The DMP platform is **BridgeCo AG** (`BridgeCo AG Telnet server`,
`bridgeco-jukeblox.net`), later SMSC then Microchip. KEF did not strip the debug
facilities.

**Two network ports were never tested**: `TelnetShellPort = 8000` and
`EthDRCPort = 10100` with `EthDRC = ENABLE`. The earlier scan covered 80, 443,
1900, 554, 4000, 7000, 8080, 9100 and 50001 only. Ports 8000 and 10100 are the
top untested surfaces.

The telnet shell (`TelnetShell.cpp`, `TelnetServer`, `TelnetShellService`)
carries `rd <addr> <count>` and `wr <addr> <value>`: arbitrary memory read and
write. `rd 0xe060b100` would settle the descriptor question immediately.

Also recovered from the default config table:

* `TCPIPTunnel = REMOTE_REGISTERS`, `TCPIPTunnelPort = 50001`. Our control port
  is the platform's generic remote-register tunnel, which explains why the
  "registers" we mapped are platform IDs.
* `Shell = UART1`, `iPodControl = UART0`. Debug shell on UART1; UART0 carries the
  Apple 30-pin accessory protocol.
* `bat <batch-name>` executes scripts from the CnE store at `cne/Shell/`. Since
  the web UI writes to CnE and `aformHandlerConfigureDMPName` emits
  `bad path(%s)`, an arbitrary-path write there would be remote code execution.
* `FirmwareIndexPage` defaults to `http://www.bridgeco-jukeblox.net/download/index.xml`,
  not `assets.kef.com`.
* Legacy web pages never tested: `/HardBoot.htm`, `/SoftBoot.htm`,
  `/FactoryDefaults.htm`, `/Configuration.htm`, `/test.html`.
* `/diag-info`, `/info`, `/auth-setup`, `/command` are AirPlay 2 RTSP endpoints,
  not debug endpoints.

Method note: this came from reading the *default config table*, not from
guessing at strings. The config table is the highest-yield structure in the
image and I should have gone looking for it much earlier.

---

## 2026-11-04 — live port scan: 10100 open, 8000 closed

After Ethernet was connected the device took about a minute to come up fully
(initially only 1900 and 8080 answered, then 80 and 50001 followed).

```
80:open   50001:open   8000:closed   10100:OPEN   8080:open
```

**Port 10100 is open and unauthenticated.** It streams display_tree XML:

```xml
<view id="browse"><text id="title">LSX 000000</text>
<text id="line0" flag="dsE:4">Media Server</text>...</view>
<view id="status"><icon id="play">empty</icon>...</view>
```

That title matches the unit MAC. It is the StreamUnlimited display protocol for
a front-panel LCD the LSX does not have, so it is vestigial. Inbound commands
are SPI-tunnelled (`newDRCCommand( %d 0x%08X )` followed by `SPI Command Th`),
including `DRC Reset Network` and `DRC Set JB Connect Mode`.

**Port 8000 is closed.** `TelnetShellEnable` appears exactly once in the image,
in the default config table, and nothing sets it at runtime. The shell with
`rd`/`wr` is therefore present but off.

That is the actionable lead: `TelnetShellEnable` is a CnE key, so any interface
that can write arbitrary CnE paths could enable the shell and with it arbitrary
memory read/write. Untested candidates: `aformHandlerConfigureDMPName` (emits
`bad path(%s)`), and the `cne/Shell/` batch path used by `bat`.

Also newly reachable: `/ui/ui.xml` (129 KB display_tree definition) and
`/spotify.asp` (8.8 KB). Every other legacy page 404s, including `/1000/`.

---

## 2026-11-04 — TelnetShellEnable cannot be flipped via the web API (hypothesis closed)

Attempted the CnE-write route to enable the telnet shell. It does not exist.

`aformHandlerConfigureDMPName` (Thumb, section 1) writes the device name to a
**hardcoded** CnE path:

```
0x4002c9c4   adr r0, "/cfg/Bluetooth"
0x4002c9ec   adr r0, "\npradeep->@@@@ Value of dmp name recieved:%s"
0x4002c9fc   adr r0, "\nBT Name Length %d: "
0x4002ca0a   cmp r0, 0x32                     ; 50 char limit
0x4002ca64   adr r1, "/cfg/Bluetooth"
0x4002ca66   adr r0, "aformHandlerConfigureDMPName: bad path(%s).\n"
```

The `bad path(%s)` message formats that same literal, so it is not
user-controllable. `webCfgNetGetCneValue` behaves the same way
(`bad path(/cfg/Networking/IpCfg)`). **The web API has no path-injection surface
for the CnE store**, so `TelnetShellEnable` cannot be set that way.

Two incidental finds:

* A developer's name is baked into a debug print: `pradeep->@@@@`, with the typo
  `recieved`. Vendor debug logging shipped in production.
* Section 1 loads at **0x40000000**, not 0x400000. Proven from the handler table:
  the literal `0x4002f581` is a Thumb pointer to file offset `0x2f580`, so
  `base = 0x4002f581 & ~1 - 0x2f580 = 0x40000000`. The earlier 0x400000 figure
  came from `bl` target clustering, which is PC-relative and therefore cannot
  determine a base at all. `tools/xref.py`'s reported addresses are off by that
  amount, though its string matching was unaffected because it compares relative
  positions.

### What is left for a shell

1. **Enumerate the 50001 remote-register space further.** `TCPIPTunnel =
   REMOTE_REGISTERS` is a generic register tunnel and only 0x00-0x40 was probed,
   read-only. There may be registers exposing CnE or memory.
2. **UART.** `Shell = UART1`, `uartcfg <device> <nr>`, and the shell has `rd`/`wr`.
   Physical access required.

---

## 2026-11-04 — full 256-register sweep: no generic CnE or memory access

Read-only sweep of `0x00`-`0xff` on one persistent connection (standby pinned to
never first). 32 registers answer; the rest return `52 12 ff`.

**Option 1 is closed.** The `TCPIPTunnel = REMOTE_REGISTERS` surface is a fixed
set of typed values. No register exposes the CnE store generically and none
exposes memory, so there is no path to `TelnetShellEnable` or to `rd`/`wr`
semantics over 50001.

### Correction: the register map is interface-dependent

Switching the unit from Wi-Fi to Ethernet changed `0x20`-`0x24`:

| Reg | Wi-Fi | Ethernet |
|---|---|---|
| `0x20` | `LSX` | `LSX 000000` |
| `0x21` | `kef-lsx-0000` | `FALSE` |
| `0x22` | `WPA2PSK` | `NONE` |
| `0x23` | `AES` | `open` |
| `0x24` | *WPA2 passphrase* | `open` |

So `0x20` is the DMP/device name (it matches the DRC title on 10100), and
`0x21`-`0x24` are Wi-Fi state: enabled flag, security mode, cipher, passphrase.
The passphrase is readable only while on Wi-Fi. Still an unauthenticated
plaintext credential exposure, but conditional on the interface.

This also explains earlier confusion: several "changed" register readings across
sessions were the network interface changing, not the firmware.

### New registers

`0x2e` returns `52 13 ff`, a third terminal reply shape (0x13), consistent with a
command register. `0x3d`-`0x43` are newly visible: `0x64`, `0x00`, `0x81`,
`0x82`, `0x82`, `0x80`. Unknown, possibly capability flags.

Terminal reply register bytes now known: `0x11` write ack, `0x12` unsupported,
`0x13` command ack.

### Both non-physical routes are now exhausted

1. Web API to CnE: no path injection (paths are literals).
2. 50001 remote registers: fixed surface, no CnE or memory access.

A memory-capable interface requires UART (`Shell = UART1`, `rd`/`wr`).

---

## 2026-11-04 — UPnP, AirPlay and Spotify Connect enumerated

Worked the remaining non-physical surfaces. Summary of what is reachable with no
credentials at all.

### UPnP MediaRenderer (8080, description at `/description.xml`)

Device: `friendlyName = LSX 000000`, `modelName = SP3994`, `serialNumber =
aabbccddeeff`, `deviceType = MediaRenderer:1`.

Four services, all answering SOAP with no auth:

| Service | Actions |
|---|---|
| RenderingControl | ListPresets, SelectPreset, Get/SetMute, Get/SetVolume |
| ConnectionManager | GetProtocolInfo, GetCurrentConnectionIDs, GetCurrentConnectionInfo |
| AVTransport | 16 actions incl. SetAVTransportURI, Play, Pause, Seek, Next, Previous |
| **QPlay** (`urn:schemas-tencent-com:service:QPlay:1`) | SetNetwork, QPlayAuth, InsertTracks, SetTracksInfo, GetTracksCount, GetMaxTracks, GetTracksInfo, RemoveTracks |

Confirmed live: `GetVolume` returned 50 (matches 50001), `GetTransportInfo`
returned `NO_MEDIA_PRESENT`, `GetDeviceCapabilities` returned `NONE,NETWORK`,
`GetMaxTracks` returned 300.

**`QPlayAuth(Seed)` works unauthenticated**: seed `1234` returns
`Code=d647aeefe5313569df59a18aab64fca0`, `MID=62900096`, `DID=KEF LSX`. A
challenge-response we can query freely, so the algorithm is recoverable from the
firmware if it matters.

**`SetNetwork(SSID, Key)` is an unauthenticated CnE write** of Wi-Fi credentials.
Not exercised: it would overwrite the Wi-Fi configuration, and the unit is
currently on Ethernet so it is recoverable but it is still a real state change.
This is the most significant security finding of this round: a write primitive
into the config store, unauthenticated, over UPnP. It is limited to SSID/Key, so
it does not reach `TelnetShellEnable`.

### AirPlay / RAOP (port 7000, advertised over mDNS)

mDNS advertises both `_airplay._tcp` and `_raop._tcp` on 7000, plus
`_spotify-connect._tcp` with `CPath=/goform/spotifyConfig`. Notably the AirPlay
and RAOP services are each advertised **twice** (with and without the MAC
suffix), which is worth remembering if the device is ever difficult to discover.

RTSP banner: `Server: AirTunes/366.0`, and
`Public: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, FLUSHBUFFERED, TEARDOWN, OPTIONS, POST, GET, PUT`.

`GET /info` returns a binary plist with no auth:

```
firmwareRevision    5202205110.105243422
firmwareBuildDate   Apr 17 2019
sdk                 AirPlay;2.0.2
build               17.0
model / name        LSX / "LSX 000000"
features            496155768982016
PTPInfo             OpenAVNU ArtAndLogic-aPTP-changes 1.1
nameIsFactoryDefault True
statusFlags         4
```

`acl=0` in the AirPlay TXT record means **no access control**: no pairing is
required to connect. `GET /diag-info` returns 404.

### Spotify Connect (port 80)

`/goform/spotifyConfig?action=getInfo` returns JSON with no auth:

```json
{"status":101,"statusString":"ERROR-OK","spotifyError":0,
 "version":"2.7.1","deviceID":"REDACTED","remoteName":"LSX 000000","activeUser":""}
```

Any other action name returns `Content-Length: 1` with body `{`, a truncated-JSON
bug, so the action vocabulary is small or different.

`/goform/lsx_api` exists (`WebCfgSpotifyZconf.cpp`) and always answers
`{"Status":"Fail"}`. It dispatches on an endpoint type drawn from
`service|cds|device|cardea|eastfork` (`cardea` and `eastfork` are Spotify Connect
codewords), but the parameter name and encoding were not identified. Left
unresolved.

### Verdict on non-physical avenues

Reachable without credentials, now documented: 50001 control (full), 80 web API,
10100 DRC state feed, 8080 UPnP with a CnE write, 7000 AirPlay with `acl=0`, and
Spotify Connect info.

None of it yields code execution. The only remaining non-physical route to that
would be **memory corruption in the AirPlay or UPnP parsers**, which is a
research project (fuzzing a live appliance), not a probe. Everything else is
control-plane and disclosure.

A memory-capable interface still requires UART.

---

## 2026-11-04 — prior art found, and the DMP is a documented part

Searched for existing teardowns. There are two, and one is thorough.

**AudioScienceReview, "KEF LSX Teardown and Repair"**
(<https://www.audiosciencereview.com/forum/index.php?threads/kef-lsx-teardown-and-repair.55697/>)
gives a full MAMP board component list. The DMP is a **Microchip CY920-C**
Network Media Module, alongside an ADAU1451 DSP, STM32F030 MCU, CS4272 codec,
PCM1754 DAC, WM8805 S/PDIF transceiver, CSRA64215 BLE, and a **TI CC8520
PurePath** radio for the master-to-slave link.

Two facts that directly change the physical plan:

* **"No screws can be seen from the outside; they are hidden by the back cover,
  which is pressure-mounted."** This settles the earlier "screws?" question. The
  rear cover is a press fit, not a screwed panel.
* **Blinking red/orange means general failure**, and the documented root cause is
  C433 (an apparently 16 V 1206 ceramic) cracked plus RF choke L236/L238 burned
  out, killing the +12 V rail from U20 (LM29150-12). Repairable for cents. The
  user's unit is on Ethernet and healthy, so its earlier orange flash was the
  Wi-Fi indicator, not this.

**The CY920 is a standard part and its debug port is documented.** Its block
diagram lists **"Debug Port UART 1"**, JTAG, UART 0 (Bluetooth baseband), SPI
0/1, I2C, I2S, S/PDIF, 16 MB serial flash and 64 MB DDR2. Microchip's Getting
Started Guide says "To view shell logs, connect the USB to serial adapter or
connect the RS232" and "On powering, the Bootloader software is executed."

So the console is a feature of the module, and it matches the KEF config key
`Shell = UART1` exactly. We are not looking for a hidden pad, we are looking for
the UART1 nets of a documented connector.

Documents needed (Microchip returns 403 to automated fetches, so these need a
browser): **DS60001270** data sheet (pinout) and **DS50002260B** getting started
guide.

Caveat: "DM920" searches are polluted by the Dreambox DM920 satellite receiver.
Search "CY920" or "DM920 Microchip".

Also useful: Darko.Audio has LS50 Wireless internals photos
(<https://darko.audio/2017/03/one-more-time-with-feeling-the-kef-ls50-wireless-at-6moons/ls50-wireless-internals/>),
and the LSX shares the platform.

---

## 2026-11-04 — Microchip documents the console: 230400 8N1, prompt `sds://>`

The user obtained both Microchip documents. They answer the remaining physical
questions outright.

**CY920 Getting Started Guide (DS50002260B)** and the manualslib copy of the same.

### Serial console settings (documented, not guessed)

`Baud rate 230400, Data 8 bit, Parity none, Stop 1 bit, Flow control none`. The
figure caption in the working session is `COM8:230400baud`. My earlier suggestion
of trying 115200 first was a guess from the `uartcfg` help text, which merely
uses 115200 as an example. Corrected in `uart-plan.md`.

### Prompt and boot behaviour

```
Wlan init
Disabling M3
Enabling M3
Starting IPOD DETECTION and IDPS
sds://>
```

Prompt is **`sds://>`**. Boot is loud: the guide's screenshot is full of
`FATAL TRACE ERROR` lines with source file and line numbers
(`UIMgr/PlayModeHandler.cpp[1101]`), including Pandora and Flickr service
complaints. Press Enter if the prompt is not there.

### The persistence chain this enables

Documented shell session:

```
sds://> cd cne/Application/NetIF
sds://> ls -v
sds://> set CurrentMode wired
sds://> netcfg
```

`set` writes arbitrary CnE keys. So one UART session yields all of:

1. `rd 0xe060b100` to settle whether the checksum descriptor is flash or fuse.
2. `set TelnetShellEnable 1`, reboot, and then the same shell arrives on
   **port 8000** over the network. The UART adapter becomes unnecessary.
3. A full flash dump via the bootloader's `Read flash out. Start xmodem now!`.

UART is therefore a one-time cost rather than a permanent tether.

### Still needed

The UART1 pin locations on the MAMP board, which live in **DS60001270** (CY920
data sheet, pinout), not in the Getting Started Guide. Microchip returns 403 to
automated fetches, so that one needs a browser download too.
