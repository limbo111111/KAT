---
layout: default
---

# Psa2 Protocol

**Rust module:** src/protocols/psa2.rs

## Overview

PSA2 protocol decoder/encoder

Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/psa2.c`.

Protocol characteristics:
- Manchester encoding: 250/500µs or 125/250µs symbols
- 128 bits total: key1 (64) + validation (16) + key2/rest (48)
- Modified TEA (XTEA-like) with dynamic key selection (sum&3, (sum>>11)&3)
- Mode 0x23: direct XOR decrypt with checksum validation
- Mode 0x36: TEA brute-force with BF1/BF2 key schedules (deferred)

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | Unknown us |
| Long       | Unknown us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
