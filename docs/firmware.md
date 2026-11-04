# The firmware image

We have the real thing. This document is what it is and what's inside.

## How it was obtained

The KEF Control Android app (`com.kef.remote`) hardcodes the OTA manifest URL:

```
https://assets.kef.com/pm/pm_firmware/lsx/LSXUpdate.json
```

That manifest is unauthenticated and lists the current image:

```json
{
  "version": 520,
  "url": "https://assets.kef.com/pm/pm_firmware/lsx/KEF_LSX_V520_S270_D100.fw",
  "note": ["Fix EQ reset to default after power off/on"],
  "desc": ""
}
```

The image itself is also unauthenticated:

```
KEF_LSX_V520_S270_D100.fw   10,022,944 bytes
sha256 991b19704770cdcda2e5865a564ec5596b13e85bca78f57200e13fd5c8737179
```

**No packet capture, no UART, no root, and no speaker interaction was needed.**
The app's strings gave the URL directly.

Naming: `V520` = version 520, `S270` = system 2.7.0, `D100` = DSP 1.0. This
matches the identity block on the test unit exactly (`system 2.7.0`, `DSP 1.0`,
build `105243`), so the unit is already on the latest published firmware.

The release note — *"Fix EQ reset to default after power off/on"* — is an
independent confirmation of the DSP-reset behaviour observed on hardware (see
`findings.md`). It is also mildly ironic, given the register block still shifts
across a power cycle on this exact version.

Reproduce:

```sh
curl -sS https://assets.kef.com/pm/pm_firmware/lsx/LSXUpdate.json
curl -sSL -o KEF_LSX_V520_S270_D100.fw \
  https://assets.kef.com/pm/pm_firmware/lsx/KEF_LSX_V520_S270_D100.fw
```

## It is not encrypted

Entropy across the file is 6.0–7.5 bits/byte with a 2.4 tail — that is plaintext
code and data plus padding. Encrypted content would sit at ~8.0 and a compressed
rootfs at ~7.9 throughout.

So there is **no Samsung-style key extraction problem here**. No padding oracle,
no TrustZone dance, no `swu/`-equivalent needed. The image can be read directly.

## Container header

The first `0x3c000` (245,760) bytes are a header/preamble; the payload that
follows is `0x953000` (9,777,152) bytes, and the two plus a 32-byte tail account
for the file size.

Little-endian words from offset 0:

| Offset | Value | Meaning |
|---|---|---|
| `+0` | `0x00000002` | container format version |
| `+4` | `0x00953000` | payload size (9,777,152) |
| `+12` | `0x0003c000` | header size (245,760) |
| `+32` | `bCoD` | magic |
| `+40` | `20190710074132` | build timestamp, 2019-07-10 07:41:32 |
| `+60` | `0x00019b1b` (105,243) | **build number — matches the speaker's `p20.5202205110.105243`** |
| `+64` | `0x000200bd` | unknown |
| `+68` | `0x0007f510` | unknown |
| `+80..+188` | 32-byte-stride records | per-section table: address, size, CRC32-ish words |
| `+208` | `DMP 3.x` | Digital Media Processor section magic |

The `+80` table entries look like `{u32 addr, u32 size, u32 size2, u32 crc}`
with trailing zero padding, but the field order is **not yet confirmed** — treat
it as a to-do, not a spec. Values seen include `0x00812994`, `0x00825d84`,
`0x0000d528` (addresses) and `0x003c0000`, `0x0012d188`, `0x0011be78` (sizes),
plus four 32-bit words that look like CRCs.

## What's inside

String evidence, with offsets:

| Evidence | Hits | Meaning |
|---|---|---|
| `LSX Microchip MCU Application V5.2.0` @9825520 | — | **the application image, named** |
| `SMSC` | 12 | SMSC = Standard Microsystems, acquired by Microchip |
| `Microchip Digital Media Adapter with UPnP` @8102236 | 4 | UPnP device description |
| `VEN_14E7&DEV_0001` | — | USB IDs; `0x14E7` is SMSC's vendor ID |
| `ThreadX` / `tx_*` | 7 / 136 | **RTOS: ThreadX (Azure RTOS)**, not Linux |
| `GoAhead` | 11 | the web server we've been poking |
| `index.asp` | 11 | the web UI pages |
| `aformHandler` | 66 | the `/goform/` handlers |
| `qwhgpstgriz` | 27 | **the exact delimiter we reverse-engineered off the wire** |
| `EndRes` | 2 | the protocol terminator |
| `wpa_supplicant` / `hostapd` | 42 / 14 | Wi-Fi supplicant and AP |
| `AirPlay` / `Spotify` | 80 / 380 | streaming protocol implementations |
| `ARM9`, `Thumb` | 1, 3 | CPU core is ARM9-class (ARM926) |
| `DMP`, `dmp_` | 54, 6 | Digital Media Processor subsystem |
| `DSP` | 86 | DSP image / control |

