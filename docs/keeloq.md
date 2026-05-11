---
layout: default
---

# Keeloq Protocol

**Rust module:** src/protocols/keeloq.rs

## Overview

KeeLoq protocol decoder and encoder (unleashed format).

Timing and state machine match Flipper Unleashed:
`REFERENCES/unleashed-firmware/lib/subghz/protocols/keeloq.c`
- te_short=400µs, te_long=800µs, te_delta=140µs, 64 data bits.
- Preamble: HIGH pulses ~400µs; when LOW and header_count>2 and LOW ~4000µs, start data.
- Data: short HIGH + long LOW = 1, long HIGH + short LOW = 0. End when LOW ≥ 940µs.
Decryption tries all keystore keys with simple, normal, secure, magic_xor_type1,
magic_serial type1/2/3 (and both key byte orders). Encoder uses simple learning
and the key stored in DecodedSignal::extra (set when decoded).

## Timing

| Parameter   | Value  |
|------------|--------|
| Short      | 400 us |
| Long       | 800 us |
| Min bits   | Unknown     |

## Decoding & Format

*(Detailed structure and decoding logic to be added)*
