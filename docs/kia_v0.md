---
layout: default
---

# Kia V0 Protocol

**Rust module:** `src/protocols/kia_v0.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/kia_v0.c`

## Overview

Kia V0 is a PWM keyfob protocol: short pulse = 0, long pulse = 1. No Manchester encoding. 61 bits per frame (1 sync bit + 60 data bits). CRC8 over bits 8â€“55; polynomial 0x7F, init 0x00.

## Timing

| Parameter   | Value  | Notes              |
|------------|--------|--------------------|
| Short (0)  | 250 us | Â±100 us (te_delta) |
| Long (1)   | 500 us | Â±100 us            |
| Min bits   | 61     |                    |

## Frame Layout (61 bits)

- **Sync:** 1 bit (the long-long pattern also counts as first data bit = 1).
- **Data:** 60 bits MSB first:
  - Bits 56â€“59: 4-bit prefix (often preserved from capture).
  - Bits 40â€“55: 16-bit counter.
  - Bits 12â€“39: 28-bit serial.
  - Bits 8â€“11: 4-bit button.
  - Bits 0â€“7: 8-bit CRC (over bits 8â€“55, 6 bytes).

## Decoder Steps

1. **Reset** â€” Wait for short HIGH (250 us Â±100).
2. **CheckPreamble** â€” Count preamble: alternating short pulses; on LOW, if shortâ€“short pair then `header_count++`; if longâ€“long and `header_count > 15` â†’ go to SaveDuration, add first bit 1.
3. **SaveDuration** â€” On HIGH: if duration â‰¥ 500 + 200 us (end marker), check `decode_count_bit == 61` and return decode; else store duration and go to CheckDuration. On LOW â†’ Reset.
4. **CheckDuration** â€” On LOW: shortâ€“short â†’ add bit 0, back to SaveDuration; longâ€“long â†’ add bit 1, back to SaveDuration; else Reset.

## Encoder

- 2 bursts; inter-burst gap 25â€¯000 us.
- 32 alternating short (250 us) preamble pairs.
- Sync: long HIGH, long LOW (500 us each).
- Data: 59 bits sent (mask 1ULL << (58 - bit_num)), i.e. bits 58 down to 0 (per reference).
- End marker: long Ã— 2 (1000 us HIGH).

## Frequencies

433.92 MHz.

