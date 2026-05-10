//! Honda Static protocol decoder
//!
//! Aligned with Flipper-ARF/lib/subghz/protocols/honda_static.c
//!
//! Protocol characteristics:
//! - 64-bit protocol (MIN_COUNT_BIT = 64)
//! - Manchester encoding
//! - Timing: Short duration 28-70 us (base 28, span 70 -> ~63us center), Long duration 61-130 us (base 61, span 130 -> ~126us center)
//! - Sync time: ~700 us
//! - Supported frequencies: 315 / 433 MHz

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 63;
const TE_LONG: u32 = 126;
const TE_DELTA: u32 = 30;
const MIN_COUNT_BIT: usize = 64;

const HONDA_STATIC_MIN_SYMBOLS: u16 = 36;
const HONDA_STATIC_SHORT_BASE_US: u32 = 28;
const HONDA_STATIC_SHORT_SPAN_US: u32 = 70;
const HONDA_STATIC_LONG_BASE_US: u32 = 61;
const HONDA_STATIC_LONG_SPAN_US: u32 = 130;
const HONDA_STATIC_SYNC_TIME_US: u32 = 700;
const HONDA_STATIC_ELEMENT_TIME_US: u32 = 63;
const HONDA_STATIC_PREAMBLE_ALTERNATING_COUNT: usize = 160;
const HONDA_STATIC_PREAMBLE_MAX_TRANSITIONS: u16 = 19;

const HONDA_STATIC_SYMBOL_CAPACITY: usize = 512;

#[derive(Debug, Clone, Copy)]
struct HondaStaticFields {
    button: u8,
    serial: u32,
    counter: u32,
    checksum: u8,
}

pub struct HondaStaticDecoder {
    symbols: Vec<u8>,
    te_last: u32,
}

impl HondaStaticDecoder {
    pub fn new() -> Self {
        Self {
            symbols: Vec::with_capacity(HONDA_STATIC_SYMBOL_CAPACITY),
            te_last: 0,
        }
    }

    fn push_symbol(&mut self, level: u8) {
        if self.symbols.len() < HONDA_STATIC_SYMBOL_CAPACITY {
            self.symbols.push(level);
        }
    }

    fn get_bits(packet: &[u8; 9], start_bit: usize, num_bits: usize) -> u8 {
        let mut value = 0u8;
        for i in 0..num_bits {
            let bit_idx = start_bit + i;
            let byte_idx = bit_idx >> 3;
            let bit_pos = 7 - (bit_idx & 7);
            let bit = (packet[byte_idx] >> bit_pos) & 1;
            value = (value << 1) | bit;
        }
        value
    }

    fn get_bits_u32(packet: &[u8; 9], start_bit: usize, num_bits: usize) -> u32 {
        let mut value = 0u32;
        for i in 0..num_bits {
            let bit_idx = start_bit + i;
            let byte_idx = bit_idx >> 3;
            let bit_pos = 7 - (bit_idx & 7);
            let bit = ((packet[byte_idx] >> bit_pos) & 1) as u32;
            value = (value << 1) | bit;
        }
        value
    }

    fn set_bits(packet: &mut [u8; 9], start_bit: usize, num_bits: usize, mut value: u32) {
        for i in (0..num_bits).rev() {
            let bit = (value & 1) as u8;
            value >>= 1;
            let bit_idx = start_bit + i;
            let byte_idx = bit_idx >> 3;
            let bit_pos = 7 - (bit_idx & 7);
            if bit == 1 {
                packet[byte_idx] |= 1 << bit_pos;
            } else {
                packet[byte_idx] &= !(1 << bit_pos);
            }
        }
    }

    fn is_valid_button(button: u8) -> bool {
        matches!(button, 0x02 | 0x04 | 0x08 | 0x05)
    }

    fn is_valid_serial(serial: u32) -> bool {
        serial != 0
    }

    fn validate_forward_packet(packet: &[u8; 9]) -> Option<HondaStaticFields> {
        let button = Self::get_bits(packet, 0, 4);
        let serial = Self::get_bits_u32(packet, 4, 28);
        let counter = Self::get_bits_u32(packet, 32, 24);
        let checksum = Self::get_bits(packet, 56, 8);

        let mut checksum_calc = 0u8;
        for i in 0..7 {
            checksum_calc ^= packet[i];
        }

        if checksum != checksum_calc {
            return None;
        }
        if !Self::is_valid_button(button) {
            return None;
        }
        if !Self::is_valid_serial(serial) {
            return None;
        }

        Some(HondaStaticFields {
            button,
            serial,
            counter,
            checksum,
        })
    }

    fn reverse_bits8(mut b: u8) -> u8 {
        b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
        b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
        b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
        b
    }

    fn validate_reverse_packet(packet: &[u8; 9]) -> Option<HondaStaticFields> {
        let mut reversed = [0u8; 9];
        for i in 0..9 {
            reversed[i] = Self::reverse_bits8(packet[i]);
        }

        let button = Self::get_bits(&reversed, 0, 4);
        let serial = Self::get_bits_u32(&reversed, 4, 28);
        let counter = Self::get_bits_u32(&reversed, 32, 24);

        let mut checksum = 0u8;
        for i in 0..7 {
            checksum ^= reversed[i];
        }

        if !Self::is_valid_button(button) {
            return None;
        }
        if !Self::is_valid_serial(serial) {
            return None;
        }

        Some(HondaStaticFields {
            button,
            serial,
            counter,
            checksum,
        })
    }

