//! Honda Static protocol decoder
//!
//! Aligned with Flipper-ARF honda_static.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::radio::demodulator::LevelDuration;

const HONDA_STATIC_BIT_COUNT: usize = 64;
const HONDA_STATIC_MIN_SYMBOLS: usize = 36;
const HONDA_STATIC_SHORT_BASE_US: u32 = 28;
const HONDA_STATIC_SHORT_SPAN_US: u32 = 70;
const HONDA_STATIC_LONG_BASE_US: u32 = 61;
const HONDA_STATIC_LONG_SPAN_US: u32 = 130;
const HONDA_STATIC_SYNC_TIME_US: u32 = 700;
const HONDA_STATIC_ELEMENT_TIME_US: u32 = 63;
const HONDA_STATIC_SYMBOL_CAPACITY: usize = 512;
const HONDA_STATIC_PREAMBLE_ALTERNATING_COUNT: usize = 160;
const HONDA_STATIC_PREAMBLE_MAX_TRANSITIONS: usize = 19;
const HONDA_STATIC_SYMBOL_BYTE_COUNT: usize = (HONDA_STATIC_SYMBOL_CAPACITY + 7) / 8;

pub struct HondaStaticDecoder {
    symbols: [u8; HONDA_STATIC_SYMBOL_BYTE_COUNT],
    symbols_count: usize,
}

impl HondaStaticDecoder {
    pub fn new() -> Self {
        Self {
            symbols: [0; HONDA_STATIC_SYMBOL_BYTE_COUNT],
            symbols_count: 0,
        }
    }

    fn symbol_set(&mut self, index: usize, v: bool) {
        let byte_index = index >> 3;
        let shift = (!index) & 0x07;
        let mask = 1 << shift;
        if v {
            self.symbols[byte_index] |= mask;
        } else {
            self.symbols[byte_index] &= !mask;
        }
    }

    fn symbol_get(buf: &[u8; HONDA_STATIC_SYMBOL_BYTE_COUNT], index: usize) -> u8 {
        let byte_index = index >> 3;
        let shift = (!index) & 0x07;
        (buf[byte_index] >> shift) & 1
    }

    fn get_bits(data: &[u8; 9], start: usize, count: usize) -> u32 {
        let mut value = 0;
        for i in 0..count {
            let bit_index = start + i;
            let byte = data[bit_index >> 3];
            let shift = (!bit_index) & 0x07;
            value = (value << 1) | ((byte >> shift) & 1) as u32;
        }
        value
    }

    fn set_bits(data: &mut [u8; 8], start: usize, count: usize, value: u32) {
        for i in 0..count {
            let bit_index = start + i;
            let byte_index = bit_index >> 3;
            let shift = (!bit_index) & 0x07;
            let mask = 1 << shift;
            let bit = ((value >> (count - 1 - i)) & 1) != 0;
            if bit {
                data[byte_index] |= mask;
            } else {
                data[byte_index] &= !mask;
            }
        }
    }

    fn reverse_bits8(mut value: u8) -> u8 {
        value = ((value >> 4) | (value << 4)) & 0xFF;
        value = ((value & 0x33) << 2) | ((value >> 2) & 0x33);
        value = ((value & 0x55) << 1) | ((value >> 1) & 0x55);
        value
    }

    fn is_valid_button(button: u8) -> bool {
        if button > 9 {
            return false;
        }
        ((0x336 >> button) & 1) != 0
    }

    fn is_valid_serial(serial: u32) -> bool {
        serial != 0 && serial != 0x0FFFFFFF
    }

    fn validate_forward_packet(packet: &[u8; 9]) -> Option<(u8, u32, u32)> {
        let button = Self::get_bits(packet, 0, 4) as u8;
        let serial = Self::get_bits(packet, 4, 28);
        let counter = Self::get_bits(packet, 32, 24);
        let checksum = Self::get_bits(packet, 56, 8) as u8;

        let mut checksum_calc = 0;
        for i in 0..7 {
            checksum_calc ^= packet[i];
        }

        if checksum != checksum_calc || !Self::is_valid_button(button) || !Self::is_valid_serial(serial) {
            return None;
        }
        Some((button, serial, counter))
    }

    fn validate_reverse_packet(packet: &[u8; 9]) -> Option<(u8, u32, u32)> {
        let mut reversed = [0u8; 9];
        for i in 0..9 {
            reversed[i] = Self::reverse_bits8(packet[i]);
        }

        let button = Self::get_bits(&reversed, 0, 4) as u8;
        let serial = Self::get_bits(&reversed, 4, 28);
        let counter = Self::get_bits(&reversed, 32, 24);

        let mut checksum = 0;
        for i in 0..7 {
            checksum ^= reversed[i];
        }

        if !Self::is_valid_button(button) || !Self::is_valid_serial(serial) {
            return None;
        }
        Some((button, serial, counter))
    }

    fn manchester_pack_64(symbol_bits: &[u8; HONDA_STATIC_SYMBOL_BYTE_COUNT], count: usize, start_pos: usize, inverted: bool, packet: &mut [u8; 9]) -> bool {
        let mut pos = start_pos;
        let mut bit_count = 0;

        for x in packet.iter_mut() { *x = 0; }

        while pos + 1 < count {
            if bit_count >= HONDA_STATIC_BIT_COUNT {
                break;
            }

            let a = Self::symbol_get(symbol_bits, pos);
            let b = Self::symbol_get(symbol_bits, pos + 1);

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
                packet[bit_count >> 3] |= 1 << ((!bit_count) & 0x07);
            }

            bit_count += 1;
            pos += 2;
        }

