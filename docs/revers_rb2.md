---
layout: default
---

# Revers Rb2 Protocol

**Rust module:** src/protocols/revers_rb2.rs

## Overview

Revers RB2 protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/revers_rb2.c`.

Protocol characteristics:
- 433.92 MHz AM, 64 bits
- TE ~250us short, 500us long
- Manchester Encoding (ShortLow, LongLow, ShortHigh, LongHigh)
- Wait for GAP < 600, wait for 4 Header events, extract bits, check 0xFF and 0x200 markers

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 250 us |
| Long       | 500 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
