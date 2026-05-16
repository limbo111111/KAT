//! SecPlus_v1 protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/secplus_v1.c`.
//!
//! Protocol characteristics:
//! - 315 MHz AM, 42 bits (2 packets of 21 digits in base-3)
//! - TE ~500us short, 1500us long
//! - Bit 0: low for te*3, high for te
//! - Bit 1: low for te*2, high for te*2
//! - Bit 2: low for te, high for te*3

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1500;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 21;

const SECPLUS_V1_BIT_0: u8 = 0;
const SECPLUS_V1_BIT_1: u8 = 1;
const SECPLUS_V1_BIT_2: u8 = 2;

const SECPLUS_V1_PACKET_1_HEADER: u8 = 0x00;
const SECPLUS_V1_PACKET_2_HEADER: u8 = 0x02;
const SECPLUS_V1_PACKET_1_INDEX_BASE: usize = 0;
const SECPLUS_V1_PACKET_2_INDEX_BASE: usize = 21;
const SECPLUS_V1_PACKET_1_ACCEPTED: u8 = 1 << 0;
const SECPLUS_V1_PACKET_2_ACCEPTED: u8 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SearchStartBit,
    SaveDuration,
    DecoderData,
}

pub struct SecPlusV1Decoder {
    step: DecoderStep,
    te_last: u32,
    packet_accepted: u8,
    base_packet_index: usize,
    data_array: [u8; 44],
    decode_data: u64,
    decode_count_bit: usize,
}

impl SecPlusV1Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            packet_accepted: 0,
            base_packet_index: 0,
            data_array: [0; 44],
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn reverse_key(mut data: u32, len: usize) -> u32 {
        let mut res = 0;
        for _ in 0..len {
            res = (res << 1) | (data & 1);
            data >>= 1;
        }
        res
    }

    fn decode_packets(&mut self) -> DecodedSignal {
        let mut rolling: u32 = 0;
        let mut fixed: u32 = 0;
        let mut acc: u32 = 0;

        // decode packet 1
        for i in (1..21).step_by(2) {
            let mut digit = self.data_array[i] as u32;
            rolling = (rolling * 3) + digit;
            acc += digit;

            digit = (60 + self.data_array[i + 1] as u32 - acc) % 3;
            fixed = (fixed * 3) + digit;
            acc += digit;
        }

        acc = 0;
        // decode packet 2
        for i in (22..42).step_by(2) {
            let mut digit = self.data_array[i] as u32;
            rolling = (rolling * 3) + digit;
            acc += digit;

            digit = (60 + self.data_array[i + 1] as u32 - acc) % 3;
            fixed = (fixed * 3) + digit;
            acc += digit;
        }

        rolling = Self::reverse_key(rolling, 32);
        let data = ((fixed as u64) << 32) | (rolling as u64);

        let id1 = (fixed / 9) % 3;
        let btn = fixed % 3;

        let serial = if id1 == 0 {
            (fixed / 27) % 2187
        } else {
            fixed / 27
        };

        DecodedSignal {
            serial: Some(serial),
            button: Some(btn as u8),
            counter: Some(rolling as u16), // Using 16 bit for generalized tracking
            crc_valid: true,
            data,
            data_count_bit: MIN_COUNT_BIT * 2,
            encoder_capable: true,
            extra: None,
            protocol_display_name: None,
        }
    }
}

