---
layout: default
---

# Hyundai Kia Rio Protocol

**Rust module:** src/protocols/hyundai_kia_rio.rs

## Overview

Hyundai / Kia RIO protocol decoder

Aligned with Flipper-ARF reference: `auto_rke_protocols.c`.

Protocol characteristics:
- 433.92 MHz AM/OOK
- 64 bits (MSB first)
- PWM: period 1040 µs; 1 = 728 µs HI + 312 µs LO; 0 = 312 µs HI + 728 µs LO
- Sync: 312 µs HI + 10400 µs LO
- Gap: 10000 µs
- Fields: [63:32] 32-bit serial, [31:16] 16-bit button mask, [15:0] 16-bit checksum
- Button codes: 0x0100=Lock, 0x0200=Unlock, 0x0400=Trunk, 0x0800=Panic

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 312 us |
| Long       | 728 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
