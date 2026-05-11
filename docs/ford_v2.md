---
layout: default
---

# Ford V2 Protocol

**Rust module:** src/protocols/ford_v2.rs

## Overview

Ford V2 protocol decoder

Aligned with Flipper-ARF reference: `ford_v2.c`.

Protocol characteristics:
- 433.92 MHz FM
- 104 bits (13 bytes) Manchester encoding
- te_short = 200 µs, te_long = 400 µs
- Sync word: 0x7F, 0xA7
- Fields: [16:47] 32-bit serial, [48:55] button, [56:71] counter (16 bit), tail
- Buttons: 0x10=Lock, 0x11=Unlock, 0x12=Trunk, 0x14=Panic, 0x15=RemoteStart

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 200 us |
| Long       | 400 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
