//! Hyundai / Kia RIO protocol decoder
//!
//! Aligned with Flipper-ARF reference: `auto_rke_protocols.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz AM/OOK
//! - 64 bits (MSB first)
//! - PWM: period 1040 µs; 1 = 728 µs HI + 312 µs LO; 0 = 312 µs HI + 728 µs LO
//! - Sync: 312 µs HI + 10400 µs LO
//! - Gap: 10000 µs
//! - Fields: [63:32] 32-bit serial, [31:16] 16-bit button mask, [15:0] 16-bit checksum
//! - Button codes: 0x0100=Lock, 0x0200=Unlock, 0x0400=Trunk, 0x0800=Panic

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 312;
const TE_LONG: u32 = 728;
const TE_DELTA: u32 = 150;
const SYNC_US: u32 = 10400;
const SYNC_DELTA: u32 = 1560; // 15% tolerance
const MIN_COUNT_BIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckSyncHi,
    CheckSyncLo,
    SaveDuration,
    CheckDuration,
}

pub struct HyundaiKiaRioDecoder {
    step: DecoderStep,
    te_last: u32,
    data: u64,
    bit_count: usize,
}

impl HyundaiKiaRioDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            data: 0,
            bit_count: 0,
        }
    }

    fn add_bit(&mut self, bit: bool) {
        self.data <<= 1;
        if bit {
            self.data |= 1;
        }
        self.bit_count += 1;
    }

    fn check_checksum(serial: u32, button_mask: u16, rx_ck: u16) -> bool {
        let mut c: u16 = (serial ^ (serial >> 16)) as u16;
        c ^= button_mask;
        rx_ck == (!c)
    }

    fn process_data(&self) -> Option<DecodedSignal> {
        if self.bit_count < MIN_COUNT_BIT {
            return None;
        }

        let serial = (self.data >> 32) as u32;
        let button_mask = ((self.data >> 16) & 0xFFFF) as u16;
        let rx_ck = (self.data & 0xFFFF) as u16;

        if !Self::check_checksum(serial, button_mask, rx_ck) {
            return None;
        }

        // Map button_mask to common button codes
        let button = match button_mask {
            0x0100 => 1, // Lock
            0x0200 => 2, // Unlock
            0x0400 => 4, // Trunk
            0x0800 => 8, // Panic
            _ => (button_mask >> 8) as u8, // fallback to upper byte
        };

        let mut button_name = None;
        if button_mask == 0x0100 { button_name = Some("Lock".to_string()); }
        if button_mask == 0x0200 { button_name = Some("Unlock".to_string()); }
        if button_mask == 0x0400 { button_name = Some("Trunk".to_string()); }
        if button_mask == 0x0800 { button_name = Some("Panic".to_string()); }

        let mut signal = DecodedSignal {
            serial: Some(serial),
            button: Some(button),
            counter: None, // No rolling counter
            crc_valid: true,
            data: self.data,
            data_count_bit: 64,
            encoder_capable: true,
            extra: None,
            protocol_display_name: Some("Hyundai/Kia RIO".to_string()),
        };

        // The `extra` field is Option<u64> used for state. We don't have extra state to store.
        if let Some(name) = button_name {
            signal.protocol_display_name = Some(format!("HKR ({})", name));
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

impl ProtocolDecoder for HyundaiKiaRioDecoder {
    fn name(&self) -> &'static str {
        "Hyundai/Kia RIO"
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
        self.data = 0;
        self.bit_count = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::CheckSyncHi;
                    self.te_last = duration;
                }
            }

            DecoderStep::CheckSyncHi => {
                if !level && duration_diff!(duration, SYNC_US) < SYNC_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.data = 0;
                    self.bit_count = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }

            DecoderStep::CheckSyncLo => {
                // Not used in this particular state machine structure,
                // but included if we wanted to split Sync into 2 states.
                self.step = DecoderStep::Reset;
            }

            DecoderStep::SaveDuration => {
                if level {
                    if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        // Long HIGH = bit 1
                        self.add_bit(true);
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    } else if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        // Short HIGH = bit 0
                        self.add_bit(false);
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    } else if duration > 3000 {
                        // EOT
                        if self.bit_count >= 64 {
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
                        if self.bit_count >= 64 {
                            let res = self.process_data();
                            self.step = DecoderStep::Reset;
                            return res;
                        }
                        self.step = DecoderStep::SaveDuration;
                    } else if duration > 3000 {
                        if self.bit_count >= 64 {
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

        let button_mask = match button {
            1 => 0x0100, // Lock
            2 => 0x0200, // Unlock
            4 => 0x0400, // Trunk
            8 => 0x0800, // Panic
            _ => (button as u16) << 8,
        };

        let mut c: u16 = (serial ^ (serial >> 16)) as u16;
        c ^= button_mask;
        let ck = !c;

        let word = ((serial as u64) << 32) | ((button_mask as u64) << 16) | (ck as u64);

        let mut signal = Vec::new();

        for rep in 0..3 {
            if rep > 0 {
                Self::add_level(&mut signal, false, 10000); // GAP
            }

            Self::add_level(&mut signal, true, TE_SHORT);
            Self::add_level(&mut signal, false, SYNC_US);

            for b in (0..64).rev() {
                let bit = (word >> b) & 1;
                if bit == 1 {
                    Self::add_level(&mut signal, true, TE_LONG);
                    Self::add_level(&mut signal, false, TE_SHORT);
                } else {
                    Self::add_level(&mut signal, true, TE_SHORT);
                    Self::add_level(&mut signal, false, TE_LONG);
                }
            }
        }

        Some(signal)
    }
}

impl Default for HyundaiKiaRioDecoder {
    fn default() -> Self {
        Self::new()
    }
}
