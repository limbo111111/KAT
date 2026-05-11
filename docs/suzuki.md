---
layout: default
---

# Suzuki Protocol

**Rust module:** `src/protocols/suzuki.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/suzuki.c`

## Overview

Suzuki uses PWM: 250 us HIGH = 0, 500 us HIGH = 1; LOW 250 us after each bit. 64 bits total. Preamble: 350 short HIGH / short LOW pairs; 2000 us gap at end. Field layout: serial = (data_high&0xFFF)<<16 | data_low>>16; btn = (data_low>>12)&0xF; cnt = (data_high<<4)>>16.

## Timing

| Parameter   | Value   | Notes   |
|------------|---------|---------|
| Short (0)  | 250 us  | Â±99 us  |
| Long (1)   | 500 us  | Â±99 us  |
| Preamble   | 350 pairs |       |
| Gap        | 2000 us | Â±399 us |
| Min bits   | 64      |         |

## Encoding

PWM: 250 us HIGH = 0, 500 us HIGH = 1; LOW 250 us after each bit.

## Frame Layout (64 bits)

- serial: (data_high & 0xFFF) << 16 | data_low >> 16  
- button: (data_low >> 12) & 0xF  
- counter: (data_high << 4) >> 16  

## Decoder Steps

1. **Reset** â€” Wait for short pulse (preamble).
2. **CountPreamble** â€” Count 350 short pairs; on 2000 us gap â†’ DecodeData.
3. **DecodeData** â€” Short = 0, long = 1; at 64 bits parse fields and return.

## Encoder

Supported; 350 preamble pairs, gap, 64 PWM bits.

## Frequencies

433.92 MHz.