impl ProtocolDecoder for SecPlusV1Decoder {
    fn name(&self) -> &'static str {
        "SecPlus_v1"
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
        &[315_000_000, 390_000_000] // Added 390MHz commonly used by Chamberlain
    }

    fn reset(&mut self) {
        // C implementation specifically says:
        // "does not reset the decoder because you need to get 2 parts of the package"
        // But KAT calls `reset()` between runs. For multi-packet, we should reset state.
        self.step = DecoderStep::Reset;
        self.packet_accepted = 0;
        self.data_array.fill(0);
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 120) < TE_DELTA * 120 {
                    self.step = DecoderStep::SearchStartBit;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.packet_accepted = 0;
                    self.data_array.fill(0);
                }
            }
            DecoderStep::SearchStartBit => {
                if level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.base_packet_index = SECPLUS_V1_PACKET_1_INDEX_BASE;
                        self.data_array[self.decode_count_bit + self.base_packet_index] = SECPLUS_V1_BIT_0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.base_packet_index = SECPLUS_V1_PACKET_2_INDEX_BASE;
                        self.data_array[self.decode_count_bit + self.base_packet_index] = SECPLUS_V1_BIT_2;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration_diff!(duration, TE_SHORT * 120) < TE_DELTA * 120 {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.base_packet_index == SECPLUS_V1_PACKET_1_INDEX_BASE {
                                self.packet_accepted |= SECPLUS_V1_PACKET_1_ACCEPTED;
                            }
                            if self.base_packet_index == SECPLUS_V1_PACKET_2_INDEX_BASE {
                                self.packet_accepted |= SECPLUS_V1_PACKET_2_ACCEPTED;
                            }

                            if self.packet_accepted == (SECPLUS_V1_PACKET_1_ACCEPTED | SECPLUS_V1_PACKET_2_ACCEPTED) {
                                let sig = self.decode_packets();
                                self.step = DecoderStep::Reset;
                                return Some(sig);
                            }
                        }
                        self.step = DecoderStep::SearchStartBit;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::DecoderData;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::DecoderData => {
                if level && self.decode_count_bit <= MIN_COUNT_BIT {
                    if duration_diff!(self.te_last, TE_SHORT * 3) < TE_DELTA * 3
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.data_array[self.decode_count_bit + self.base_packet_index] = SECPLUS_V1_BIT_0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT * 2) < TE_DELTA * 2
                        && duration_diff!(duration, TE_SHORT * 2) < TE_DELTA * 2
                    {
                        self.data_array[self.decode_count_bit + self.base_packet_index] = SECPLUS_V1_BIT_1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT * 3) < TE_DELTA * 3
                    {
                        self.data_array[self.decode_count_bit + self.base_packet_index] = SECPLUS_V1_BIT_2;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let fixed = (decoded.data >> 32) as u32;
        let mut rolling = decoded.counter.unwrap_or(0) as u32;

        rolling = rolling.wrapping_add(1); // Standard increment
        if rolling < 0xE6000000 {
            rolling = 0xE6000000;
        }

        let mut data_array = [0u8; 44];
        let mut rolling_array = [0u8; 20];
        let mut fixed_array = [0u8; 20];
        let mut acc = 0;

        let reversed_rolling = Self::reverse_key(rolling, 32);

        let mut r = reversed_rolling;
        let mut f = fixed;

        for i in (0..20).rev() {
            rolling_array[i] = (r % 3) as u8;
            r /= 3;
            fixed_array[i] = (f % 3) as u8;
            f /= 3;
        }

        data_array[SECPLUS_V1_PACKET_1_INDEX_BASE] = SECPLUS_V1_PACKET_1_HEADER;
        data_array[SECPLUS_V1_PACKET_2_INDEX_BASE] = SECPLUS_V1_PACKET_2_HEADER;

        // encode packet 1
        for i in 1..11 {
            acc += rolling_array[i - 1];
            data_array[i * 2 - 1] = rolling_array[i - 1];
            acc += fixed_array[i - 1];
            data_array[i * 2] = acc % 3;
        }

        acc = 0;
        // encode packet 2
        for i in 11..21 {
            acc += rolling_array[i - 1];
            data_array[i * 2] = rolling_array[i - 1];
            acc += fixed_array[i - 1];
            data_array[i * 2 + 1] = acc % 3;
        }

        let mut signal = Vec::with_capacity(128);

        // Send header packet 1
        signal.push(LevelDuration::new(false, TE_SHORT * 119));
        signal.push(LevelDuration::new(true, TE_SHORT));

        // Send data packet 1
        for i in (SECPLUS_V1_PACKET_1_INDEX_BASE + 1)..(SECPLUS_V1_PACKET_1_INDEX_BASE + 21) {
            match data_array[i] {
                SECPLUS_V1_BIT_0 => {
                    signal.push(LevelDuration::new(false, TE_SHORT * 3));
                    signal.push(LevelDuration::new(true, TE_SHORT));
                }
                SECPLUS_V1_BIT_1 => {
                    signal.push(LevelDuration::new(false, TE_SHORT * 2));
                    signal.push(LevelDuration::new(true, TE_SHORT * 2));
                }
                SECPLUS_V1_BIT_2 => {
                    signal.push(LevelDuration::new(false, TE_SHORT));
                    signal.push(LevelDuration::new(true, TE_SHORT * 3));
                }
                _ => return None,
            }
        }

        // Send header packet 2
        signal.push(LevelDuration::new(false, TE_SHORT * 116));
        signal.push(LevelDuration::new(true, TE_SHORT * 3));

        // Send data packet 2
        for i in (SECPLUS_V1_PACKET_2_INDEX_BASE + 1)..(SECPLUS_V1_PACKET_2_INDEX_BASE + 21) {
            match data_array[i] {
                SECPLUS_V1_BIT_0 => {
                    signal.push(LevelDuration::new(false, TE_SHORT * 3));
                    signal.push(LevelDuration::new(true, TE_SHORT));
                }
                SECPLUS_V1_BIT_1 => {
                    signal.push(LevelDuration::new(false, TE_SHORT * 2));
                    signal.push(LevelDuration::new(true, TE_SHORT * 2));
                }
                SECPLUS_V1_BIT_2 => {
                    signal.push(LevelDuration::new(false, TE_SHORT));
                    signal.push(LevelDuration::new(true, TE_SHORT * 3));
                }
                _ => return None,
            }
        }

        Some(signal)
    }
}

impl Default for SecPlusV1Decoder {
    fn default() -> Self {
        Self::new()
    }
}
