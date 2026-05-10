//! Kia V7 protocol decoder
//!
//! Aligned with Flipper-ARF reference: `kia_v7.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz FM
//! - 64 bits Manchester encoding
//! - te_short = 250 µs, te_long = 500 µs
//! - Preamble: >= 16 short pairs
//! - Sync: Long HI, Short LO. Implicitly adds 4 bits: 1, 0, 1, 1
//! - Payload inverted
//! - Custom CRC8 (poly 0x7F, init 0x4C)
//! - Fields: [16:47] serial, [8:23] counter, [48:51] button

use super::common::DecodedSignal;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};
use super::{ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 250;
const TE_LONG: u32 = 500;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 64;
const PREAMBLE_MIN_PAIRS: u16 = 16;
const TAIL_GAP_US: u32 = 2000; // 0x7D0
const GAP_US: u32 = 10000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    SyncLow,
    Data,
}

pub struct KiaV7Decoder {
    step: DecoderStep,
    te_last: u32,
    preamble_count: u16,
    manchester_state: CommonManchesterState,
    decode_data: u64,
    decode_count_bit: usize,
}

impl KiaV7Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            preamble_count: 0,
            manchester_state: CommonManchesterState::Start0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn manchester_advance(&mut self, event: u8) -> Option<bool> {
        let (next_state, bit) = common_manchester_advance(self.manchester_state, event);
        self.manchester_state = next_state;
        bit
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }

    fn crc8(data: &[u8]) -> u8 {
        let mut crc: u8 = 0x4C;
        for &byte in data {
            crc ^= byte;
            for _ in 0..8 {
                let msb = (crc & 0x80) != 0;
                crc <<= 1;
                if msb {
                    crc ^= 0x7F;
                }
            }
        }
        crc
    }

    fn process_data(&self) -> Option<DecodedSignal> {
        let candidate = !self.decode_data; // inverted payload

        let mut bytes = [0u8; 8];
        for i in 0..8 {
            bytes[i] = ((candidate >> ((7 - i) * 8)) & 0xFF) as u8;
        }

        let crc_calc = Self::crc8(&bytes[0..7]);
        let crc_pkt = bytes[7];

        if crc_calc != crc_pkt {
            return None;
        }

        let serial = (((bytes[3] as u32) << 20) | ((bytes[4] as u32) << 12) | ((bytes[5] as u32) << 4) | ((bytes[6] as u32) >> 4)) & 0x0FFFFFFF;
        let counter = ((bytes[1] as u16) << 8) | (bytes[2] as u16);
        let button = bytes[6] & 0x0F;

        let mut button_name = None;
        match button {
            0x01 => button_name = Some("Lock"),
            0x02 => button_name = Some("Unlock"),
            0x03 | 0x08 => button_name = Some("Trunk"), // BOOT
            _ => button_name = Some("Unknown"),
        }

        let mut sig = DecodedSignal {
            serial: Some(serial),
            button: Some(button),
            counter: Some(counter),
            crc_valid: true,
            data: candidate,
            data_count_bit: MIN_COUNT_BIT,
            encoder_capable: true,
            protocol_display_name: Some("Kia V7".to_string()),
            extra: Some(bytes[0] as u64), // Store fixed_high_byte in extra for encoding
        };

        if let Some(name) = button_name {
            sig.protocol_display_name = Some(format!("Kia V7 ({})", name));
        }

        Some(sig)
    }

    fn add_level(signal: &mut Vec<LevelDuration>, level: bool, duration: u32) {
        if let Some(last) = signal.last_mut() {
            if last.level == level {
                *last = LevelDuration::new(level, last.duration_us + duration);
                return;
            }
        }
        signal.push(LevelDuration::new(level, duration));
    }
}

