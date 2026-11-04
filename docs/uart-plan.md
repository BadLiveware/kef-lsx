# UART access

Known facts about the debug console on the KEF LSX (gen 1). Measurements and
observations are from a single unit: master speaker, firmware system 2.7.0.
Provenance is noted per section.

## Streaming module

The network module is a **Microchip CY920-C** Network Media Module. Source: the
AudioScienceReview teardown, and the firmware identifies itself as
`LSX Microchip MCU Application V5.2.0`. The CY920 is built on the DM920
multi-core network media processor.

Its block diagram lists: Debug Port UART 1, JTAG, UART 0 (Bluetooth baseband),
SPI 0 and 1, I2C, 2x I2S, S/PDIF, 16 MB serial flash, 64 MB DDR2.

The module connects through a 200-pin SODIMM-style connector, plus an optional
extended connector on some SKUs.

## Console

| Setting | Value |
|---|---|
| Port | UART 1, labelled "Debug Port" |
| Baud | 230400 |
| Data, parity, stop | 8, none, 1 |
| Flow control | none |
| Prompt | `sds://>` |

Source: Microchip CY920 Getting Started Guide (DS50002260B). Figure 2-4 gives the
port settings, Figure 2-6 shows the prompt. The firmware config agrees:
`Shell = UART1`.

Boot output is verbose. The guide's example session contains repeated
`FATAL TRACE ERROR` lines carrying source file and line numbers, for example
`UIMgr/PlayModeHandler.cpp[1101]`, and ends with:

```
Wlan init
Disabling M3
Enabling M3
Starting IPOD DETECTION and IDPS
sds://>
```

The console is silent until the application is running; the guide states that the
bootloader executes first and the prompt appears once the application starts.

Pin locations are in the CY920 data sheet (DS60001270). That document was not
obtained, so the physical pin numbers are unknown.

## Shell commands

Usage strings recovered from the firmware image:

```
rd <addr> <count> [b|h]        read memory
wr <addr> <value> [q] [b|h]    write memory
reg add <address> <name> <size in bytes>
reg print <address>
bat <batch-name>               batch scripts from cne/Shell/, depth 5
cd <dir> | pwd | get/set <attr> <value>
nslookup <host> | iperf [-s|-c host] | probe <ip>
setip/setmc/remmc/setdnssvr <ip> [iface]
nc [play|stop|pause|stat|show]
upnp av dump | upnp av reader
lui [power | mode | cbname | toplist | busyloop <loadMs> <sleepMs>]
st trace on|off | prefLpcm on|off | clear
uartcfg <device> <nr>          device = uart0|uart1, nr = baudrate
shell [-o|-i|-b|-s]            console settings
```

`cd` and `set` against the config store are documented in the Microchip guide,
Example 3-1: `cd cne/Application/NetIF`, `ls -v`, `set CurrentMode wired`,
`netcfg`.

## MAMP board components

Source: AudioScienceReview teardown.

| Part | Role |
|---|---|
| Microchip CY920-C | network media module |
| Analog Devices ADAU1451WBCPZ | 32-bit DSP |
| ST STM32F030 | Cortex-M0 MCU |
| Cirrus Logic CS4272 | 24/192 stereo codec |
| TI PCM1754 | 24/192 stereo DAC |
| Cirrus/Wolfson WM8805 | S/PDIF transceiver |
| Qualcomm/CSR CSRA64215 | Bluetooth 4.2 BLE |
| TI CC8520 | PurePath 2.4 GHz link to the slave speaker |
| TI TPS5430, TPS54260 | buck converters |
| under the PCB | power amp, likely TI TPA32xx |

The MAMP board and a switch-mode PSU share the enclosure. The PSU uses Infineon
IPD60R360P7 (600 V) MOSFETs.

## Enclosure and retention

Observed on one unit. Photographed in [`images/teardown/`](images/teardown/).

Fasteners, in the order they become reachable:

1. Rear cosmetic panel, on 8 rubber posts press-fitted into holes.
2. About 6 screws around the perimeter of the heatsink and connector plate.
3. 4 screws under the rubber feet on the underside.
4. Screws recessed inside the black cylindrical standoffs on the plate, which
   clamp the back plate to the front baffle.

The heatsink plate and the connector module are one piece, not separable.

The standoff bores, measured: 19 mm at the opening, at least 11 mm guaranteed
clear, 110 mm deep. The heads are Phillips, not Torx.

## Known failure mode

Blinking red and orange indicates general failure and is not listed in the
manual. AudioScienceReview traced one instance to C433 (a 1206 ceramic, apparently
16 V rated) cracked, and RF choke L236/L238 burned out. That removes the +12 V
rail from U20 (LM29150-12), which is fed by a Q211/Z203 pre-regulator.

## Open questions

* Firmware integrity is a Fletcher-32 checksum, and the expected value is
  supplied at runtime from the descriptor at `0xe060b100` rather than stored in
  the image. Whether that descriptor is backed by flash or by OTP/eFuse is
  unknown and cannot be determined from the image. `rd 0xe060b100` would answer
  it. See [`firmware-map.md`](firmware-map.md).
* The OTA and USB update paths have not been examined for checks beyond the
  boot-time verify.
