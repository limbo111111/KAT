# Flipper-ARF to KAT Protocol Porting Tasks

This document contains 10 separate batches of C protocol decoders from the `Flipper-ARF/lib/subghz/protocols/` directory that need to be ported into native Rust for the `KAT` application (`src/protocols/`).

**Instructions for the AI Agent:**
1. The user will assign you a "Worker ID" (from 1 to 10).
2. Look at the section below that matches your Worker ID.
3. For each protocol in your batch:
   - Clone or reference the Flipper-ARF repo (`https://github.com/limbo111111/Flipper-ARF.git`) if you don't already have it.
   - Read the `.c` and `.h` files for the protocol.
   - Replicate the logic natively in Rust by creating a new file in `src/protocols/` (e.g., `src/protocols/honda_static.rs`).
   - Implement the `ProtocolDecoder` trait (see existing files like `mazda_v0.rs` or `ford_v1.rs` for reference).
   - If the C code has an encoder, implement the `encode()` function in Rust. If not, return `None`.
   - Register the new decoder in `src/protocols/mod.rs` (add the `mod` and add it to `ProtocolRegistry::new()`).
4. **Before committing**, ensure `cargo check` and `cargo test` pass. Ensure you have installed the `libhackrf-dev` dependency (`sudo apt-get install -y libhackrf-dev`).
5. Remove any temporary patch or bash files you created before submitting.

---

## Worker 1
- `alutech_at_4n.c`
- `ansonic.c`
- `beninca_arc.c`
- `bett.c`
- `bin_raw.c`
- `bmw_cas4.c`
- `came.c`
- `came_atomo.c`
- `came_twee.c`
- `chamberlain_code.c`

## Worker 2
- `chrysler.c`
- `clemsa.c`
- `dickert_mahs.c`
- `doitrand.c`
- `dooya.c`
- `elplast.c`
- `faac_slh.c`
- `feron.c`
- `fiat_marelli.c`
- `fiat_spa.c`

## Worker 3
- `gangqi.c`
- `gate_tx.c`
- `hay21.c`
- `hollarm.c`
- `holtek.c`
- `holtek_ht12x.c`
- `honda_static.c`
- `honeywell.c`
- `honeywell_wdb.c`
- `hormann.c`

## Worker 4
- `ido.c`
- `intertechno_v3.c`
- `jarolift.c`
- `keeloq_common.c` (Note: KAT already has a keeloq module, but check if there are missing variants from Flipper-ARF)
- `keyfinder.c`
- `kia_v0.c` (Verify if updates from Flipper-ARF are needed, KAT already has kia_v0)
- `kia_v1.c` (Verify updates)
- `kia_v2.c` (Verify updates)
- `kia_v3_v4.c` (Verify updates)
- `kia_v5.c` (Verify updates)

## Worker 5
- `kia_v6.c` (Verify updates)
- `kinggates_stylo_4k.c`
- `landrover_rke.c`
- `legrand.c`
- `linear.c`
- `linear_delta3.c`
- `magellan.c`
- `marantec.c`
- `marantec24.c`
- `mastercode.c`

## Worker 6
- `mazda_siemens.c`
- `megacode.c`
- `mitsubishi_v0.c` (Verify updates)
- `nero_radio.c`
- `nero_sketch.c`
- `nice_flo.c`
- `nice_flor_s.c`
- `phoenix_v2.c`
- `porsche_cayenne.c`
- `power_smart.c`

## Worker 7
- `princeton.c`
- `protocol_items.c` (Check if there's any generic structure to port, or ignore if irrelevant to KAT's architecture)
- `psa.c` (Verify updates)
- `psa2.c`
- `raw.c`
- `revers_rb2.c`
- `roger.c`
- `scher_khan.c` (Verify updates)
- `secplus_v1.c`
- `secplus_v2.c`

## Worker 8
- `sheriff_cfm.c`
- `smc5326.c`
- `somfy_keytis.c`
- `somfy_telis.c`
- `star_line.c` (Verify updates)
- `subaru.c` (Verify updates)
- `suzuki.c` (Verify updates)
- `treadmill37.c`
- `vag.c` (Verify updates)
- (Any other unlisted protocols from `Flipper-ARF/lib/subghz/protocols/`)

## Worker 9
*Reserved for complex protocols or testing edge cases (e.g. multi-packet generic decoding rules, new encoder types).*

## Worker 10
*Reserved for final integration testing, verifying `ProtocolRegistry` completeness, cleaning up duplicate/deprecated code, and polishing the TUI interface to ensure all new protocols render correctly.*
