//! PSA2 protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/psa2.c`.
//!
//! Protocol characteristics:
//! - Manchester encoding: 250/500µs or 125/250µs symbols
//! - 128 bits total: key1 (64) + validation (16) + key2/rest (48)
//! - Modified TEA (XTEA-like) with dynamic key selection (sum&3, (sum>>11)&3)
//! - Mode 0x23: direct XOR decrypt with checksum validation
//! - Mode 0x36: TEA brute-force with BF1/BF2 key schedules (deferred)

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT_STD: u32 = 250;
const TE_LONG_STD: u32 = 500;
const TE_SHORT_HALF: u32 = 125;
const TE_LONG_HALF: u32 = 250;

const TE_DELTA: u32 = 100;
const TE_DELTA_HALF: u32 = 50;
const MIN_COUNT_BIT: usize = 128;

// Internal timing for Manchester sub-symbol detection
const TE_END_1000: u32 = 1000;
const TE_END_500: u32 = 500;

// TEA constants
const TEA_DELTA: u32 = 0x9E3779B9;
const TEA_ROUNDS: u32 = 32;

// Brute-force constants for mode 0x23
const BF1_KEY_SCHEDULE: [u32; 4] = [0x4A434915, 0xD6743C2B, 0x1F29D308, 0xE6B79A64];

// Brute-force constants for mode 0x36
const BF2_KEY_SCHEDULE: [u32; 4] = [0x4039C240, 0xEDA92CAB, 0x4306C02A, 0x02192A04];

/// Manchester decoder states (matches protopirate psa.c Manchester state machine)
#[derive(Debug, Clone, Copy, PartialEq)]
enum ManchesterState {
    Mid0,
    Mid1,
    Start0,
    Start1,
}

/// Decoder states (matches protopirate's PsaDecoderState 0-4)
#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderState {
    /// State0: Wait for first edge (pattern detection)
    WaitEdge,
    /// State1: Count 250µs preamble patterns (Pattern 1)
    CountPattern250,
    /// State2: Manchester decode at 250/500µs (Pattern 1)
    DecodeManchester250,
    /// State3: Count 125µs preamble patterns (Pattern 2)
    CountPattern125,
    /// State4: Manchester decode at 125/250µs (Pattern 2)
    DecodeManchester125,
}

/// PSA2 protocol decoder
pub struct Psa2Decoder {
    state: DecoderState,
    prev_duration: u32,
    manchester_state: ManchesterState,
    pattern_counter: u16,
    data_low: u32,
    data_high: u32,
    bit_count: u8,
    // Decoded fields
    key1_low: u32,
    key1_high: u32,
    validation_field: u16,
    key2_low: u32,
    key2_high: u32,
    mode: u8, // 0x23 or 0x36
}

impl Psa2Decoder {
    pub fn new() -> Self {
        Self {
            state: DecoderState::WaitEdge,
            prev_duration: 0,
            manchester_state: ManchesterState::Mid1,
            pattern_counter: 0,
            data_low: 0,
            data_high: 0,
            bit_count: 0,
            key1_low: 0,
            key1_high: 0,
            validation_field: 0,
            key2_low: 0,
            key2_high: 0,
            mode: 0,
        }
    }