impl ProtocolDecoder for KiaV7Decoder {
    fn name(&self) -> &'static str {
        "Kia V7"
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
        &[433_920_000, 315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_last = 0;
        self.preamble_count = 0;
        self.manchester_state = CommonManchesterState::Start0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let is_short = duration_diff!(duration, TE_SHORT) < TE_DELTA;
        let is_long = duration_diff!(duration, TE_LONG) < TE_DELTA;

        match self.step {
            DecoderStep::Reset => {
                if level && is_short {
                    self.step = DecoderStep::Preamble;
                    self.te_last = duration;
                    self.preamble_count = 0;
                    self.manchester_state = CommonManchesterState::Start0;
                }
            }

            DecoderStep::Preamble => {
                if level {
                    if is_long && duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                        if self.preamble_count >= PREAMBLE_MIN_PAIRS - 1 {
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            self.preamble_count = 0;

                            self.add_bit(1);
                            self.add_bit(0);
                            self.add_bit(1);
                            self.add_bit(1);

                            self.te_last = duration;
                            self.step = DecoderStep::SyncLow;
                        } else {
                            self.reset();
                        }
                    } else if is_short {
                        self.te_last = duration;
                    } else {
                        self.reset();
                    }
                } else {
                    if is_short && duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                        self.preamble_count += 1;
                    } else {
                        self.reset();
                    }
                }
            }

            DecoderStep::SyncLow => {
                if !level && is_short && duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                    self.te_last = duration;
                    self.step = DecoderStep::Data;
                } else {
                    self.reset();
                }
            }

            DecoderStep::Data => {
                let event = if is_short {
                    if level { 1 } else { 0 }
                } else if is_long {
                    if level { 3 } else { 2 }
                } else {
                    4
                };

                if is_short || is_long {
                    if let Some(bit) = self.manchester_advance(event) {
                        self.add_bit(if bit { 1 } else { 0 });
                    }
                } else {
                    self.reset();
                    return None;
                }

                if self.decode_count_bit == MIN_COUNT_BIT {
                    let res = self.process_data();
                    self.reset();
                    return res;
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
        let counter = decoded.counter.unwrap_or(0);
        let fixed_high_byte = decoded.extra.unwrap_or(0x4C) as u8;

        let mut bytes = [0u8; 8];
        bytes[0] = fixed_high_byte;
        bytes[1] = (counter >> 8) as u8;
        bytes[2] = counter as u8;
        bytes[3] = (serial >> 20) as u8;
        bytes[4] = (serial >> 12) as u8;
        bytes[5] = (serial >> 4) as u8;
        bytes[6] = (((serial & 0x0F) as u8) << 4) | (button & 0x0F);
        bytes[7] = Self::crc8(&bytes[0..7]);

        let mut data: u64 = 0;
        for i in 0..8 {
            data = (data << 8) | (bytes[i] as u64);
        }

        let mut signal = Vec::new();

        for _ in 0..2 {
            for _ in 0..319 { // KIA_V7_PREAMBLE_PAIRS = 0x13F = 319
                Self::add_level(&mut signal, true, TE_SHORT);
                Self::add_level(&mut signal, false, TE_SHORT);
            }

            Self::add_level(&mut signal, true, TE_SHORT); // extra preamble high? Wait, the encoder loop says index++ = high_short, then bit_count loop...
            // In Flipper-ARF:
            // for preamble_pairs { upload(high_short); upload(low_short); }
            // upload(high_short);
            // for bit in data { upload(bit ? high_short : low_short); upload(bit ? low_short : high_short); }
            // upload(high_short); upload(low_tail);

            for i in (0..64).rev() {
                let bit = (data >> i) & 1;
                if bit == 1 {
                    Self::add_level(&mut signal, true, TE_SHORT);
                    Self::add_level(&mut signal, false, TE_SHORT);
                } else {
                    Self::add_level(&mut signal, false, TE_SHORT);
                    Self::add_level(&mut signal, true, TE_SHORT);
                }
            }

            Self::add_level(&mut signal, true, TE_SHORT);
            Self::add_level(&mut signal, false, TAIL_GAP_US);
        }

        Some(signal)
    }
}

impl Default for KiaV7Decoder {
    fn default() -> Self {
        Self::new()
    }
}
