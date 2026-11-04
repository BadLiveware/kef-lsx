# Firmware map — a reference for poking at this device

What we know about the LSX firmware, where it lives, and what to probe next.
Companion to [`firmware.md`](firmware.md) (provenance and container format) and
[`uart-plan.md`](uart-plan.md) (physical access).

**Conventions:** file offsets are into `KEF_LSX_V520_S270_D100.fw`. Section-1
code addresses are `file_offset + 0x400000` (proven). Anything marked
*unresolved* is a guess and is labelled as such.

---

## 1. The container holds two different builds

```
0x000000 – 0x03c000   245,760 B   section 1
0x03c000 – 0x98f000 9,777,152 B   section 2
0x98f000 – 0x98f020        32 B   tail: 0xff padding + "76 74"
```

They are **not** duplicates: only 19% of section 1's strings appear anywhere in
section 2 (146 shared, 629 section-1-only, 29,093 section-2 strings). Two
distinct builds shipped in one file.

| | Section 1 | Section 2 |
|---|---|---|
| Size | 245,760 B | 9,777,152 B |
| Code | ~102,500 Thumb-2 insns | code + resource blob |
| Role | bootloader + **recovery** web UI | the main application |
| DDR init, flash driver | yes | yes (separate copies) |
| GoAhead + `index.asp` | yes | yes |
| `aformHandlerRestartNetworkSettings` | yes | — |
| `LSX Microchip MCU Application V5.2.0` | — | yes, @9,825,520 |

**Unresolved:** whether section 1 is a separate *partition* (recovery) or a
second *product variant*. The 19% overlap plus a full network stack in a 245 KB
image fits "recovery build that links the shared libraries", but that is
inference, not proof.

### Section 2 region map

| Range (in section) | Content |
|---|---|
| `0x000000`–`0x6c0000` | code (entropy 5.8–7.1, dense strings) |
| `0x6c0000`–`0x700000` | transitional, sparse strings |
| `0x700000`–`0x940000` | **embedded web resources / string tables** (entropy 4.4–5.4 with 1,400+ strings per 64 KiB — HTML/JS/CSS, not code) |
| `0x940000`–`0x953000` | padding (entropy 0.05) |

That resource blob is why `fwProgStatus` / `flashprogStatus` appear at 8.66 MB:
they are **JavaScript**, not C.

---

## 2. Code model

**Section 1 is MIXED ARM and Thumb; section 2 is ARM.**

This matters more than anything else in this document. Section 1 is not
uniformly Thumb — it interworks:

| Address | Mode | ARM decode | Thumb decode |
|---|---|---|---|
| `0x401000` (flash verifier) | **ARM** | correct | `rors r0, r6` (garbage) |
| `0x42c8b4` (`Sirius` loader) | **Thumb** | `addlt fp, r8, r0, ror r5` (garbage) | correct |

Consequences:

* A **single-mode dump of section 1 is roughly half wrong**, in whichever mode
  you pick. `objdump -M force-thumb` over the whole section silently produced
  garbage for the ARM half, which is why `tools/xref.py` reported *zero*
  references to the `checksum failed` string: the referencing code at `0x1000`
  is ARM, and the tool disassembled it as Thumb.
* The reliable way to pick a mode per function is the call graph: `bl` stays in
  mode, `blx` interworks, and a target with bit 0 set is Thumb.
* `pdg`/`pd` output must be cross-checked against the other mode before being
  believed — a wrong-mode decode looks like plausible instruction soup.

```sh
# section 1
arm-none-eabi-objdump -D -b binary -m arm -M force-thumb ...
# section 2  (note: no force-thumb)
arm-none-eabi-objdump -D -b binary -m arm ...
```

Section 2 also differs in how it reaches data:

* Data pointers are **relocated / RAM addresses**, not file offsets. Code at
  `0x58f000` loads `0x40c14350` from its literal pool, which does not map into
  the image at any base tested. So the file-offset string-xref technique that
  works on section 1 **does not work on section 2**.
* The compiler sometimes **inlines strings as immediates**:
  `0x20232323` = `"### "`, `0x746e6f43` = `"Cont"` — which is why
  `"### Control pair-verify"` shows up as data in odd places.

