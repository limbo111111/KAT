---
layout: default
---

# Star Line Protocol

**Rust module:** `src/protocols/star_line.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/star_line.c`

## Overview

Star Line uses PWM: 250 us = 0, 500 us = 1. 64 bits: key_fix (32) + key_hop (32), sent MSB-first (reversed on air). Header: 6 pairs of 1000 us HIGH + 1000 us LOW. KeeLoq: fix = serial(24) + button(8); hop encrypted with manufacturer key or normal-learning derived key.

## Timing

| Parameter | Value   | Notes   |
|-----------|---------|---------|
| Short (0) | 250 us  | Â±120 us |
| Long (1)  | 500 us  | Â±120 us |
| Header    | 1000 us Ã— 2 (6 pairs) | |
| Min bits  | 64      |         |

## Encoding

PWM; 64 bits MSB-first (reversed on air). KeeLoq encrypt for hop; fix half plain.

## Frame Layout (64 bits)

- **key_fix (32 bits):** serial (24) + button (8).
- **key_hop (32 bits):** KeeLoq-encrypted rolling code (MF key or normal-learning key).

## Decoder Steps

1. **Reset** â€” Wait for header (6 Ã— 1000 us HIGH, 1000 us LOW).
2. **CheckPreamble** â€” Confirm 6 header pairs.
3. **SaveDuration** â€” Store duration.
4. **CheckDuration** â€” Shortâ€“short = 0, longâ€“long = 1; at 64 bits KeeLoq-decrypt hop (or normal-learning), extract serial/button/counter, return.

## Encoder

Supported; header, 64-bit fix+hop (KeeLoq encrypt hop with MF or derived key).

## Frequencies

433.92 MHz.

## Keystore

Star Line manufacturer key (keystore type 20, `star_line_mf_key`). Used for KeeLoq hop decryption and normal-learning derivation.

