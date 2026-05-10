//! Hyundai Santa Fe 2013-2016 protocol decoder
//!
//! Aligned with Flipper-ARF reference: `auto_rke_protocols.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz AM/OOK
//! - 80 bits (MSB first)
//! - PWM: period 500 µs; 1 = 375 µs HI + 125 µs LO; 0 = 125 µs HI + 375 µs LO
//! - Sync: 375 µs HI + 12000 µs LO
//! - Gap: 15000 µs
//! - Fields: [79:48] rolling, [47:24] 24-bit serial, [23:16] counter, [15:8] button, [7:0] CRC8
//! - CRC8 poly 0x31, init 0xFF

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 125;
const TE_LONG: u32 = 375;
const TE_DELTA: u32 = 100;
const SYNC_US: u32 = 12000;
const SYNC_DELTA: u32 = 1800; // 15% tolerance
const MIN_COUNT_BIT: usize = 80;
const GAP_US: u32 = 15000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckSyncHi,
    SaveDuration,
    CheckDuration,
}

pub struct SantaFeDecoder {
    step: DecoderStep,
    te_last: u32,
    data: [u8; 10], // 80 bits
    bit_count: usize,
}

impl SantaFeDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            data: [0u8; 10],
            bit_count: 0,
        }
    }

    fn add_bit(&mut self, bit: bool) {
        if self.bit_count < 80 {
            let byte_idx = self.bit_count / 8;
            let bit_idx = 7 - (self.bit_count % 8);
            if bit {
                self.data[byte_idx] |= 1 << bit_idx;
            }
            self.bit_count += 1;
        }
    }

    fn crc8(data: &[u8], len: usize) -> u8 {
        let mut crc: u8 = 0xFF;
        for i in 0..len {
            crc ^= data[i];
            for _ in 0..8 {
                if (crc & 0x80) != 0 {
                    crc = (crc << 1) ^ 0x31;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }

    fn process_data(&self) -> Option<DecodedSignal> {
        if self.bit_count < MIN_COUNT_BIT {
            return None;
        }

        let rx_crc = self.data[9];
        let calc_crc = Self::crc8(&self.data, 9);

        if rx_crc != calc_crc {
            return None;
        }

        let mut rolling: u32 = 0;
        rolling |= (self.data[0] as u32) << 24;
        rolling |= (self.data[1] as u32) << 16;
        rolling |= (self.data[2] as u32) << 8;
        rolling |= self.data[3] as u32;

        let mut serial: u32 = 0;
        serial |= (self.data[4] as u32) << 16;
        serial |= (self.data[5] as u32) << 8;
        serial |= self.data[6] as u32;

        let counter = self.data[7];
        let button = self.data[8];

        let mut button_name = None;
        if button == 0x01 { button_name = Some("Lock".to_string()); }
        if button == 0x02 { button_name = Some("Unlock".to_string()); }
        if button == 0x04 { button_name = Some("Trunk".to_string()); }
        if button == 0x08 { button_name = Some("Panic".to_string()); }

        let mut packed: u64 = 0;
        for i in 0..8 {
            packed = (packed << 8) | (self.data[i] as u64);
        }

        let mut signal = DecodedSignal {
            serial: Some(serial),
            button: Some(button),
            counter: Some(counter as u16),
            crc_valid: true,
            data: packed, // returning first 64 bits as 'data' since struct is u64
            data_count_bit: 80,
            encoder_capable: true,
            extra: Some(rolling as u64),
            protocol_display_name: Some("SantaFe 13-16".to_string()),
        };

        if let Some(name) = button_name {
            signal.protocol_display_name = Some(format!("SantaFe ({})", name));
        }

        Some(signal)
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

impl ProtocolDecoder for SantaFeDecoder {
    fn name(&self) -> &'static str {
        "SantaFe 13-16"
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
        self.te_last = 0;
        self.data = [0u8; 10];
        self.bit_count = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_LONG) < TE_DELTA {
                    self.step = DecoderStep::CheckSyncHi;
                    self.te_last = duration;
                }
            }

            DecoderStep::CheckSyncHi => {
                if !level && duration_diff!(duration, SYNC_US) < SYNC_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.data = [0u8; 10];
                    self.bit_count = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }

            DecoderStep::SaveDuration => {
                if level {
                    if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.add_bit(true);
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    } else if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.add_bit(false);
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    } else if duration > 3000 {
                        if self.bit_count >= 80 {
                            let res = self.process_data();
                            self.step = DecoderStep::Reset;
                            return res;
                        }
                        self.step = DecoderStep::Reset;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }

            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA || duration_diff!(duration, TE_LONG) < TE_DELTA {
                        if self.bit_count >= 80 {
                            let res = self.process_data();
                            self.step = DecoderStep::Reset;
                            return res;
                        }
                        self.step = DecoderStep::SaveDuration;
                    } else if duration > 3000 {
                        if self.bit_count >= 80 {
                            let res = self.process_data();
                            self.step = DecoderStep::Reset;
                            return res;
                        }
                        self.step = DecoderStep::Reset;
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
        let counter = decoded.counter.unwrap_or(0) as u8;

        let rolling = decoded.extra.unwrap_or(0) as u32;

        let mut pkt = [0u8; 10];
        pkt[0] = (rolling >> 24) as u8;
        pkt[1] = (rolling >> 16) as u8;
        pkt[2] = (rolling >> 8) as u8;
        pkt[3] = rolling as u8;

        pkt[4] = (serial >> 16) as u8;
        pkt[5] = (serial >> 8) as u8;
        pkt[6] = serial as u8;

        pkt[7] = counter;
        pkt[8] = button;
        pkt[9] = Self::crc8(&pkt, 9);

        let mut signal = Vec::new();

        for rep in 0..3 {
            if rep > 0 {
                Self::add_level(&mut signal, false, GAP_US);
            }

            Self::add_level(&mut signal, true, TE_LONG);
            Self::add_level(&mut signal, false, SYNC_US);

            for byte in 0..10 {
                for bit in (0..8).rev() {
                    let b = (pkt[byte] >> bit) & 1;
                    if b == 1 {
                        Self::add_level(&mut signal, true, TE_LONG);
                        Self::add_level(&mut signal, false, TE_SHORT);
                    } else {
                        Self::add_level(&mut signal, true, TE_SHORT);
                        Self::add_level(&mut signal, false, TE_LONG);
                    }
                }
            }
        }

        Some(signal)
    }
}

impl Default for SantaFeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