### Section 1 code model (verified)

* **Thumb-2**, ARM9-class (ARM926EJ-S), little-endian.
* **Load base: `0x400000` for call targets — proven.** Every function called
  from the coherent code resolves under it: `bl 0x41c420` → file `0x1c420`,
  `bl 0x41fa1c` → `0x1fa1c`, and so on, all in range for a 245,760-byte image.

  **But the handler-table literals do not.** The form-registration function
  stores values like `0x4002a135` — a different magnitude, and *no*
  interpretation tested puts them on a function prologue:

  | Interpretation | Prologue hits |
  |---|---|
  | `v - 0x400000` | 0/20 |
  | `v - 0x40000000` | 0/20 |
  | `v & 0xFFFFF`, `v & 0x3FFFF` | 0/20 |
  | halves swapped, `v >> 8` | 0/20 |

  `0x4002a134` lands inside the string `"…nnectionServ…"`, so it is definitely
  not code. **Status: unresolved.** Either those literals are relocated at
  runtime, they are not function pointers at all (a flags/length field, with the
  handler derived another way), or the registration ABI differs from the obvious
  `websFormDefine(name, fn)` shape.

  Do not build on the handler table until this is settled. Everything else in
  this document that rests on `0x400000` — call targets, string xrefs, the
  `Sirius` loader — is independently verified and unaffected.
* Strings are **PC-relative** via `adr`, never absolute literals:

  ```
  2c8c6:  add r1, pc, #140   @ (adr r1, 0x2c954)   ; "Certificate"
  ```

