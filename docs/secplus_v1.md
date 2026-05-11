---
layout: default
---

# Sec Plus V1 Protocol

**Rust module:** src/protocols/secplus_v1.rs

## Overview

SecPlus_v1 protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/secplus_v1.c`.

Protocol characteristics:
- 315 MHz AM, 42 bits (2 packets of 21 digits in base-3)
- TE ~500us short, 1500us long
- Bit 0: low for te*3, high for te
- Bit 1: low for te*2, high for te*2
- Bit 2: low for te, high for te*3

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 500 us |
| Long       | 1500 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
