//! Mazda Siemens protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/mazda_siemens.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 250;
const TE_LONG: u32 = 500;
const TE_DELTA: u32 = 100;

const MIN_COUNT_BIT: usize = 64;
const MAZDA_PREAMBLE_MIN: u16 = 13;
const MAZDA_COMPLETION_MIN: u16 = 80;
const MAZDA_COMPLETION_MAX: u16 = 105;
const MAZDA_DATA_BUFFER_SIZE: usize = 14;
const MAZDA_PREAMBLE_BYTES: usize = 12;
const MAZDA_TX_GAP_US: u32 = 50000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    PreambleSave,
    PreambleCheck,
    DataSave,
    DataCheck,
}

pub struct MazdaSiemensDecoder {
    step: DecoderStep,
    te_last: u32,
    preamble_count: u16,
    bit_counter: u16,
    prev_state: u8,
    data_buffer: [u8; MAZDA_DATA_BUFFER_SIZE],
}

impl MazdaSiemensDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            preamble_count: 0,
            bit_counter: 0,
            prev_state: 0,
            data_buffer: [0; MAZDA_DATA_BUFFER_SIZE],
        }
    }

    #[inline]
    fn is_short(duration: u32) -> bool {
        duration_diff!(duration, TE_SHORT) < TE_DELTA
    }

    #[inline]
    fn is_long(duration: u32) -> bool {
        duration_diff!(duration, TE_LONG) < TE_DELTA
    }

    fn collect_bit(&mut self, state_bit: u8) {
        let byte_idx = (self.bit_counter >> 3) as usize;
        if byte_idx < MAZDA_DATA_BUFFER_SIZE {
            self.data_buffer[byte_idx] <<= 1;
            if state_bit == 0 {
                self.data_buffer[byte_idx] |= 1;
            }
        }
        self.bit_counter += 1;
    }

    fn byte_parity(mut val: u8) -> u8 {
        val ^= val >> 4;
        val ^= val >> 2;
        val ^= val >> 1;
        val & 1
    }

    fn xor_deobfuscate(data: &mut [u8; 8]) {
        let parity = Self::byte_parity(data[7]);

        if parity != 0 {
            let mask = data[6];
            for i in 0..6 {
                data[i] ^= mask;
            }
        } else {
            let mask = data[5];
            for i in 0..5 {
                data[i] ^= mask;
            }
            data[6] ^= mask;
        }

        let old5 = data[5];
        let old6 = data[6];
        data[5] = (old5 & 0xAA) | (old6 & 0x55);
        data[6] = (old5 & 0x55) | (old6 & 0xAA);
    }

    fn xor_obfuscate(data: &mut [u8; 8]) {
        let old5 = data[5];
        let old6 = data[6];
        data[5] = (old5 & 0xAA) | (old6 & 0x55);
        data[6] = (old5 & 0x55) | (old6 & 0xAA);

        let parity = Self::byte_parity(data[7]);

        if parity != 0 {
            let mask = data[6];
            for i in 0..6 {
                data[i] ^= mask;
            }
        } else {
            let mask = data[5];
            for i in 0..5 {
                data[i] ^= mask;
            }
            data[6] ^= mask;
        }
    }

    fn process_pair(&mut self, dur_first: u32, dur_second: u32) -> bool {
        let first_short = Self::is_short(dur_first);
        let first_long = Self::is_long(dur_first);
        let second_short = Self::is_short(dur_second);
        let second_long = Self::is_long(dur_second);

        if first_long && second_short {
            self.collect_bit(0);
            self.collect_bit(1);
            self.prev_state = 1;
            return true;
        }

        if first_short && second_long {
            self.collect_bit(1);
            self.prev_state = 0;
            return true;
        }

        if first_short && second_short {
            self.collect_bit(self.prev_state);
            return true;
        }

        if first_long && second_long {
            self.collect_bit(0);
            self.collect_bit(1);
            self.prev_state = 0;
            return true;
        }

        false
    }

    fn check_completion(&mut self) -> Option<DecodedSignal> {
        if self.bit_counter < MAZDA_COMPLETION_MIN || self.bit_counter > MAZDA_COMPLETION_MAX {
            return None;
        }

        let mut data = [0u8; 8];
        for i in 0..8 {
            data[i] = self.data_buffer[i + 1];
        }

        Self::xor_deobfuscate(&mut data);

        let mut checksum: u8 = 0;
        for i in 0..7 {
            checksum = checksum.wrapping_add(data[i]);
        }
        if checksum != data[7] {
            return None;
        }

        let mut packed: u64 = 0;
        for i in 0..8 {
            packed = (packed << 8) | (data[i] as u64);
        }

        let serial = (packed >> 32) as u32;
        let btn = ((packed >> 24) & 0xFF) as u8;
        let cnt = ((packed >> 8) & 0xFFFF) as u16;

        Some(DecodedSignal {
            serial: Some(serial),
            button: Some(btn),
            counter: Some(cnt),
            crc_valid: true,
            data: packed,
            data_count_bit: 64,
            encoder_capable: true,
            extra: None,
            protocol_display_name: None,
        })
    }

    fn encode_byte(upload: &mut Vec<LevelDuration>, byte: u8) {
        for bit in (0..=7).rev() {
            if ((byte >> bit) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_SHORT));
            }
        }
    }
}

