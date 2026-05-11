---
layout: default
---

# Ford V0 Protocol

**Rust module:** `src/protocols/ford_v0.rs`  
**Reference:** `REFERENCES/ProtoPirate/protocols/ford_v0.c`

## Overview

Ford V0 uses Manchester encoding at 250/500 us. 80 bits total: 64-bit key1 + 16-bit key2. CRC is computed via GF(2) matrix multiplication (CRC matrix in code). BS (byte) and â€œBS magicâ€ are used for encoding and validation. 6 bursts; 4 preamble pairs per burst; 3500 us gap before data. Flipper-style Manchester: level true â†’ ShortLow/LongLow, level false â†’ ShortHigh/LongHigh.

## Timing

| Parameter   | Value  | Notes              |
|------------|--------|--------------------|
| Short      | 250 us | Â±100 us (te_delta) |
| Long       | 500 us | Â±100 us            |
| Gap        | 3500 us| Â±250 us            |
| Min bits   | 64     | (key1); 80 total   |

## Encoding

Manchester: short/low, short/high, long/low, long/high map to events 0â€“3; state machine emits data bits. First bit after gap is implicit 1; then 79 more bits from Manchester (64 for key1, 16 for key2). key1/key2 sent inverted (~key1, ~key2).

## Frame Layout (80 bits)

- **key1 (64 bits):** header byte, serial, button, counter, XOR/parity and mixed nibbles (see decode_ford_v0 in code).
- **key2 (16 bits):** BS (high byte), CRC (low byte) XOR 0x80.

CRC is matrix-based over key1 bytes and BS byte; received CRC is key2 low byte XOR 0x80.

## Decoder Steps

1. **Reset** â€” Short HIGH (250 us) or long HIGH (500 us) â†’ Preamble (allows re-sync mid-preamble).
2. **Preamble** â€” LOW long (500 us) â†’ PreambleCheck.
3. **PreambleCheck** â€” HIGH long â†’ header_count++, back to Preamble; HIGH short â†’ Gap.
4. **Gap** â€” LOW ~3500 us (Â±250) â†’ Data, set first bit to 1, bit_count=1.
5. **Data** â€” Manchester events (short/long Ã— level); add_bit (two 64-bit registers, C-style); at 64 bits form key1 = ~combined, clear; at 80 bits key2 = ~low 16, decode_ford_v0, verify CRC, return.

## Encoder

Supported. Builds 6 bursts: short-long, 4 preamble pairs, short-long, gap, then Manchester 80 bits (key1 then key2, inverted). Inter-burst gap longÃ—100.

## Frequencies

315 MHz, 433.92 MHz.

