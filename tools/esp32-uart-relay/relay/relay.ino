// ESP32-S3 UART relay for the KEF LSX debug console.
//
// Host side: UART0, which on most ESP32-S3 devkits is wired to an onboard
// CH343/CH9102 USB-UART bridge. The host therefore sees /dev/ttyACM0.
//
// Target side: UART1 on the pins below, connected to the CY920 "Debug Port
// UART 1" on the speaker.
//
// Wiring (3.3 V logic, common ground):
//   ESP32 GND        <-> speaker GND            (required)
//   ESP32 GPIO4 (RX) <-> speaker UART1 TX       (required to read the console)
//   ESP32 GPIO5 (TX) ->  speaker UART1 RX       (optional, needed to type)
//
// Do NOT connect the ESP32 3V3 to the speaker. The speaker is already powered.
//
// Note: this sketch prints nothing on purpose. UART0 is the data path to the
// host, so any Serial.print would corrupt the console stream.

#define SPK_RX_PIN 4
#define SPK_TX_PIN 5
#define LINK_BAUD  230400

void setup() {
  Serial.begin(LINK_BAUD);                                    // UART0 -> onboard bridge -> host
  Serial1.begin(LINK_BAUD, SERIAL_8N1, SPK_RX_PIN, SPK_TX_PIN); // UART1 -> speaker
}

void loop() {
  while (Serial.available())  Serial1.write(Serial.read());
  while (Serial1.available()) Serial.write(Serial1.read());
}
