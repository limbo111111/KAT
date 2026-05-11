---
layout: default
---

# Kia V2 Protocol

**Rust module:** `src/protocols/kia_v2.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/kia_v2.c`

## Overview

Kia V2 uses Manchester encoding at 500/1000 us. 53 bits: 32 serial + 4 button + 12 counter + 4 CRC, plus start bit. Long preamble of 252 long pairs. CRC4 (XOR nibbles + offset 1).

## Timing

| Parameter | Value   | Notes   |
|-----------|---------|---------|
| Short     | 500 us  | Â±150 us |
| Long      | 1000 us | Â±150 us |
| Min bits  | 53      |         |

## Encoding

Manchester encoding; bit value from short/long and transition.

## Frame Layout (53 bits)

- Start bit + 32 serial + 4 button + 12 counter (byte-swapped) + 4 CRC.

## Decoder Steps

1. **Reset** â€” Wait for preamble (long pulses).
2. **CheckPreamble** â€” Count 252 long pairs.
3. **CollectRawBits** â€” Manchester decode; at 53 bits validate CRC4 and return.

## Encoder

Supported; preamble and Manchester 53-bit frame.

## Frequencies

433.92 MHz.