Absent entirely: `Linux`, `busybox`, `/bin/sh`, `squashfs`, `UBI`, `jffs2`,
`u-boot`, `init=`, `console=`, `rootfs`.

## Conclusion: this is not a Linux box

The streaming module is a **Microchip/SMSC DMP (Digital Media Processor) —
ARM9 running ThreadX**, with a GoAhead web server, a UPnP stack, the AirPlay and
Spotify implementations, and the TCP 50001 control server. KEF's "MCU" version
in the identity block is this application.

This single fact reorganises the whole project:

* **There is no shell and no `root` to get.** No POSIX, no dynamic linker, no
  process model. The Samsung/LG/OpenWrt playbook — get a shell, run binaries —
  does not apply.
* **UART will not give a shell.** Expect a ThreadX/debug console: boot logging,
  maybe a vendor CLI. Useful for a memory dump and for the boot log, not for
  running our own code.
* **"Run our own control software" means patching this image.** A single
  statically-linked ThreadX application for ARM926. That is a firmware-patching
  project, not a software-installation project.
* **The upside:** the image is plaintext, so patching is at least tractable —
  the blocker is the update signature check, not cryptography.

## Section layout

`0x3c000 + 0x953000 + 32 = file size`, so the image is **two sections plus a
32-byte tail**. The tail is `0xff` padding ending in `76 74` — not a signature.

### Section 1 — `0x000000`–`0x03c000` (245,760 B): bootloader + recovery UI

Starts with DDR initialisation and flash programming:

```
Init DDR Done · Single read/write DDR Fail · --------> Phy Init
PHY:DX0GSR0.dtpass · upctl:POWSTAT · DDR_UMCTL_INIT
target=%x, FlashAddr=%x, size=%x · checksum failed
```

and from ~124 K onward it holds **GoAhead, `index.asp`, `aformHandler`,
`qwhgpstgriz`** — the recovery web UI we have been poking, with its firmware
update flow.

At ~182 K sits a secure-boot subsystem: **`Sirius` (268 hits), `Keyblob` (10),
`Certificate`, `ERROR getting certificate`, `ERROR getting Key`**.

### Section 2 — `0x03c000`–`0x98f000` (9,777,152 B): the main application

Begins immediately with ARM Thumb-2 (`4f f0 ff 30` = `mov.w r0,#0xff`) and the
string `"doAckPowerChange() entered"` / `PowerManager.cpp`. Contains ThreadX,
PowerManager, wpa_supplicant, hostapd, AirPlay, Spotify, a second copy of
GoAhead/`index.asp`, `LSX Microchip MCU Application V5.2.0` near the end, and
`bad signature` at 2,378,766.

Both sections carry a web server, which fits: section 1 is the recovery
environment, section 2 the running product.

## Verification: probably a signature, not a CRC

Evidence so far:

* `crc32` of each section does **not** match any of the four hash-like words in
  the header (`0xc4a2b2c4`, `0xef218632`, `0x77313e02`, `0x9b96fdd4`), nor does
  `adler32`. So the obvious "CRC32 of each section" model is wrong.
* The bootloader carries a `Sirius`/`Keyblob`/`Certificate` subsystem, which is
  the shape of signature-based secure boot rather than a checksum.
* `bad signature` and `generate signature failed (%d)` strings exist in the
  image.
* The other crypto strings are **not** firmware signing and should not be
  confused for it: `RSA`/`RSA Key Length` @1,652,332 and `Certificate` @165,233
  are TLS/EAP; `SHA256`/`NSEC3` @3,836,956 is the DNS resolver; `WPS: DH public
  keys` @6,953,972 is Wi-Fi Protected Setup.