* `objdump -M force-thumb` decodes it correctly; Ghidra headless does not
  (see [`firmware.md`](firmware.md#ghidra-negative-result-recorded)).

### Decompiling section 2 with REA (the working recipe)

REA's Ghidra provider rejects a raw blob (`unsupported binary format`) and a
minimal ELF wrapper works but gives Ghidra **nothing to seed analysis from** —
it discovers only the ELF entry point, so you get one real function and nothing
else.

The correct approach is REA's **native raw-binary mode**, which needs no wrapper:

```sh
rea decompile captures/s2.bin \
  --target-format raw-binary --architecture arm \
  --entry-point 0x58f000 --file-offset 0x2d393c \
  --provider ghidra --json
```

* `--entry-point` sets the **load base**, so `address = base + file_offset`.
  With `--entry-point 0x58f000`, file offset `X` maps to `0x58f000 + X`.
* `--file-offset` decompiles the function *containing* that offset, so you do not
  need to know the function boundary in advance. This is the key feature — it
  removes the "find the prologue" problem entirely.
* Pick a base where you have already confirmed coherent code (e.g. `0x58f000`,
  verified against objdump).

Confirmed working output: `0x58f000` decompiles to AirPlay/HomeKit pairing code
(`s_X_Apple_PD`, `s_X_Apple_HKP`), and `0x2d393c` to a flash erase routine
(`FUN_0096c828(s_Erasing_...)`).

Use `tools/xref.py` for fast bulk string xrefs on section 1:

```sh
tools/xref.py captures/s1.bin --map                       # all string xrefs
tools/xref.py captures/s1.bin --xref 0x2c954              # who uses a string
tools/xref.py captures/s1.bin --region 0x2c860-0x2c9c0    # read code
```

---

## 3. Subsystems, by evidence

### RTOS and platform
`ThreadX` / 136 `tx_*` symbols. Not Linux — no POSIX, no dynamic linker, no
shell in the Unix sense. Single statically linked application.

### Flash and burning — **the most useful area for us**
| Offset | String |
|---|---|
| 3,205,189 | `MIOSPI-Driver Panic!!!!!! Unknown External Flash type` |
| 3,205,369 | `Total Flash Memory : Internal %d MB External %d MB` |
| 4,235,816 | `queryFlashBlockSize::queryBlockAddress Failed!!!` |
| 5,808,296 | `Could not read out flash.` / `flashBurn.cpp` |
| 5,808,996 | `initBurn: xmodem object could not be allocated.` |
| 5,809,071 | `initBurn: xmodemReceive failed.` |
| 5,809,181 | `%d bytes burnt` |
| 5,809,448 | `Erase address space at 0x%8.8X` |
| 6,290,056 | `flashBurn` · `fburn shell commands not loaded. Load flashBurn module first` |
| 6,177,400 | `Flashing routine must not be in flash, cannot continue!` |
| 6,314,285 | `***getFlashSpaceStateString***` → `NOT_USED` `BEING_WRITTEN` `ACTIVE` `BEING_REMOVED` |

**There is a shell.** `fburn shell commands not loaded. Load flashBurn module
first` implies a command interpreter with loadable modules. And:

| Offset | String |
|---|---|
| 3,471,296 | **`Read flash out. Start xmodem now!`** |
| — | `Are you sure [y/n]?` |
| — | `Select a binary file for download!` |

That is a **flash dump over XMODEM, behind a y/n prompt**. If we reach that shell
over UART, the dump problem is solved with no flash surgery. This is the single
highest-value target in the whole image.

Also present: `MultiFlashManager`, `FlashPartitionManager`, `Ram Flash`,
`Parallel Flash`, `Serial Flash`, `(flash sector size= 0x%x=%u)`.

### Config store ("CnE")
Hierarchical path-based key/value store shared by bootloader, app and web UI.

```
/cfg/FirmwareDownload          { DownloadURL, BCDUrl }
/cfg/Networking/IpCfg
/cfg/Networking/DrvCfg/WlanCfg/Profile4
/cfg/Networking/DrvCfg/WlanCfg/RxPowerAdj
/cfg/Networking/DrvCfg/WlanCfg/TxPower
/cfg/Application/NetIF         wired | wireless | autosense
Sirius/                        { Certificate, Keyblob }
```

Diagnostics: `CnE couldn't read attrNameSpaceStartAddr from Flash!`,
`CnE Erased Successfully`, `PersistentStringList::erase %d not found`,
`persParamUpdate(%s)`, `persParamUpdateIfNotExist(%s)`.

### Networking
Own TCP/SSL/DNS stack (`gethostbyname failed`, `SSLClient::connect failed`,
`TCP_CONNECT failed`), Wi-Fi driver with regulatory TX-power tables
(`2G_20_EU`, `5G_40_JP`, `ANT_GAIN_2G`), `wpa_supplicant` + `hostapd` +
`WPS: E-S1/E-S2`.

### Streaming services
AirPlay/RAOP and **HomeKit** (`/pair-verify`, `Pair-Verify-ECDH-Info`,
`PairVerifyClient/Server`), Spotify, UPnP/DLNA + SSDP, RTSP, iPod USB
(`iPodUsbComm.cpp`, `IPodAuthCoprocessorV2Impl`), legacy Napster
(`api.dev.napster.com`).

### Power
`PowerManager.cpp`: `doAckPowerChange`, `executeOsPwrChange`,
`Restarting System and Stopping at Bootloader...`, `powerChangeTimedOut`.
Matches the wedging behaviour we hit on port 50001.

### "Sirius" is SiriusXM, not secure boot

**Correction.** This was originally written up as a secure-boot credential
loader. It is not. `Sirius` in this firmware is **SiriusXM satellite radio**:

```
SiriusService.cpp   SiriusXM_TokenUpdate   SiriusContentBrowser   SiriusDecoder
/cfg/Sirius         SiriusXMFixedKey       SiriusXMGeneratePWKey  SiriusXMLParser
SiriusContentRetriever.cpp   SiriusXMLParser.cpp   SiriusZCLParser
```

Section 1 has **one** `Sirius` string (the `Sirius/` namespace); section 2 has
**267**. The loader at section-1 `0x2c8b4` does
`lookup("Sirius/","Certificate")` + `lookup("Sirius/","Keyblob")` and copies
them into `{cert, certLen, key, keyLen}` — that is **SiriusXM DRM material**
read from the CnE config store, not firmware trust anchors.

It also has no callers in section 1 (searched for `bl 0x42c8b4`, `blx
0x42c8b4`, and the pointer `0x42c8b5`/`0x42c8b4` — all empty). Consistent with
DRM code that this build does not use.

---

## 4. Web API surface

Reachable unauthenticated on port 80. Handler names recovered from the image
(25), which is a superset of what the shipped `airplayRelJavScr.js` uses:

```
aformHandlerConfigureStandByMode      aformHandlerObtainUsbBcdList
aformHandlerConfigureDMPName          aformHandlerRestartNetworkSettings
aformHandlerConfigureSSIDList         aformHandlerRestartNotify
aformHandlerConfigureProfileSettings  aformHandlerConfigureJbAp
aformHandlerConfigureAirplayPassword  aformHandlerConfigureBluetoothName
aformHandlerIfBluetoothPlaying        aformHandlerIfHomeAdded
aformHandlerSpotifyDeleteUser         aformHandlerSpotifyLogOutUser
aformHandlerSpotifySwitchUser         aformHandlerTestHandler
```

Pages (11): `bl_index.asp`, `index.asp`, `network_config_error.asp`,
`network_config_success.asp`, `success.asp`, `fail.asp`,
`Firmware_update_prepare.asp`, `Firmware_update_prepare_for_usb.asp`,
`fwupdatestatus.asp`, `firmware_update_progress_page.asp`,
`wireless_network_config.asp`.

`aformHandlerTestHandler` is worth a look — a test handler shipped in production
is often the least-guarded one.

---

## 5. Firmware update path

```
FUOption = select | default | LAN | internet | usb     (method selector)
  -> SearchFwUrl            persistent parameter, NOT compiled in
  -> fetch manifest         bootloader/app has its own TCP+SSL+DNS
  -> GetBcdNameAndUrlForFirmwareUpdate
  -> CountOfBCD
  -> /cfg/FirmwareDownload { DownloadURL, BCDUrl }
  -> Firmware_update_prepare.asp  -> fwupdatestatus.asp
  -> download -> verify -> erase -> burn
```

Progress states, from the embedded JS at 8,668,920:

```
Firmware Update Idle | started | downloading | downloadComplete
Verifying image | Download Update flash | download update done
download failed | failed

Flashing not started | Erasing Flash | Burning...
```

**Note:** `SearchFwUrl` is a persistent parameter, not a constant — there is no
`assets.kef.com` string in the image. The manifest URL we extracted from the APK
is pushed by the app or provisioned at the factory.

---

## 6. Verification: it is a Fletcher-32 checksum, NOT a signature

**Answered.** Section 1 contains the boot-time image verifier at file offset
`0x1000` (address `0x401000`), found with rizin + rz-ghidra as Thumb:

```c
fcn.00401000:                       // push {r4-r8,lr}; sub sp,0x38
  p   = [0x40da08]                  // descriptor pointer
  r5  = [p+0]  + 0x40000000         // target address
  r6  = [p+4]  + 0xb6000000         // FlashAddr
  r4  = [p+0]  + 0x40000000         // destination
  size= [p+8]
  fcn.004011cc(0x3b, 1)             // driver setup
  *(0xb700002c) = 0xf               // hardware register writes
  *([0x4010ec] + 0x50) = 0xa
  sprintf(buf, "\ntarget=%x, FlashAddr=%x, size=%x\n", r5, r6, size)
  print(buf)
  fcn.004002ac(r5, r6, size)        // read image out of flash
  computed = fcn.00400744(r4, size/2)   // <-- checksum over size/2 words
  if ([p+0xc] != computed) {
      print("checksum failed\n");
      fcn.00401868(0); fcn.004012b4();
      for(;;);                      // hang
  }
  print("read done\n");
  ((void(*)(void))r4)();            // jump into the loaded image
```

The checksum (`fcn.00400744`) is **Fletcher-32** over 16-bit words:

* `sum1 = 0xffff`, `sum2 = 0xffff`
* blocks of `0x168` (360) words
* per word: `sum1 += w; sum2 += sum1`
* per block: fold `sum1`, `sum2` into 16 bits
* returns `fold(sum1) | (fold(sum2) << 16)`

### What this means

**No public key. No RSA. No ECDSA. No signature anywhere in this path.** The
integrity check is a non-cryptographic additive checksum compared against a
value in a descriptor.

Fletcher-32 is trivially recomputable for any modified image, so **repacking is
feasible** — you patch the code, recompute Fletcher-32 over the new image, and
write the result back into the descriptor.

### Caveats, stated honestly

1. This routine verifies the image the **bootloader loads before jumping to it**.
   The **OTA/USB update path** may perform additional checks elsewhere. Not yet
   ruled out.
2. **The expected checksum is NOT in the firmware image.** `0xe060b100` occurs
   exactly **once** in the whole 10 MB image — at file offset `0xda08`, as a
   literal — and every reference to that slot is inside `fcn.00401000` itself.
   Nothing in the firmware writes it, maps it, or initialises it.

   It sits alongside `0xb700002c` and `[0xe0007000]+0x50`, both written by the
   same function. Those are MMIO registers, so `0xe060b100` is a
   **memory-mapped register block**, not a flash pointer. The descriptor
   (target, FlashAddr, size, expected checksum) is presented by
   hardware/ROM at runtime.

   **This is as far as static analysis goes.** Whether that block is a flash
   controller shadowing a header stored in patchable flash, or an OTP/eFuse
   window, cannot be determined from the image. It needs a runtime read of
   `0xe060b100` — i.e. UART or JTAG.
3. `+0x40000000` and `+0xb6000000` are applied to the descriptor fields, i.e. the
   descriptor holds offsets that are rebased at runtime.

### How it was finally read

rizin + `rz-ghidra`, which handles Thumb (REA/Ghidra cannot):

```sh
rizin -a arm -b 16 -m 0x400000 -A -q \
  -c 'axt @ 0x401114' -c 'pdg @ 0x401000' -c 'q' captures/s1.bin
```

`axt @ 0x401114` (the `checksum failed` string) led straight to `fcn.00401000`.

## 6b. Crypto primitives located (with a false-positive warning)

Found by constant, which is how you find crypto in a stripped image. **Filter by
region first**: section 2's `0x700000`-`0x940000` range is the embedded web
resource blob (HTML/JS/CSS), so byte patterns there are text, not code.

| Constant | Section | File offsets | Region | Trust |
|---|---|---|---|---|
| SHA-256 IV (`6a09e667`) | S1 | `0x15698` | code | good |
| SHA-256 IV | S2 | `0x2d393c` | code | good |
| SHA-256 IV | S2 | `0x65af04`, `0x65b3e4` | code | good |
| SHA-1/MD5 IV (`67452301`) | S1 | `0x15498` | code | good |
| SHA-1/MD5 IV | S2 | `0x231a24`, `0x23e4c0`, `0x241240`, `0x24b844`, `0x287d44` | code | good |
| RSA `e=65537` | S1 | `0x382c7` | code | good |
| RSA `e=65537` | S2 | `0x282777` | code | good |
| AES S-box | S2 | `0x78081c` | **resource blob** | **suspect** |
| RSA `e=65537` | S2 | `0x70bae7`, `0x70c1f9`, `0x71f6bd`, `0x722fd9` | **resource blob** | **suspect** |
| PKCS#1 v1.5 padding (`00 01 FF`) | S2 | `0x75a320`, `0x7fbb16`, `0x80405e`, `0x80407e`, `0x97a5bc` | **resource blob** | **FALSE POSITIVE** |

**Correction.** An earlier version of this document called
`0x80405e`/`0x80407e` "the strongest lead for signature verification". That was
wrong. Disassembly of `0x803f80`-`0x8040c0` shows plain JavaScript and HTML
(`resObj.js`, `resObj.valueOf`, `alert(msg)`, `if(msg=="1")`). The `00 01 FF`
bytes occur inside a web asset, not in crypto code. Every PKCS#1 hit above is in
that blob.

The code-region constants are still real leads: the SHA-256 IVs and the RSA
exponent at `0x282777` are in actual code. Start there instead.

## 6c. Leftover debug interfaces (the platform is BridgeCo)

The DMP platform is **BridgeCo AG** (`BridgeCo AG Telnet server`,
`bridgeco-jukeblox.net`), later SMSC/Microchip. KEF shipped the platform's debug
facilities intact, and the default config table lists them explicitly:

```
[ResourceManager
  [UartSwitch
    TelnetShellEnable
    TelnetShellPort = 8000
[CommunicationSettings
    Shell = UART1
    TelnetPort_Obsolete = 10000
    iPodControl = UART0
    HostController = SPI
    TCPIPTunnel = REMOTE_REGISTERS
    TCPIPTunnelPort = 50001
    EthDRC = ENABLE
    EthDRCPort = 10100
FirmwareIndexPage = http://www.bridgeco-jukeblox.net/download/index.xml
EmergencyFirmwareUrl
ProductPWD
EncryptionMandatory
FirmwareExt = fw
```

### Two network interfaces never tested

| Port | Config key | Observed on live device |
|---|---|---|
| **8000** | `TelnetShellPort` / `TelnetShellEnable` | **closed** (shell disabled) |
| **10100** | `EthDRCPort`, `EthDRC = ENABLE` | **OPEN, unauthenticated** |

Measured on the live device after Ethernet was connected (it needed ~1 minute
after link-up before 80 and 50001 came back):

```
80:open   50001:open   8000:closed   10100:OPEN   8080:open
```

### Port 10100 speaks display_tree XML

Connecting and reading yields a live UI state document, no auth:

```xml
<view id="browse">
  <text id="scrid">89</text>
  <text id="cbId">0</text>
  <text id="title">LSX 000000</text>
  <text id="line0" flag="dsE:4">Media Server</text>
  <icon id="hchyUp">empty</icon>
  <icon id="hchyDn">empty</icon>
</view><view id="status">
  <icon id="play">empty</icon>
  <icon id="shuffle">empty</icon>
  <icon id="repeat">empty</icon>
  <value id="busy" min="0" max="1">0</value>
</view>
```

`LSX 000000` matches the unit MAC (`aa:bb:cc:dd:ee:ff`). This is the
**display_tree 0.1** protocol from StreamUnlimited, which drives a front-panel
LCD. The LSX has no screen, so it is vestigial platform content being pushed to
nobody.

Inbound DRC commands are `newDRCCommand( %d 0x%08X )`, and the log line
immediately after it is `SPI Command Th...`, so commands are **SPI-tunnelled**
from a host MCU; EthDRC is that tunnel carried over TCP. Command names found:
`Send DRC Play/Play-Pause/Next/Previous/Stop/Allow/Prevent/Available`,
`DRC Reset Network`, `DRC Set JB Connect Mode`, `DRC Exit JB Connect Mode`.
Not probed: sending binary to an undocumented SPI tunnel risks crashing the
service for little gain, since 50001 already gives full control.

### The telnet shell is disabled by default

`TelnetShellEnable` occurs **exactly once** in the whole image, inside the
default config table. No code sets it at runtime. Hence 8000 closed.

**This is the actionable lead**: it is a CnE (config store) key. Anything that
can write arbitrary CnE keys can turn the shell on, and the shell has `rd`/`wr`
arbitrary memory access. Candidate writers are the web handlers that take a path
(`aformHandlerConfigureDMPName` emits `bad path(%s)`) and `bat` reads scripts
from `cne/Shell/`. Whether the path validation is weak enough to write
`TelnetShellEnable` is untested.

### Newly reachable web pages

Most legacy pages 404 (`/HardBoot.htm`, `/SoftBoot.htm`, `/FactoryDefaults.htm`,
`/Configuration.htm`, `/test.html`, `/1000/`, `/new/1000/success.asp`), but two
are served:

* **`/ui/ui.xml`** 200, 129,528 bytes: `<?xml version='1.0'?><gui xmlns='http://www.streamunlimited.com/display_tree/0.1'>`, the 320x240 LCD
  definition. Vestigial on a speaker with no display.
* **`/spotify.asp`** 200, 8,850 bytes.

### The shell has arbitrary memory access

Usage strings recovered from the image:

```
rd <addr> <count> [b|h]        read memory
wr <addr> <value> [q] [b|h]    write memory
reg add <address> <name> <size in bytes>
reg print <address>
bat <batch-name>               batch scripts from cne/Shell/, TAB-completion, depth 5
cd <dir> | pwd | get/set <attr>
nslookup <host> | iperf [-s|-c host] | probe <ip>
setip/setmc/remmc/setdnssvr <ip> [iface]
nc [play|stop|pause|stat|show]
upnp av dump | upnp av reader
lui [power | mode | cbname | toplist | busyloop <loadMs> <sleepMs>]
st trace on|off | prefLpcm on|off | clear
uartcfg <device> <nr>          device = uart0|uart1, nr = baudrate (e.g. 115200)
shell [-o|-i|-b|-s]            console settings only, NOT a network shell
```

`rd`/`wr` mean arbitrary memory read and write. `rd 0xe060b100` would answer the
descriptor question (section 6) in one command.

`bat` reads batch scripts from the **CnE config store** at `cne/Shell/`, the same
store the web UI writes to. That is a potential remote code execution path if any
web handler can write an arbitrary CnE path (`aformHandlerConfigureDMPName`
emits `bad path(%s)` diagnostics, so it does take a path).

### Two UARTs

`/dev/uart0` and `/dev/uart1`. Defaults: `Shell = UART1`, `iPodControl = UART0`.
So the debug shell is on **UART1**, and UART0 carries the Apple 30-pin accessory
protocol (`iPodShellCmd::`, `IPodAuthCoprocessorV2Impl`, `iPodSerialComm.cpp`).

### 50001 explained

`TCPIPTunnel = REMOTE_REGISTERS` with `TCPIPTunnelPort = 50001`. Our control
port is the platform's generic **remote-register tunnel**, which is why the
"registers" we mapped (`0x25` volume, `0x30` source) are platform-level remote
register IDs rather than device-specific ones.

### Also present

* `EthDRC` remote control on 10100, enabled by default.
* Legacy web UI pages that are not the ones we tested: `/HardBoot.htm`,
  `/SoftBoot.htm`, `/FactoryDefaults.htm`, `/Configuration.htm`, `/test.html`,
  `/config_*.html`, `/status_*.html`, `/ui/ui.xml`.
* `/diag-info`, `/info`, `/auth-setup`, `/command` are **AirPlay 2 RTSP
  endpoints**, not KEF debug endpoints.
* `test mode on/off`, `/cfg/Debug/DebugModule/Levels` (`[trace|debug]`).
* Firmware index default points at `bridgeco-jukeblox.net`, not `assets.kef.com`.

## 7. Where to poke next, ranked

1. **The XMODEM flash dump.** `Read flash out. Start xmodem now!` behind
   `Are you sure [y/n]?`. Reach it over UART and the dump is done. Highest value
   by a wide margin.
2. **The shell.** Find how `flashBurn` registers `fburn` and what else is
   registered. `aformHandlerTestHandler` may be an easier way in than UART.
3. **`aformHandlerTestHandler`** — unauthenticated, and a test handler.
4. **`Sirius/` config contents.** If `Certificate`/`Keyblob` are readable via the
   CnE store (the same store that leaks the WPA PSK over port 50001), we get the
   actual trust anchors without any disassembly.
5. **The USB update path** (`Firmware_update_prepare_for_usb.asp`,
   `aformHandlerObtainUsbBcdList`) — likely the least-hardened route.
6. **Section 2 code** around the flash/verify strings, using `tools/xref.py`
   pointed at section 2 with its own base.
7. **`Read flash out` / xmodem on the slave** via the `TO SLAVE` RJ45, if the
   slave has its own debug path.

## 8. Open structural questions

* Is section 1 a recovery partition or a product variant? (19% string overlap)
* Does section 2 use the same `0x400000` base, or is it relocated?
* What is the 32-byte tail (`0xff` × 30 + `76 74`)?
* Where do the four header hash words come from — over what range?
* The device ships *both* a bootloader web UI and an app web UI; which one answers
  on port 80 in normal operation, and does the other come up in recovery?
