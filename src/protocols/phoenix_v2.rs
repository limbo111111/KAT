//! Phoenix V2 protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/phoenix_v2.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 427;
const TE_LONG: u32 = 853;
const TE_DELTA: u32 = 100;

const MIN_COUNT_BIT: usize = 52;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct PhoenixV2Decoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl PhoenixV2Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }

    // Mirroring subghz_protocol_blocks_reverse_key logic
    fn reverse_key(key: u64, bit_count: usize) -> u64 {
        let mut res = 0;
        for i in 0..bit_count {
            res <<= 1;
            res |= (key >> i) & 1;
        }
        res
    }

    fn encrypt_counter(full_key: u64, counter: u16) -> u16 {
        let xor_key1 = (full_key >> 24) as u8;
        let xor_key2 = ((full_key >> 16) & 0xFF) as u8;

        let mut byte2 = (counter >> 8) as u8;
        let mut byte1 = (counter & 0xFF) as u8;

        for _ in 0..16 {
            let msb_of_prev_byte1 = byte2 & 0x80;

            if msb_of_prev_byte1 == 0 {
                byte2 ^= xor_key2;
                byte1 ^= xor_key1;
            }

            let lsb_of_current_byte1 = byte1 & 1;
            byte2 = (byte2 << 1) | lsb_of_current_byte1;
            byte1 = (byte1 >> 1) | msb_of_prev_byte1;
        }

        ((byte1 as u16) << 8) | (byte2 as u16)
    }

    fn decrypt_counter(full_key: u64) -> u16 {
        let encrypted_value = ((full_key >> 40) & 0xFFFF) as u16;

        let mut byte1 = (encrypted_value >> 8) as u8;
        let mut byte2 = (encrypted_value & 0xFF) as u8;

        let xor_key1 = (full_key >> 24) as u8;
        let xor_key2 = ((full_key >> 16) & 0xFF) as u8;

        for _ in 0..16 {
            let msb_of_byte1 = byte1 & 0x80;
            let lsb_of_byte2 = byte2 & 1;

            byte2 = (byte2 >> 1) | msb_of_byte1;
            byte1 = (byte1 << 1) | lsb_of_byte2;

            if msb_of_byte1 == 0 {
                byte1 ^= xor_key1;
                byte2 = (byte2 ^ xor_key2) & 0x7F;
            }
        }

        ((byte2 as u16) << 8) | (byte1 as u16)
    }
}

impl ProtocolDecoder for PhoenixV2Decoder {
    fn name(&self) -> &'static str {
        "V2 Phoenix"
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
        &[433_920_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 60) < TE_DELTA * 30 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_SHORT * 6) < TE_DELTA * 4 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= (TE_SHORT * 10 + TE_DELTA) {
                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let data_rev = Self::reverse_key(self.decode_data, self.decode_count_bit + 4);

                            let serial = (data_rev & 0xFFFFFFFF) as u32;
                            let cnt = Self::decrypt_counter(data_rev);
                            let btn = ((data_rev >> 32) & 0xF) as u8;

                            let res = DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: Some(cnt),
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };
                            return Some(res);
                        }
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    }
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA * 3
                    {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.add_bit(0);
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

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let serial = decoded.serial.unwrap_or(0);
        let mut cnt = decoded.counter.unwrap_or(0);
        let btn = button;

        cnt = cnt.wrapping_add(1); // Standard increment

        let data_rev = Self::reverse_key(decoded.data, decoded.data_count_bit + 4);
        let encrypted_counter = Self::encrypt_counter(data_rev, cnt);

        let new_data_rev = ((encrypted_counter as u64) << 40)
            | ((btn as u64) << 32)
            | (serial as u64);

        let data_to_encode = Self::reverse_key(new_data_rev, decoded.data_count_bit + 4);

        let mut upload = Vec::new();

        // Header
        upload.push(LevelDuration::new(false, TE_SHORT * 60));

        // Start bit
        upload.push(LevelDuration::new(true, TE_SHORT * 6));

        // Key data
        for i in (0..decoded.data_count_bit).rev() {
            if ((data_to_encode >> i) & 1) == 0 { // Inverted logic from C code !bit_read
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for PhoenixV2Decoder {
    fn default() -> Self {
        Self::new()
    }
}