**Not yet proven.** `checksum failed` in the bootloader is real, and
`ERROR getting certificate` / `ERROR getting Key` may be recoverable paths. The
open question is precisely which check the *update* path enforces, and whether
the USB route is laxer than the internet route.

Tooling is already available for the next step: `arm-none-eabi-objdump` and
`ghidra` are both installed.

## Next questions

1. **Is the image verified on the device?** The 32-byte tail and the CRC-ish
   table entries suggest integrity checking. If the device only checks CRC32,
   a patched image is feasible. If it verifies an RSA/ECDSA signature, we need
   the key or a way around the check.
2. **Does the OTA path verify the same way as the USB path?** The web UI can
   update from a USB stick (`aformHandlerObtainUsbBcdList`), which may be a
   laxer code path than the internet update.
3. **What exactly is in each section?** Resolve the `+80` table, then carve:
   bootloader, DMP application, DSP image, resources (the web UI is probably a
   filesystem image or an embedded blob).
4. **Is the DSP image separately verified?** `D100` is its own component with
   its own version.

## Handling

The `.fw` and the `.apk` live in `captures/`, which is gitignored — they are
large binaries, not secrets. Recorded here by hash so they are reproducible.
Note that the *speaker's* captures (registers, `dump`, `--jsonl`) do contain the
WPA2 PSK and must stay private; this image does not.

## Static RE attempt: what worked, what didn't

Recorded so nobody repeats it.

**Worked.** Section 1 is **Thumb-2** code (`4770` `bx lr`, `b500` `push {lr}`,
`e8bd 87f0` `ldmia.w sp!`). It addresses strings **PC-relatively**:

```
44a:  4905   ldr  r1, [pc, #20]
44c:  4479   add  r1, pc          <- r1 = literal + (pc & ~3)
```

**Didn't work.** Three approaches, all negative:

1. *Absolute-literal search.* Scanning for `u32 == string_offset + base` over
   candidate page-aligned bases found nothing. Expected, in hindsight: the
   addressing is PC-relative, so no absolute address is ever encoded.
2. *Solving for the load base by intersecting reference deltas.* Built the set
   of `literal - string_offset` for six known strings and intersected: empty.
   Resolving all 107 `ldr [pc,#n]` + `add rd, pc` pairs and histogramming the
   deltas also gave no dominant value.
3. *Header words as checksums.* Tested `crc32`, `adler32`, `sum32`, `sum8`,
   `xor8`, and `md5`/`sha1`/`sha256` head and tail 32-bit words over six
   plausible ranges (each section, both, minus tail, etc.) against all seven
   header candidate words. **No match.**

Conclusion: the header words are either hashes over boundaries we haven't found,
or signatures. Combined with the `Sirius`/`Keyblob`/`Certificate` strings this
still leans **signature**, but it is not proven.

**Why it stalled.** The image has no ELF/PE container, so there is no load
address, entry point, or symbol table to anchor on, and the load base is not
equal to the file offset. Doing this properly means a **Ghidra project with an
explicit base address** (or a debugger attached over UART/JTAG), not ad-hoc
disassembly. Both `ghidra` and `arm-none-eabi-objdump` are installed.

**Cheaper decisive alternative:** get the boot log over UART. It will name the
load addresses and print the actual verification verdict on a rejected image —
which is the answer we want, from the device's own mouth, without guessing.

## Reverse engineering results (section 1)

### The load base is 0x400000

Solved, and it was the thing blocking everything. Of 8,320 `bl` targets,
**5,490 fall in the `0x400000` range**, and every one maps to a plausible file
offset (`bl 0x41eae4` → file `0x1eae4`). So:

```
address = file_offset + 0x400000
```

### How strings are addressed

Not absolute literals — that is why the first three attempts failed. The
compiler emits:

```
2c8c6:  add r1, pc, #140   @ (adr r1, 0x2c954)   ; "Certificate"
```

objdump resolves the target in a comment, so no PC arithmetic is needed.
`tools/xref.py` parses exactly this, plus `ldr [pc]` + `add pc` pairs and
`movw`/`movt`, and infers the base from `bl` clustering:

```sh
tools/xref.py captures/s1.bin --map
tools/xref.py captures/s1.bin --xref 0x2c954
tools/xref.py captures/s1.bin --region 0x2c860-0x2c9c0
```