        bit_count >= HONDA_STATIC_BIT_COUNT
    }

    fn parse_symbols(&mut self, inverted: bool) -> Option<DecodedSignal> {
        let count = self.symbols_count;
        let mut index = 1;
        let mut transitions = 0;

        while index < count {
            if Self::symbol_get(&self.symbols, index) != Self::symbol_get(&self.symbols, index - 1) {
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

        while index + 1 < count && Self::symbol_get(&self.symbols, index) == Self::symbol_get(&self.symbols, index + 1) {
            index += 1;
        }

        let data_start = index;
        let mut packet = [0u8; 9];

        if !Self::manchester_pack_64(&self.symbols, count, data_start, inverted, &mut packet) {
            return None;
        }

        let validated = Self::validate_forward_packet(&packet)
            .or_else(|| {
                if inverted { None } else { Self::validate_reverse_packet(&packet) }
            });

        if let Some((button, serial, counter)) = validated {
            // Need to convert to standard generic.data 64-bit compact format used in Flipper
            let mut compact = [0u8; 8];
            compact[0] = button & 0x0F;
            compact[1] = (serial >> 20) as u8;
            compact[2] = (serial >> 12) as u8;
            compact[3] = (serial >> 4) as u8;
            compact[4] = (serial << 4) as u8;
            compact[5] = (counter >> 16) as u8;
            compact[6] = (counter >> 8) as u8;
            compact[7] = counter as u8;

            let mut data = 0u64;
            for i in 0..8 {
                data = (data << 8) | compact[i] as u64;
            }

            return Some(DecodedSignal {
                serial: Some(serial),
                button: Some(button),
                counter: Some(counter as u16), // usually displayed / returned as u16 or u32
                crc_valid: true,
                data,
                data_count_bit: HONDA_STATIC_BIT_COUNT,
                encoder_capable: true,
                extra: None,
                protocol_display_name: None,
            });
        }
        None
    }
}

impl ProtocolDecoder for HondaStaticDecoder {
    fn name(&self) -> &'static str {
        "Honda Static"
    }

    fn timing(&self) -> ProtocolTiming {
        // Mock timings, this protocol mostly relies on strict ranges in its feed
        ProtocolTiming {
            te_short: HONDA_STATIC_SHORT_BASE_US,
            te_long: HONDA_STATIC_LONG_BASE_US,
            te_delta: 50,
            min_count_bit: HONDA_STATIC_BIT_COUNT,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[315_000_000, 433_920_000]
    }

    fn reset(&mut self) {
        self.symbols_count = 0;
        self.symbols = [0; HONDA_STATIC_SYMBOL_BYTE_COUNT];
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        if duration >= HONDA_STATIC_SHORT_BASE_US && (duration - HONDA_STATIC_SHORT_BASE_US) <= HONDA_STATIC_SHORT_SPAN_US {
            if self.symbols_count < HONDA_STATIC_SYMBOL_CAPACITY {
                self.symbol_set(self.symbols_count, level);
                self.symbols_count += 1;
            }
            return None;
        }

        if duration >= HONDA_STATIC_LONG_BASE_US && (duration - HONDA_STATIC_LONG_BASE_US) <= HONDA_STATIC_LONG_SPAN_US {
            if self.symbols_count + 2 <= HONDA_STATIC_SYMBOL_CAPACITY {
                self.symbol_set(self.symbols_count, level);
                self.symbols_count += 1;
                self.symbol_set(self.symbols_count, level);
                self.symbols_count += 1;
            }
            return None;
        }

        if self.symbols_count >= HONDA_STATIC_MIN_SYMBOLS {
            if let Some(res) = self.parse_symbols(true) {
                self.symbols_count = 0;
                return Some(res);
            }
            if let Some(res) = self.parse_symbols(false) {
                self.symbols_count = 0;
                return Some(res);
            }
        }

        self.symbols_count = 0;
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        // Recover serial, counter
        let mut compact = [0u8; 8];
        let mut key = decoded.data;
        for i in 0..8 {
            compact[7 - i] = (key & 0xFF) as u8;
            key >>= 8;
        }

        let serial = ((compact[1] as u32) << 20) | ((compact[2] as u32) << 12) | ((compact[3] as u32) << 4) | ((compact[4] as u32) >> 4);
        let counter = ((compact[5] as u32) << 16) | ((compact[6] as u32) << 8) | (compact[7] as u32);

        // Use provided button, or fall back to map
        let mut btn = button;
        if !Self::is_valid_button(btn) {
            btn = 1; // Default
        }

        let mut packet = [0u8; 8];
        Self::set_bits(&mut packet, 0, 4, btn as u32);
        Self::set_bits(&mut packet, 4, 28, serial);
        Self::set_bits(&mut packet, 32, 24, counter);

        let mut checksum = 0;
        for i in 0..7 {
            checksum ^= packet[i];
        }
        Self::set_bits(&mut packet, 56, 8, checksum as u32);

        let mut out = Vec::new();
        out.push(LevelDuration::new(true, HONDA_STATIC_SYNC_TIME_US));

        for i in 0..HONDA_STATIC_PREAMBLE_ALTERNATING_COUNT {
            out.push(LevelDuration::new((i & 1) != 0, HONDA_STATIC_ELEMENT_TIME_US));
        }

        for bit in 0..HONDA_STATIC_BIT_COUNT {
            let value = ((packet[bit >> 3] >> ((!bit) & 0x07)) & 1) != 0;
            out.push(LevelDuration::new(!value, HONDA_STATIC_ELEMENT_TIME_US));
            out.push(LevelDuration::new(value, HONDA_STATIC_ELEMENT_TIME_US));
        }

        let last_bit = (packet[7] & 1) != 0;
        out.push(LevelDuration::new(!last_bit, HONDA_STATIC_SYNC_TIME_US));

        Some(out)
    }
}

impl Default for HondaStaticDecoder {
    fn default() -> Self {
        Self::new()
    }
}
