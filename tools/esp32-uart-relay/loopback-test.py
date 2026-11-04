#!/usr/bin/env python3
"""Byte-integrity and throughput test for the ESP32-S3 UART relay.

Short the relay's TX pin to its RX pin (GPIO5 to GPIO4 by default), then run
this. It writes a known pattern and checks that exactly the same bytes come
back, which proves both directions of the relay and that 230400 baud does not
drop data under a boot-log-sized burst.

    ./loopback-test.py /dev/ttyACM0 --bytes 200000
"""
import argparse
import os
import select
import sys
import time


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("port", nargs="?", default="/dev/ttyACM0")
    ap.add_argument("--bytes", type=int, default=200_000,
                    help="payload size, default 200 kB which is a generous boot log")
    ap.add_argument("--chunk", type=int, default=256)
    ap.add_argument("--timeout", type=float, default=10.0)
    args = ap.parse_args()

    try:
        fd = os.open(args.port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except OSError as exc:
        print(f"cannot open {args.port}: {exc}", file=sys.stderr)
        return 1

    payload = bytes((i * 7 + 11) & 0xFF for i in range(args.bytes))
    received = bytearray()

    started = time.monotonic()
    sent = 0
    while sent < len(payload):
        block = payload[sent:sent + args.chunk]
        try:
            sent += os.write(fd, block)
        except BlockingIOError:
            select.select([], [fd], [], 0.05)

    deadline = started + args.timeout
    while len(received) < len(payload) and time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.2)
        if not ready:
            continue
        try:
            received += os.read(fd, 65536)
        except BlockingIOError:
            pass

    elapsed = time.monotonic() - started
    os.close(fd)

    got = bytes(received)
    print(f"sent     {len(payload)} bytes")
    print(f"received {len(got)} bytes in {elapsed:.2f}s "
          f"({len(got) / elapsed / 1024:.1f} kB/s)")
    if got == payload:
        print("RESULT   byte-perfect round trip")
        return 0
    if len(got) < len(payload):
        print(f"RESULT   truncated, missing {len(payload) - len(got)} bytes")
    else:
        first = next((i for i, (a, b) in enumerate(zip(payload, got)) if a != b), None)
        print(f"RESULT   corrupted, first difference at offset {first}")
    return 2


if __name__ == "__main__":
    sys.exit(main())