    fn manchester_pack_64(symbols: &[u8], start_pos: usize, inverted: bool) -> Option<[u8; 9]> {
        let mut packet = [0u8; 9];
        let mut pos = start_pos;
        let mut bit_count = 0;
        let count = symbols.len();

        while pos + 1 < count {
            if bit_count >= MIN_COUNT_BIT {
                break;
            }

            let a = symbols[pos];
            let b = symbols[pos + 1];

            if a == b {
                pos += 1;
                continue;
            }

            let bit = if inverted {
                a == 0 && b == 1
            } else {
                a == 1 && b == 0
            };

            if bit {
                packet[bit_count >> 3] |= 1 << (7 - (bit_count & 7));
            }

            bit_count += 1;
            pos += 2;
        }

        if bit_count < MIN_COUNT_BIT {
            None
        } else {
            Some(packet)
        }
    }

    fn parse_symbols(&self, inverted: bool) -> Option<HondaStaticFields> {
        let count = self.symbols.len();
        if count == 0 {
            return None;
        }

        let mut index = 1;
        let mut transitions = 0;

        while index < count {
            if self.symbols[index] != self.symbols[index - 1] {
                transitions += 1;
            } else {
                if transitions > HONDA_STATIC_PREAMBLE_MAX_TRANSITIONS {
                    break;
                }
                transitions = 0;
            }
            index += 1;
        }

        if index >= count {
            return None;
        }

        while index + 1 < count && self.symbols[index] == self.symbols[index + 1] {
            index += 1;
        }

        let data_start = index;

        if let Some(packet) = Self::manchester_pack_64(&self.symbols, data_start, inverted) {
            if let Some(fields) = Self::validate_forward_packet(&packet) {
                return Some(fields);
            }
            if !inverted {
                if let Some(fields) = Self::validate_reverse_packet(&packet) {
                    return Some(fields);
                }
            }
        }
        None
    }
}

impl ProtocolDecoder for HondaStaticDecoder {
    fn name(&self) -> &'static str {
        "Honda Static"
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
        &[315_000_000, 433_920_000]
    }

    fn reset(&mut self) {
        self.symbols.clear();
        self.te_last = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let sym = if level { 1 } else { 0 };

        if duration >= HONDA_STATIC_SHORT_BASE_US && duration - HONDA_STATIC_SHORT_BASE_US <= HONDA_STATIC_SHORT_SPAN_US {
            self.push_symbol(sym);
            return None;
        }

        if duration >= HONDA_STATIC_LONG_BASE_US && duration - HONDA_STATIC_LONG_BASE_US <= HONDA_STATIC_LONG_SPAN_US {
            self.push_symbol(sym);
            self.push_symbol(sym);
            return None;
        }

        let sc = self.symbols.len() as u16;
        let mut result = None;

        if sc >= HONDA_STATIC_MIN_SYMBOLS {
            if let Some(fields) = self.parse_symbols(true) {
                result = Some(fields);
            } else if let Some(fields) = self.parse_symbols(false) {
                result = Some(fields);
            }
        }

        self.symbols.clear();

        if let Some(fields) = result {
            let mut packet = [0u8; 9];
            Self::set_bits(&mut packet, 0, 4, fields.button as u32);
            Self::set_bits(&mut packet, 4, 28, fields.serial);
            Self::set_bits(&mut packet, 32, 24, fields.counter);
            Self::set_bits(&mut packet, 56, 8, fields.checksum as u32);

            let mut data = 0u64;
            for i in 0..8 {
                data = (data << 8) | packet[i] as u64;
            }

            Some(DecodedSignal {
                serial: Some(fields.serial),
                button: Some(fields.button),
                counter: Some(fields.counter as u16),
                crc_valid: true,
                data,
                data_count_bit: MIN_COUNT_BIT,
                encoder_capable: true,
                extra: Some(fields.counter as u64), // We can use extra for full counter since it's 24-bit
                protocol_display_name: None,
            })
        } else {
            None
        }
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut packet = [0u8; 9];
        let serial = decoded.serial?;
        let counter = decoded.extra.unwrap_or(decoded.counter.unwrap_or(0) as u64) as u32;

        HondaStaticDecoder::set_bits(&mut packet, 0, 4, button as u32);
        HondaStaticDecoder::set_bits(&mut packet, 4, 28, serial);
        HondaStaticDecoder::set_bits(&mut packet, 32, 24, counter);

        let mut checksum = 0u8;
        for i in 0..7 {
            checksum ^= packet[i];
        }
        HondaStaticDecoder::set_bits(&mut packet, 56, 8, checksum as u32);

        let mut signal = Vec::with_capacity(500);

        for _repeat in 0..3 {
            signal.push(LevelDuration::new(true, HONDA_STATIC_SYNC_TIME_US));

            for i in 0..HONDA_STATIC_PREAMBLE_ALTERNATING_COUNT {
                signal.push(LevelDuration::new((i & 1) != 0, HONDA_STATIC_ELEMENT_TIME_US));
            }

            for bit_idx in 0..MIN_COUNT_BIT {
                let byte_idx = bit_idx >> 3;
                let bit_pos = 7 - (bit_idx & 7);
                let value = ((packet[byte_idx] >> bit_pos) & 1) != 0;
                signal.push(LevelDuration::new(!value, HONDA_STATIC_ELEMENT_TIME_US));
                signal.push(LevelDuration::new(value, HONDA_STATIC_ELEMENT_TIME_US));
            }

            let last_bit = (packet[7] & 1) != 0;
            signal.push(LevelDuration::new(!last_bit, HONDA_STATIC_SYNC_TIME_US));
        }

        Some(signal)
    }
}

impl Default for HondaStaticDecoder {
    fn default() -> Self {
        Self::new()
    }
}
