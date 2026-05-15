//! SecPlus_v2 protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/secplus_v2.c`.
//!
//! Protocol characteristics:
//! - 315 MHz AM, 62 bits
//! - TE ~250us short, 500us long
//! - Manchester Encoding (ShortLow, LongLow, ShortHigh, LongHigh)
//! - 2 packets combined into one DecodedSignal via mix_invert and mix_order logic

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 250;
const TE_LONG: u32 = 500;
const TE_DELTA: u32 = 110;
const MIN_COUNT_BIT: usize = 62;

const SECPLUS_V2_HEADER: u64 = 0x3C0000000000;
const SECPLUS_V2_HEADER_MASK: u64 = 0xFFFF3C0000000000;
const SECPLUS_V2_PACKET_1: u64 = 0x000000000000;
const SECPLUS_V2_PACKET_2: u64 = 0x010000000000;
const SECPLUS_V2_PACKET_MASK: u64 = 0x30000000000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ManchesterState {
    Mid0,
    Mid1,
    Start0,
    Start1,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    DecoderData,
}

pub struct SecPlusV2Decoder {
    step: DecoderStep,
    decode_data: u64,
    decode_count_bit: usize,
    manchester_saved_state: ManchesterState,
    secplus_packet_1: u64,
}

impl SecPlusV2Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            decode_data: 0,
            decode_count_bit: 0,
            manchester_saved_state: ManchesterState::Mid1,
            secplus_packet_1: 0,
        }
    }

    fn manchester_advance(&mut self, event_is_short: bool, event_is_high: bool) -> Option<bool> {
        match self.manchester_saved_state {
            ManchesterState::Mid1 => {
                if event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Start1;
                    None
                } else if !event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Mid0 => {
                if event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Start0;
                    None
                } else if !event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start1 => {
                if event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start0 => {
                if event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
        }
    }

    fn mix_invert(invert: u8, p: &mut [u16; 3]) -> bool {
        match invert {
            0x00 => { p[0] = !p[0] & 0x03FF; p[1] = !p[1] & 0x03FF; }
            0x01 => { p[1] = !p[1] & 0x03FF; }
            0x02 => { p[2] = !p[2] & 0x03FF; }
            0x04 => { p[0] = !p[0] & 0x03FF; p[1] = !p[1] & 0x03FF; p[2] = !p[2] & 0x03FF; }
            0x05 | 0x0A => { p[0] = !p[0] & 0x03FF; p[2] = !p[2] & 0x03FF; }
            0x06 => { p[1] = !p[1] & 0x03FF; p[2] = !p[2] & 0x03FF; }
            0x08 => { p[0] = !p[0] & 0x03FF; }
            0x09 => {}
            _ => return false,
        }
        true
    }

    fn mix_order_decode(order: u8, p: &mut [u16; 3]) -> bool {
        let a = p[0];
        let b = p[1];
        let c = p[2];
        match order {
            0x06 | 0x09 => { p[2] = a; p[0] = c; }
            0x08 | 0x04 => { p[1] = a; p[2] = b; p[0] = c; }
            0x01 => { p[2] = a; p[0] = b; p[1] = c; }
            0x00 => { p[2] = b; p[1] = c; }
            0x05 => { p[1] = a; p[0] = b; }
            0x02 | 0x0A => {}
            _ => return false,
        }
        true
    }

    fn mix_order_encode(order: u8, p: &mut [u16; 3]) -> bool {
        let a;
        let b;
        let c;
        match order {
            0x06 | 0x09 => { a = p[2]; b = p[1]; c = p[0]; }
            0x08 | 0x04 => { a = p[1]; b = p[2]; c = p[0]; }
            0x01 => { a = p[2]; b = p[0]; c = p[1]; }
            0x00 => { a = p[0]; b = p[2]; c = p[1]; }
            0x05 => { a = p[1]; b = p[0]; c = p[2]; }
            0x02 | 0x0A => { a = p[0]; b = p[1]; c = p[2]; }
            _ => return false,
        }
        p[0] = a;
        p[1] = b;
        p[2] = c;
        true
    }

    fn decode_half(mut data: u64, roll_array: &mut [u8; 9], fixed: &mut u32) -> bool {
        let order = ((data >> 34) & 0x0F) as u8;
        let invert = ((data >> 30) & 0x0F) as u8;
        let mut p = [0u16; 3];

        for i in (0..=29).rev().step_by(3) {
            p[0] = (p[0] << 1) | (((data >> i) & 1) as u16);
            if i >= 1 { p[1] = (p[1] << 1) | (((data >> (i - 1)) & 1) as u16); }
            if i >= 2 { p[2] = (p[2] << 1) | (((data >> (i - 2)) & 1) as u16); }
        }

        if !Self::mix_invert(invert, &mut p) { return false; }
        if !Self::mix_order_decode(order, &mut p) { return false; }

        data = ((order as u64) << 4) | (invert as u64);
        let mut k = 0;
        for i in (0..=6).rev().step_by(2) {
            roll_array[k] = ((data >> i) & 0x03) as u8;
            if roll_array[k] == 3 { return false; }
            k += 1;
        }

        for i in (0..=8).rev().step_by(2) {
            roll_array[k] = ((p[2] >> i) & 0x03) as u8;
            if roll_array[k] == 3 { return false; }
            k += 1;
        }

        *fixed = ((p[0] as u32) << 10) | (p[1] as u32);
        true
    }

    fn encode_half(roll_array: &[u8; 9], fixed: u32) -> u64 {
        let mut data: u64 = 0;
        let mut p = [((fixed >> 10) & 0x3FF) as u16, (fixed & 0x3FF) as u16, 0];
        let order = (roll_array[0] << 2) | roll_array[1];
        let invert = (roll_array[2] << 2) | roll_array[3];
        p[2] = ((roll_array[4] as u16) << 8) | ((roll_array[5] as u16) << 6) |
               ((roll_array[6] as u16) << 4) | ((roll_array[7] as u16) << 2) |
               (roll_array[8] as u16);

        if !Self::mix_order_encode(order, &mut p) { return 0; }
        if !Self::mix_invert(invert, &mut p) { return 0; }

        for i in 0..10 {
            data <<= 3;
            data |= (((p[0] >> (9 - i)) & 1) as u64) << 2 |
                    (((p[1] >> (9 - i)) & 1) as u64) << 1 |
                    (((p[2] >> (9 - i)) & 1) as u64);
        }
        data |= (order as u64) << 34 | (invert as u64) << 30;

        data
    }

    fn reverse_key(mut data: u32, len: usize) -> u32 {
        let mut res = 0;
        for _ in 0..len {
            res = (res << 1) | (data & 1);
            data >>= 1;
        }
        res
    }

    fn check_packet(&mut self) -> bool {
        if (self.decode_data & SECPLUS_V2_HEADER_MASK) == SECPLUS_V2_HEADER {
            if (self.decode_data & SECPLUS_V2_PACKET_MASK) == SECPLUS_V2_PACKET_1 {
                self.secplus_packet_1 = self.decode_data;
            } else if (self.decode_data & SECPLUS_V2_PACKET_MASK) == SECPLUS_V2_PACKET_2
                && self.secplus_packet_1 != 0
            {
                return true;
            }
        }
        false
    }
}

impl ProtocolDecoder for SecPlusV2Decoder {
    fn name(&self) -> &'static str {
        "SecPlus_v2"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: TE_DELTA,
            min_count_bit: MIN_COUNT_BIT,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[315_000_000, 390_000_000] // Common Chamberlain frequencies
    }

    fn reset(&mut self) {
        // As per C logic, don't reset tracking state fully to allow multi-packet capture.
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut is_adv = false;
        let mut is_short = false;
        let mut is_high = false;

        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_LONG * 130) < TE_DELTA * 100 {
                    self.step = DecoderStep::DecoderData;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.secplus_packet_1 = 0;

                    self.manchester_saved_state = ManchesterState::Mid1;

                    // Manually inject LongHigh then ShortLow
                    self.manchester_saved_state = ManchesterState::Mid0; // After LongHigh
                    self.manchester_saved_state = ManchesterState::Start0; // After ShortLow
                }
            }
            DecoderStep::DecoderData => {
                let diff_short = duration_diff!(duration, TE_SHORT);
                let diff_long = duration_diff!(duration, TE_LONG);

                if !level {
                    if diff_short < TE_DELTA {
                        is_adv = true; is_short = true; is_high = false;
                    } else if diff_long < TE_DELTA {
                        is_adv = true; is_short = false; is_high = false;
                    } else if duration >= (TE_LONG * 2 + TE_DELTA) {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.check_packet() {
                                let mut roll_1 = [0u8; 9];
                                let mut fixed_1 = 0;
                                let mut roll_2 = [0u8; 9];
                                let mut fixed_2 = 0;

                                if Self::decode_half(self.secplus_packet_1, &mut roll_1, &mut fixed_1) &&
                                   Self::decode_half(self.decode_data, &mut roll_2, &mut fixed_2) {

                                    let mut rolling_digits = [0u8; 18];
                                    rolling_digits[0] = roll_2[8]; rolling_digits[1] = roll_1[8];
                                    rolling_digits[2] = roll_2[4]; rolling_digits[3] = roll_2[5];
                                    rolling_digits[4] = roll_2[6]; rolling_digits[5] = roll_2[7];
                                    rolling_digits[6] = roll_1[4]; rolling_digits[7] = roll_1[5];
                                    rolling_digits[8] = roll_1[6]; rolling_digits[9] = roll_1[7];
                                    rolling_digits[10] = roll_2[0]; rolling_digits[11] = roll_2[1];
                                    rolling_digits[12] = roll_2[2]; rolling_digits[13] = roll_2[3];
                                    rolling_digits[14] = roll_1[0]; rolling_digits[15] = roll_1[1];
                                    rolling_digits[16] = roll_1[2]; rolling_digits[17] = roll_1[3];

                                    let mut rolling: u32 = 0;
                                    for digit in rolling_digits {
                                        rolling = (rolling * 3) + digit as u32;
                                    }

                                    if rolling < 0x10000000 {
                                        let cnt = Self::reverse_key(rolling, 28) as u16;
                                        let btn = (fixed_1 >> 12) as u8;
                                        let serial = (fixed_1 << 20) | fixed_2;

                                        let data = self.decode_data; // Store packet 2 as representation
                                        let bit_count = MIN_COUNT_BIT;

                                        self.step = DecoderStep::Reset;

                                        return Some(DecodedSignal {
                                            serial: Some(serial),
                                            button: Some(btn),
                                            counter: Some(cnt),
                                            crc_valid: true,
                                            data,
                                            data_count_bit: bit_count,
                                            encoder_capable: true,
                                            extra: Some(self.secplus_packet_1), // Store pack1 in extra
                                            protocol_display_name: None,
                                        });
                                    }
                                }
                            }
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.manchester_saved_state = ManchesterState::Mid1;
                        self.manchester_saved_state = ManchesterState::Mid0;
                        self.manchester_saved_state = ManchesterState::Start0;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if diff_short < TE_DELTA {
                        is_adv = true; is_short = true; is_high = true;
                    } else if diff_long < TE_DELTA {
                        is_adv = true; is_short = false; is_high = true;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }
            }
        }

        if is_adv {
            if let Some(bit) = self.manchester_advance(is_short, is_high) {
                self.decode_data = (self.decode_data << 1) | (bit as u64);
                self.decode_count_bit += 1;
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let serial = decoded.serial.unwrap_or(0);
        let btn = button;
        let mut cnt = decoded.counter.unwrap_or(0) as u32;

        cnt = cnt.wrapping_add(1);
        if cnt < 0xE500000 {
            cnt = 0xE500000;
        }

        let fixed_1 = ((btn as u32) << 12) | (serial >> 20);
        let fixed_2 = serial & 0xFFFFF;

        let mut rolling_digits = [0u8; 18];
        let mut rolling = Self::reverse_key(cnt, 28);
        for i in (0..=17).rev() {
            rolling_digits[i] = (rolling % 3) as u8;
            rolling /= 3;
        }

        let mut roll_1 = [0u8; 9];
        let mut roll_2 = [0u8; 9];

        roll_2[8] = rolling_digits[0]; roll_1[8] = rolling_digits[1];
        roll_2[4] = rolling_digits[2]; roll_2[5] = rolling_digits[3];
        roll_2[6] = rolling_digits[4]; roll_2[7] = rolling_digits[5];
        roll_1[4] = rolling_digits[6]; roll_1[5] = rolling_digits[7];
        roll_1[6] = rolling_digits[8]; roll_1[7] = rolling_digits[9];
        roll_2[0] = rolling_digits[10]; roll_2[1] = rolling_digits[11];
        roll_2[2] = rolling_digits[12]; roll_2[3] = rolling_digits[13];
        roll_1[0] = rolling_digits[14]; roll_1[1] = rolling_digits[15];
        roll_1[2] = rolling_digits[16]; roll_1[3] = rolling_digits[17];

        let p1 = SECPLUS_V2_HEADER | SECPLUS_V2_PACKET_1 | Self::encode_half(&roll_1, fixed_1);
        let p2 = SECPLUS_V2_HEADER | SECPLUS_V2_PACKET_2 | Self::encode_half(&roll_2, fixed_2);

        let mut signal = Vec::with_capacity(256);

        let add_duration = |sig: &mut Vec<LevelDuration>, event_is_short: bool, event_is_high: bool| {
            let dur = if event_is_short { TE_SHORT } else { TE_LONG };
            sig.push(LevelDuration::new(event_is_high, dur));
        };

        for p in [p1, p2] {
            let mut enc_state = ManchesterState::Mid1;

            for i in (0..MIN_COUNT_BIT).rev() {
                let bit = (p >> i) & 1 == 1;

                let (is_adv, is_short, is_high) = match enc_state {
                    ManchesterState::Mid1 => {
                        if bit {
                            enc_state = ManchesterState::Start1;
                            (false, true, false)
                        } else {
                            enc_state = ManchesterState::Mid0;
                            (false, false, true)
                        }
                    }
                    ManchesterState::Mid0 => {
                        if bit {
                            enc_state = ManchesterState::Mid1;
                            (false, false, false)
                        } else {
                            enc_state = ManchesterState::Start0;
                            (false, true, true)
                        }
                    }
                    ManchesterState::Start1 => {
                        enc_state = ManchesterState::Mid1;
                        (true, true, true)
                    }
                    ManchesterState::Start0 => {
                        enc_state = ManchesterState::Mid0;
                        (true, true, false)
                    }
                };

                if is_adv {
                    add_duration(&mut signal, is_short, is_high);
                    let (_, s2, h2) = match enc_state {
                        ManchesterState::Mid1 => if bit { enc_state = ManchesterState::Start1; (false, true, false) } else { enc_state = ManchesterState::Mid0; (false, false, true) },
                        ManchesterState::Mid0 => if bit { enc_state = ManchesterState::Mid1; (false, false, false) } else { enc_state = ManchesterState::Start0; (false, true, true) },
                        _ => (false, true, false)
                    };
                    add_duration(&mut signal, s2, h2);
                } else {
                    add_duration(&mut signal, is_short, is_high);
                }
            }

            let (is_short, is_high) = match enc_state {
                ManchesterState::Mid1 => (true, false),
                ManchesterState::Mid0 => (true, true),
                ManchesterState::Start1 => (true, true),
                ManchesterState::Start0 => (true, false),
            };
            add_duration(&mut signal, is_short, is_high);
            signal.push(LevelDuration::new(false, TE_LONG * 136));
        }

        Some(signal)
    }
}

impl Default for SecPlusV2Decoder {
    fn default() -> Self {
        Self::new()
    }
}
