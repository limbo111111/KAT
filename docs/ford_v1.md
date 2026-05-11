---
layout: default
---

# Ford V1 Protocol

**Rust module:** src/protocols/ford_v1.rs

## Overview

Ford V1 protocol decoder

Aligned with Flipper-ARF reference: `ford_v1.c`.

Protocol characteristics:
- 433.92 MHz FM
- 136 bits (17 bytes) Manchester encoding
- te_short = 65 µs, te_long = 130 µs
- Preamble: >= 50 long pulses
- CRC16 (poly 0x1021) on bytes 3..15

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 65 us |
| Long       | 130 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
