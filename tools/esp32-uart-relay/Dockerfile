# Build and flash the ESP32-S3 UART relay without touching the host toolchain.
# Modelled on the toolchain/device split used by ~/code/personal/laptop-cooler.
FROM debian:bookworm-slim

ARG ARDUINO_CLI_VERSION=1.5.1

RUN apt-get update \
    && apt-get install --no-install-recommends --yes \
        ca-certificates \
        curl \
        git \
        python3 \
        python3-serial \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://raw.githubusercontent.com/arduino/arduino-cli/master/install.sh \
        | BINDIR=/usr/local/bin sh -s "${ARDUINO_CLI_VERSION}"

ENV ARDUINO_DIRECTORIES_DATA=/opt/arduino/data \
    ARDUINO_DIRECTORIES_DOWNLOADS=/opt/arduino/staging \
    ARDUINO_DIRECTORIES_USER=/opt/arduino/user

RUN arduino-cli config init \
    && arduino-cli config add board_manager.additional_urls \
         https://espressif.github.io/arduino-esp32/package_esp32_index.json \
    && arduino-cli core update-index \
    && arduino-cli core install esp32:esp32 \
    && chmod -R a+rwX /opt/arduino

WORKDIR /work
