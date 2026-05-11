---
layout: default
---

# Sec Plus V2 Protocol

**Rust module:** src/protocols/secplus_v2.rs

## Overview

SecPlus_v2 protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/secplus_v2.c`.

Protocol characteristics:
- 315 MHz AM, 62 bits
- TE ~250us short, 500us long
- Manchester Encoding (ShortLow, LongLow, ShortHigh, LongHigh)
- 2 packets combined into one DecodedSignal via mix_invert and mix_order logic

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 250 us |
| Long       | 500 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