    /// Manchester state machine (matches psa.c event mapping)
    fn manchester_advance(&mut self, is_short: bool, is_high: bool) -> Option<bool> {
        let is_long = !is_short;
        match self.manchester_state {
            ManchesterState::Mid1 => {
                if is_short && !is_high {
                    self.manchester_state = ManchesterState::Start1;
                    None
                } else if is_long && is_high {
                    self.manchester_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Mid0 => {
                if is_short && is_high {
                    self.manchester_state = ManchesterState::Start0;
                    None
                } else if is_long && !is_high {
                    self.manchester_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start1 => {
                if is_short && is_high {
                    self.manchester_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start0 => {
                if is_short && !is_high {
                    self.manchester_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_state = ManchesterState::Mid1;
                    None
                }
            }
        }
    }

    fn init_preamble_state(&mut self) {
        self.data_low = 0;
        self.data_high = 0;
        self.bit_count = 0;
        self.pattern_counter = 0;
        self.manchester_state = ManchesterState::Mid1;
    }

    fn add_bit(&mut self, bit: bool) {
        let carry = (self.data_low >> 31) & 1;
        self.data_low = (self.data_low << 1) | (bit as u32);
        self.data_high = (self.data_high << 1) | carry;
        self.bit_count += 1;

        if self.bit_count == 64 {
            self.key1_low = self.data_low;
            self.key1_high = self.data_high;
            self.data_low = 0;
            self.data_high = 0;
        }
    }

    fn finalize_frame(&mut self) -> Option<DecodedSignal> {
        if self.bit_count < 80 {
            self.state = DecoderState::WaitEdge;
            return None;
        }

        self.validation_field = (self.data_low & 0xFFFF) as u16;
        self.key2_low = self.data_low;
        self.key2_high = self.data_high;

        self.state = DecoderState::WaitEdge;

        let data = ((self.key1_high as u64) << 32) | (self.key1_low as u64);

        Some(DecodedSignal {
            serial: None,
            button: None,
            counter: None,
            crc_valid: false,
            data,
            data_count_bit: 128,
            encoder_capable: true,
            extra: None,
            protocol_display_name: None,
        })
    }

    fn tea_encrypt(v0: &mut u32, v1: &mut u32, key: &[u32; 4]) {
        let mut a = *v0;
        let mut b = *v1;
        let mut sum: u32 = 0;
        for _ in 0..TEA_ROUNDS {
            let idx1 = (sum & 3) as usize;
            let t1 = key[idx1].wrapping_add(sum);
            sum = sum.wrapping_add(TEA_DELTA);
            a = a.wrapping_add(t1 ^ (((b >> 5) ^ (b << 4)).wrapping_add(b)));

            let idx2 = ((sum >> 11) & 3) as usize;
            let t2 = key[idx2].wrapping_add(sum);
            b = b.wrapping_add(t2 ^ (((a >> 5) ^ (a << 4)).wrapping_add(a)));
        }
        *v0 = a;
        *v1 = b;
    }

    fn xor_encrypt(buf: &mut [u8; 10]) {
        let mut t = [0u8; 8];
        // psa_copy_reverse logic in C
        t[0] = buf[5];
        t[1] = buf[4];
        t[2] = buf[3];
        t[3] = buf[2];
        t[4] = buf[9];
        t[5] = buf[8];
        t[6] = buf[7];
        t[7] = buf[6];

        // psa_second_stage_xor_encrypt logic in C
        let p0 = t[0];
        let p1 = t[1];
        let p2 = t[2];
        let p3 = t[3];
        let p4 = t[4];
        let p5 = t[5];
        let e6 = buf[8];
        let e7 = buf[9];

        let e5 = p5 ^ e7 ^ e6;
        let e0 = p2 ^ e5;
        let e2 = p4 ^ e0;
        let e4 = p3 ^ e2;
        let e3 = p0 ^ e5;
        let e1 = p1 ^ e3;

        buf[2] = e0;
        buf[3] = e1;
        buf[4] = e2;
        buf[5] = e3;
        buf[6] = e4;
        buf[7] = e5;
    }
}

impl ProtocolDecoder for Psa2Decoder {
    fn name(&self) -> &'static str {
        "PSA2"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT_STD,
            te_long: TE_LONG_STD,
            te_delta: TE_DELTA,
            min_count_bit: MIN_COUNT_BIT,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000]
    }

    fn reset(&mut self) {
        self.state = DecoderState::WaitEdge;
        self.prev_duration = 0;
        self.manchester_state = ManchesterState::Mid1;
        self.pattern_counter = 0;
        self.data_low = 0;
        self.data_high = 0;
        self.bit_count = 0;
        self.key1_low = 0;
        self.key1_high = 0;
        self.validation_field = 0;
        self.key2_low = 0;
        self.key2_high = 0;
        self.mode = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.state {
            // State0: detect preamble pattern type
            DecoderState::WaitEdge => {
                if !level {
                    return None;
                }

                let diff_250 = duration_diff!(duration, TE_SHORT_STD);
                let diff_125 = duration_diff!(duration, TE_SHORT_HALF);

                if diff_250 < TE_DELTA {
                    self.init_preamble_state();
                    self.state = DecoderState::CountPattern250;
                } else if diff_125 < TE_DELTA_HALF && duration <= 180 {
                    self.init_preamble_state();
                    self.state = DecoderState::CountPattern125;
                }
                self.prev_duration = duration;
            }

            // State1: count 250µs preamble (Pattern 1)
            DecoderState::CountPattern250 => {
                if level {
                    return None;
                }
                let diff_short = duration_diff!(duration, TE_SHORT_STD);
                if diff_short < TE_DELTA {
                    let prev_diff = duration_diff!(self.prev_duration, TE_SHORT_STD);
                    if prev_diff <= TE_DELTA {
                        self.pattern_counter += 1;
                    }
                    self.prev_duration = duration;
                } else {
                    let diff_long = duration_diff!(duration, TE_LONG_STD);
                    if diff_long < 100 && self.pattern_counter > 0x46 {
                        // Transition to Manchester decode at 250/500µs
                        self.state = DecoderState::DecodeManchester250;
                        self.data_low = 0;
                        self.data_high = 0;
                        self.bit_count = 0;
                        self.manchester_state = ManchesterState::Mid1;
                        self.pattern_counter = 0;
                        self.prev_duration = duration;
                        self.mode = 0x23;
                    } else {
                        self.state = DecoderState::WaitEdge;
                        self.pattern_counter = 0;
                    }
                }
            }

            // State2: Manchester decode at 250/500µs (Pattern 1)
            DecoderState::DecodeManchester250 => {
                if self.bit_count >= 121 {
                    return self.finalize_frame();
                }
                // Check for end-of-frame marker
                if level && self.bit_count == 80 {
                    let end_diff = duration_diff!(duration, TE_END_1000);
                    if end_diff <= 199 {
                        return self.finalize_frame();
                    }
                }

                let is_short = duration_diff!(duration, TE_SHORT_STD) < TE_DELTA;
                let is_long = duration_diff!(duration, TE_LONG_STD) < TE_DELTA;

                if duration > 10000 {
                    self.state = DecoderState::WaitEdge;
                    self.pattern_counter = 0;
                    return None;
                }

                if is_short || is_long {
                    let event_is_short = is_short;
                    let event_is_high = level;

                    if let Some(bit) = self.manchester_advance(event_is_short, event_is_high) {
                        self.add_bit(bit);
                    }
                } else {
                    if !level && duration_diff!(duration, TE_END_1000) < 199 {
                        if self.bit_count == 80 && (self.data_low & 0xF) == 0xA {
                             return self.finalize_frame();
                        }
                    }

                    self.state = DecoderState::WaitEdge;
                    self.pattern_counter = 0;
                }
            }

            // State3: count 125µs preamble (Pattern 2)
            DecoderState::CountPattern125 => {
                if level {
                    return None;
                }
                let diff_125 = duration_diff!(duration, TE_SHORT_HALF);
                let diff_250 = duration_diff!(duration, TE_LONG_HALF);

                if diff_125 < TE_DELTA_HALF {
                    let prev_diff = duration_diff!(self.prev_duration, TE_SHORT_HALF);
                    if prev_diff <= TE_DELTA_HALF {
                        self.pattern_counter += 1;
                    } else {
                        self.pattern_counter = 0;
                    }
                    self.prev_duration = duration;
                } else if diff_250 < TE_DELTA && self.pattern_counter >= 0x45 {
                    // Transition to Manchester decode at 125/250µs
                    self.state = DecoderState::DecodeManchester125;
                    self.data_low = 0;
                    self.data_high = 0;
                    self.bit_count = 0;
                    self.manchester_state = ManchesterState::Mid1;
                    self.prev_duration = duration;
                    self.mode = 0x36;
                } else {
                    self.state = DecoderState::WaitEdge;
                    self.pattern_counter = 0;
                }
            }

            // State4: Manchester decode at 125/250µs (Pattern 2)
            DecoderState::DecodeManchester125 => {
                if self.bit_count >= 121 {
                    return self.finalize_frame();
                }

                if level {
                    let end_diff = duration_diff!(duration, TE_END_500);
                    if end_diff <= 99 && self.bit_count == 80 {
                        return self.finalize_frame();
                    }
                } else {
                    let is_short = duration_diff!(duration, TE_SHORT_HALF) < TE_DELTA_HALF;
                    let is_long = duration >= TE_LONG_HALF && duration < 300; // TE_LONG_HALF matching C

                    if is_short || is_long {
                        let event_is_short = is_short;
                        let event_is_high = level;

                        if let Some(bit) = self.manchester_advance(event_is_short, event_is_high) {
                            self.add_bit(bit);
                        }
                    } else {
                        self.state = DecoderState::WaitEdge;
                        self.pattern_counter = 0;
                    }
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let serial = decoded.serial?;
        let counter = decoded.counter.unwrap_or(0).wrapping_add(1) as u32;

        let te_short: u32;
        let te_long_sync: u32;
        let end_dur: u32;

        let mut buffer = [0u8; 10];

        // C logic relies on mode inside generic, if extra has mode data. Assume mode 0x23 if none.
        let mode = if self.mode == 0x36 { 0x36 } else { 0x23 };

        if mode == 0x23 {
            te_short = TE_SHORT_STD;
            te_long_sync = TE_LONG_STD;
            end_dur = TE_END_1000;

            buffer[0] = 0x23;
            buffer[1] = 0x00;
            buffer[2] = (serial >> 16) as u8;
            buffer[3] = (serial >> 8) as u8;
            buffer[4] = serial as u8;
            buffer[5] = (counter >> 8) as u8;
            buffer[6] = counter as u8;
            buffer[7] = 0; // CRC
            buffer[8] = button & 0x0F;
            buffer[9] = 0;

            Self::xor_encrypt(&mut buffer);

            // psa_calculate_checksum equivalent
            let mut sum: u32 = 0;
            for i in 2..8 {
                sum += (buffer[i] as u32 & 0xF) + ((buffer[i] as u32 >> 4) & 0xF);
            }
            let checksum = (sum * 0x10) & 0xFF;

            buffer[8] = (buffer[8] & 0x0F) | (checksum as u8 & 0xF0);

            // Ptr restoration is omitted since we are recreating completely
            buffer[0] = buffer[2] ^ buffer[6];
            buffer[1] = buffer[3] ^ buffer[7];

        } else {
            te_short = TE_SHORT_HALF;
            te_long_sync = TE_LONG_HALF;
            end_dur = TE_END_500;

            let mut v0 = ((serial & 0xFFFFFF) << 8) |
                         ((button as u32 & 0xF) << 4) |
                         ((counter >> 24) & 0x0F);
            let mut v1 = ((counter & 0xFFFFFF) << 8) | 0;

            let crc8 = {
                let crc_val = ((v0 >> 24) & 0xFF) + ((v0 >> 16) & 0xFF) + ((v0 >> 8) & 0xFF) + (v0 & 0xFF) +
                              ((v1 >> 24) & 0xFF) + ((v1 >> 16) & 0xFF) + ((v1 >> 8) & 0xFF);
                crc_val & 0xFF
            };
            v1 = (v1 & 0xFFFFFF00) | crc8;

            let bf_counter = 0x23000000 | (serial & 0xFFFFFF); // PSA_BF1_START
            let mut wk2 = 0x0E0F5C41; // PSA_BF1_CONST_U4
            let mut wk3 = bf_counter;
            Self::tea_encrypt(&mut wk2, &mut wk3, &BF1_KEY_SCHEDULE);

            let mut wk0 = (bf_counter << 8) | 0x0E;
            let mut wk1 = 0x0F5C4123; // PSA_BF1_CONST_U5
            Self::tea_encrypt(&mut wk0, &mut wk1, &BF1_KEY_SCHEDULE);

            let wkey = [wk0, wk1, wk2, wk3];
            Self::tea_encrypt(&mut v0, &mut v1, &wkey);

            buffer[2] = (v0 >> 24) as u8;
            buffer[3] = (v0 >> 16) as u8;
            buffer[4] = (v0 >> 8) as u8;
            buffer[5] = v0 as u8;
            buffer[6] = (v1 >> 24) as u8;
            buffer[7] = (v1 >> 16) as u8;
            buffer[8] = (v1 >> 8) as u8;
            buffer[9] = v1 as u8;

            buffer[0] = buffer[2] ^ buffer[6];
            buffer[1] = buffer[3] ^ buffer[7];
        }

        let k1h = ((buffer[0] as u32) << 24) | ((buffer[1] as u32) << 16) | ((buffer[2] as u32) << 8) | buffer[3] as u32;
        let k1l = ((buffer[4] as u32) << 24) | ((buffer[5] as u32) << 16) | ((buffer[6] as u32) << 8) | buffer[7] as u32;
        let vf = ((buffer[8] as u16) << 8) | buffer[9] as u16;

        let mut signal = Vec::with_capacity(512);

        for _ in 0..80 {
            signal.push(LevelDuration::new(true, te_short));
            signal.push(LevelDuration::new(false, te_short));
        }

        signal.push(LevelDuration::new(false, te_short));
        signal.push(LevelDuration::new(true, te_long_sync));
        signal.push(LevelDuration::new(false, te_short));

        let k1data = ((k1h as u64) << 32) | k1l as u64;
        for bit in (0..64).rev() {
            let b = ((k1data >> bit) & 1) == 1;
            signal.push(LevelDuration::new(b, te_short));
            signal.push(LevelDuration::new(!b, te_short));
        }

        for bit in (0..16).rev() {
            let b = ((vf >> bit) & 1) == 1;
            signal.push(LevelDuration::new(b, te_short));
            signal.push(LevelDuration::new(!b, te_short));
        }

        signal.push(LevelDuration::new(true, end_dur));
        signal.push(LevelDuration::new(false, end_dur));

        Some(signal)
    }
}

impl Default for Psa2Decoder {
    fn default() -> Self {
        Self::new()
    }
}