impl ProtocolDecoder for MazdaSiemensDecoder {
    fn name(&self) -> &'static str {
        "MazdaSiemens"
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
        self.preamble_count = 0;
        self.bit_counter = 0;
        self.prev_state = 0;
        self.data_buffer = [0; MAZDA_DATA_BUFFER_SIZE];
    }

    fn feed(&mut self, _level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if Self::is_short(duration) {
                    self.te_last = duration;
                    self.preamble_count = 0;
                    self.step = DecoderStep::PreambleCheck;
                }
            }
            DecoderStep::PreambleSave => {
                self.te_last = duration;
                self.step = DecoderStep::PreambleCheck;
            }
            DecoderStep::PreambleCheck => {
                if Self::is_short(self.te_last) && Self::is_short(duration) {
                    self.preamble_count += 1;
                    self.step = DecoderStep::PreambleSave;
                } else if Self::is_short(self.te_last)
                    && Self::is_long(duration)
                    && self.preamble_count >= MAZDA_PREAMBLE_MIN
                {
                    self.bit_counter = 1;
                    self.data_buffer = [0; MAZDA_DATA_BUFFER_SIZE];
                    self.collect_bit(1);
                    self.prev_state = 0;
                    self.step = DecoderStep::DataSave;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::DataSave => {
                self.te_last = duration;
                self.step = DecoderStep::DataCheck;
            }
            DecoderStep::DataCheck => {
                if self.process_pair(self.te_last, duration) {
                    self.step = DecoderStep::DataSave;
                } else {
                    let result = self.check_completion();
                    self.step = DecoderStep::Reset;
                    return result;
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut data = [0u8; 8];
        for i in 0..8 {
            data[i] = ((decoded.data >> (56 - 8 * i)) & 0xFF) as u8;
        }

        // Apply new button if requested?
        // Note: keeping button change out unless specified

        let mut cnt_lo = data[6];
        cnt_lo = cnt_lo.wrapping_add(1);
        data[6] = cnt_lo;
        if cnt_lo == 0 {
            data[5] = data[5].wrapping_add(1);
        }
        data[4] = button;

        let mut checksum: u8 = 0;
        for i in 0..7 {
            checksum = checksum.wrapping_add(data[i]);
        }
        data[7] = checksum;

        let mut tx_data = data;
        Self::xor_obfuscate(&mut tx_data);

        let mut upload = Vec::new();

        for _ in 0..MAZDA_PREAMBLE_BYTES {
            Self::encode_byte(&mut upload, 0xFF);
        }

        upload.push(LevelDuration::new(false, MAZDA_TX_GAP_US));

        Self::encode_byte(&mut upload, 0xFF);
        Self::encode_byte(&mut upload, 0xFF);
        Self::encode_byte(&mut upload, 0xD7);

        for i in 0..8 {
            Self::encode_byte(&mut upload, 255 - tx_data[i]);
        }

        Self::encode_byte(&mut upload, 0x5A);
        upload.push(LevelDuration::new(false, MAZDA_TX_GAP_US));

        Some(upload)
    }
}

impl Default for MazdaSiemensDecoder {
    fn default() -> Self {
        Self::new()
    }
}
