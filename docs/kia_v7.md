---
layout: default
---

# Kia V7 Protocol

**Rust module:** src/protocols/kia_v7.rs

## Overview

Kia V7 protocol decoder

Aligned with Flipper-ARF reference: `kia_v7.c`.

Protocol characteristics:
- 433.92 MHz FM
- 64 bits Manchester encoding
- te_short = 250 µs, te_long = 500 µs
- Preamble: >= 16 short pairs
- Sync: Long HI, Short LO. Implicitly adds 4 bits: 1, 0, 1, 1
- Payload inverted
- Custom CRC8 (poly 0x7F, init 0x4C)
- Fields: [16:47] serial, [8:23] counter, [48:51] button

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 250 us |
| Long       | 500 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
