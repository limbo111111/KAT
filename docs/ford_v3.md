---
layout: default
---

# Ford V3 Protocol

**Rust module:** src/protocols/ford_v3.rs

## Overview

Ford V3 protocol decoder

Aligned with Flipper-ARF reference: `ford_v3.c`.

Protocol characteristics:
- 433.92 MHz FM
- 136 bits (17 bytes) Manchester encoding
- te_short = 200 µs, te_long = 400 µs
- Sync word: 0x7F, 0xA7
- CRC16 (poly 0x1021) on bytes 3..15
- Fields: [32:55] 24-bit serial
- Crypt chunk: bytes 7..14 (8 bytes). `btn` at crypt[4], `counter` at crypt[5..6].

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 200 us |
| Long       | 400 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
