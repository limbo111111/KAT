---
layout: default
---

# Roger Protocol

**Rust module:** src/protocols/roger.rs

## Overview

Roger protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/roger.c`.

Protocol characteristics:
- 433.92 MHz AM, 28 bits
- TE ~500us short, 1000us long
- Bit 0: high for te_short, low for te_long
- Bit 1: high for te_long, low for te_short
- GAP: low for te_short * 19

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 500 us |
| Long       | 1000 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
