# ESP32-S3 UART relay

Fallback for a dedicated USB-UART adapter. An ESP32-S3 devkit becomes a
3.3 V USB-to-UART bridge for the CY920 debug console at **230400 8N1**.

Electrically this is a good fit: ESP32-S3 GPIO is natively 3.3 V, so there is no
5 V jumper to get wrong. The cost is firmware and toolchain rather than money.

## Why UART0 and not USB CDC

Most ESP32-S3 devkits expose two USB ports. The one labelled `COM` or `UART`
goes to an onboard **CH343 or CH9102** bridge chip, which enumerates as
`1a86:55d3 QinHeng Electronics USB Single Serial` and is wired to the ESP32's
**UART0** (GPIO43/44). That is the port used for flashing.

So the relay sits on UART0, and `Serial` in Arduino maps to UART0 when
**USB CDC On Boot** is left at default. If you instead plug into the native USB
port, the device enumerates as `303a:xxxx` (Espressif) and you would need the
USB CDC variant of this sketch.

## Wiring

| ESP32-S3 | Speaker |
|---|---|
| GND | GND (required) |
| GPIO4 (RX) | UART1 TX (required) |
| GPIO5 (TX) | UART1 RX (optional, for typing) |

Never connect 3V3 to the speaker.

## Flash

```sh
sudo pacman -S arduino-cli
arduino-cli config init
arduino-cli config add board_manager.additional_urls \
  https://espressif.github.io/arduino-esp32/package_esp32_index.json
arduino-cli core update-index
arduino-cli core install esp32:esp32

# N16R8 = 16 MB flash, 8 MB octal PSRAM
arduino-cli compile --fqbn 'esp32:esp32:esp32s3:FlashSize=16M,PSRAM=opi' \
  tools/esp32-uart-relay
arduino-cli upload -p /dev/ttyACM0 \
  --fqbn 'esp32:esp32:esp32s3:FlashSize=16M,PSRAM=opi' tools/esp32-uart-relay
```

If the board does not enter the bootloader automatically, hold **BOOT**, tap
**RESET**, then release **BOOT**, and retry the upload.

If the FQBN menu keys differ in your core version, list them with:

```sh
arduino-cli board details -b esp32:esp32:esp32s3
```

## Access

`/dev/ttyACM0` is usually `root:uucp` mode 0660. Add yourself to the group and
re-login:

```sh
sudo usermod -aG uucp "$USER"
```

## Verify before trusting it

Short GPIO5 to GPIO4 and type in `picocom -b 230400 /dev/ttyACM0`. Echo back
means the relay and host side are good, and any remaining problem is on the
speaker side.

## Caveats

* The ESP32 ROM bootloader prints its banner on UART0 at 115200, so you get one
  line of garbage at reset before the relay starts. Cosmetic.
* Do not add `Serial.print` calls. UART0 is the data path.
* The toolchain download is around a gigabyte, which is a lot more than a
  USB-UART cable costs. Worth it only if you want the ESP32 anyway, or want to
  reuse it as a multi-channel sniffer for pin identification.
