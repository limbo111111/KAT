---
layout: default
---

# Subaru Protocol

**Rust module:** `src/protocols/subaru.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/subaru.c`

## Overview

Subaru uses PWM: 800 us HIGH = 1, 1600 us HIGH = 0; LOW is 800 us after each bit. 64 bits total (8 bytes MSB first: button(4) + serial(24) + counter-related). Preamble: 79 full 1600 us pairs + 80th HIGH only; then gap 2800 us, sync 2800 us HIGH + 1600 us LOW. Complex counter decoding from bytes 4â€“7.

## Timing

| Parameter | Value   | Notes   |
|-----------|---------|---------|
| Short (1) | 800 us  | Â±200 us |
| Long (0)  | 1600 us | Â±200 us |
| Gap       | 2800 us |         |
| Sync      | 2800 us HIGH + 1600 us LOW | |
| Min bits  | 64      |         |

## Encoding

PWM: duration of HIGH = bit value (800 = 1, 1600 = 0); LOW 800 us after each bit.

## Frame Layout (64 bits = 8 bytes)

- 4 bits button, 24 bits serial, counter-related in bytes 4â€“7 (decode_counter in code).

## Decoder Steps

1. **Reset** â€” Wait for long HIGH (preamble).
2. **CheckPreamble** â€” Count 79 long pairs + 80th HIGH; then expect gap 2800 us.
3. **FoundGap** â€” Gap seen â†’ look for sync (2800 HIGH, 1600 LOW).
4. **FoundSync** â€” Sync seen â†’ SaveDuration.
5. **SaveDuration** â€” On HIGH: store duration, â†’ CheckDuration. On end marker â†’ return decode if 64 bits.
6. **CheckDuration** â€” On LOW: pair (te_last, duration) â†’ shortâ€“short = 1, longâ€“long = 0; add bit, â†’ SaveDuration.

## Encoder

Supported; builds preamble, gap, sync, then 64 PWM bits.

## Frequencies

433.92 MHz (and/or 315 MHz as in reference).

