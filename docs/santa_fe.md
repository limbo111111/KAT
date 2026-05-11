---
layout: default
---

# Santa Fe Protocol

**Rust module:** src/protocols/santa_fe.rs

## Overview

Hyundai Santa Fe 2013-2016 protocol decoder

Aligned with Flipper-ARF reference: `auto_rke_protocols.c`.

Protocol characteristics:
- 433.92 MHz AM/OOK
- 80 bits (MSB first)
- PWM: period 500 µs; 1 = 375 µs HI + 125 µs LO; 0 = 125 µs HI + 375 µs LO
- Sync: 375 µs HI + 12000 µs LO
- Gap: 15000 µs
- Fields: [79:48] rolling, [47:24] 24-bit serial, [23:16] counter, [15:8] button, [7:0] CRC8
- CRC8 poly 0x31, init 0xFF

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 125 us |
| Long       | 375 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
