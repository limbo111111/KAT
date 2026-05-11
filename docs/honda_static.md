---
layout: default
---

# Honda Static Protocol

**Rust module:** src/protocols/honda_static.rs

## Overview

Honda Static protocol decoder

Aligned with Flipper-ARF/lib/subghz/protocols/honda_static.c

Protocol characteristics:
- 64-bit protocol (MIN_COUNT_BIT = 64)
- Manchester encoding
- Timing: Short duration 28-70 us (base 28, span 70 -> ~63us center), Long duration 61-130 us (base 61, span 130 -> ~126us center)
- Sync time: ~700 us
- Supported frequencies: 315 / 433 MHz

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 63 us |
| Long       | 126 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
