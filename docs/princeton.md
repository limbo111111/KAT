---
layout: default
---

# Princeton Protocol

**Rust module:** src/protocols/princeton.rs

## Overview

Princeton protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/princeton.c`.

Protocol characteristics:
- 433.92 MHz AM, 24 bits
- TE ~390us short, 1170us long
- Bit 0: high for te, low for te*3
- Bit 1: high for te*3, low for te
- Preamble: low for te*36

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 390 us |
| Long       | 1170 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