### A secure-boot credential loader at 0x42c8b4

```
42c8b4  push {r4,r5,r6,lr}; sub sp,#32
42c8be  bl   0x403930                     ; init two temp strings
42c8c6  adr  r1, 0x2c954                  ; "Certificate"
42c8c8  adr  r0, 0x2c960                  ; "Sirius/"
42c8ca  blx  0x403424                     ; lookup("Sirius/", "Certificate", &buf)
42c8ce  cbz  r0, 0x2c940                  ; NULL -> "ERROR getting certificate\n"
        ... memcpy into out->cert, out->certLen ...
42c8f6  adr  r1, 0x2c96c                  ; "Keyblob"
42c8f8  adr  r0, 0x2c960                  ; "Sirius/"
42c8fa  blx  0x403424                     ; lookup("Sirius/", "Keyblob", &buf2)
42c8fe  cbz  r0, 0x2c94a                  ; NULL -> "ERROR getting Key\n"
        ... memcpy into out->key, out->keyLen ...
42c91a  str r5,[r4,#0] ; strd r0,r6,[r4,#4] ; str r0,[r4,#12]
```

It fills `{cert_ptr, cert_len, key_ptr, key_len}` from a **path-based config
store** and logs `ERROR getting certificate` / `ERROR getting Key` on failure.
That is a certificate + keyblob pair — the shape of signature-based secure boot.

The store is the same one the web UI uses: paths like `/cfg/Networking/IpCfg`,
`/cfg/FirmwareDownload`, `Sirius/`. So the bootloader and the application share
a hierarchical key/value store, and the boot keys live under `Sirius/`.

**Caveat:** no `bl` callers and no function-pointer references to 0x42c8b4 exist
in section 1, so this loader is either called from section 2 or reached through
a computed table. It proves the mechanism exists; it does not yet prove when it
runs.

### The firmware update path, mapped

```
SearchFwUrl              persistent parameter, not compiled in
  -> fetch manifest       (bootloader has its own TCP/SSL/DNS stack)
  -> GetBcdNameAndUrlForFirmwareUpdate
  -> CountOfBCD
  -> /cfg/FirmwareDownload { DownloadURL, BCDUrl }
  -> Firmware_update_prepare.asp
  -> download + flash
```

Relevant strings: `SearchFwUrl`, `SearchFwUrlStart`, `CountOfBCD`,
`GetBcdNameAndUrlForFirmwareUpdate`, `BCDUrl`, `DownloadURL`,
`/cfg/FirmwareDownload`, `Firmware_update_prepare.asp`,
`aformHandlerObtainUsbBcdList`, `formPrepareFirmwareUpdateHandler`.

Note `SearchFwUrl` is a **persistent parameter, not a compiled-in constant** —
there is no `assets.kef.com` URL anywhere in the image. The manifest URL we
pulled out of the APK is either pushed to the speaker by the app or provisioned
at factory time.

### Still not located: the verification step

The loader gets the credentials; something else must consume them to verify an
image. That consumer is the remaining target. It is almost certainly in the
download/flash path above, reachable from `Firmware_update_prepare.asp` and the
flash driver (`Flash driver returned invalid blcok size!!! Panic!!!` — note the
typo, a good signature for the flash module).

### Ghidra: negative result, recorded

Ghidra 12.1.2 headless could not be made to decode this image as Thumb:

* `ARM:LE:32:v5` and `ARM:LE:32:v5t` both decode ARM at `0x400`.
* Thumb mode is a `contextreg` field `TMode` (bit 0) per `ARM.sinc`, but it is
  **absent from the compiled `.sla`** and unreachable via
  `lang.getRegister("TMode")`, `currentProgram.getRegister("TMode")`,
  `getContextRegisters()`, or `getContextBaseRegister().getChildRegisters()`.
* Writing the whole 64-bit `contextreg` to 1 makes `disassemble()` fail outright.
* `-loader-baseAddr 0x400000` plus `v5t` still yielded no instructions.

`objdump` decodes it correctly with `-M force-thumb`, so that is the tool of
choice. Ghidra would still be worth it for the decompiler, but it needs the GUI
(where Thumb is a tick-box) or a hand-built `.sla`. Recorded so the next person
does not re-walk it.
