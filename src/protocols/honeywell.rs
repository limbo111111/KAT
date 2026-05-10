//! Honeywell protocol decoder
//!
//! Aligned with Flipper-ARF honeywell.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 143;
const TE_LONG: u32 = 280;
const TE_DELTA: u32 = 51;
const MIN_COUNT_BIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ManchesterEvent {
    Reset,
    ShortLow,
    ShortHigh,
    LongLow,
    LongHigh,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ManchesterState {
    Start1,
    Mid1,
    Mid0,
    Start0,
}

pub struct HoneywellDecoder {
    manchester_state: ManchesterState,
    decode_data: u64,
    decode_count_bit: usize,
}

impl HoneywellDecoder {
    pub fn new() -> Self {
        Self {
            manchester_state: ManchesterState::Start1,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn manchester_advance(&mut self, event: ManchesterEvent) -> Option<bool> {
        let prev_state = self.manchester_state;
        let mut data = None;
        let mut data_ok = false;

        match (prev_state, event) {
            (ManchesterState::Start1, ManchesterEvent::ShortLow) => {
                self.manchester_state = ManchesterState::Mid1;
                data_ok = true;
            }
            (ManchesterState::Start1, ManchesterEvent::LongLow) => {
                self.manchester_state = ManchesterState::Mid0;
                data = Some(true);
                data_ok = true;
            }
            (ManchesterState::Mid1, ManchesterEvent::ShortHigh) => {
                self.manchester_state = ManchesterState::Start1;
                data = Some(true);
                data_ok = true;
            }
            (ManchesterState::Mid1, ManchesterEvent::LongHigh) => {
                self.manchester_state = ManchesterState::Start0;
                data = Some(true);
                data_ok = true;
            }
            (ManchesterState::Mid0, ManchesterEvent::ShortHigh) => {
                self.manchester_state = ManchesterState::Start0;
                data = Some(false);
                data_ok = true;
            }
            (ManchesterState::Mid0, ManchesterEvent::LongHigh) => {
                self.manchester_state = ManchesterState::Start1;
                data = Some(false);
                data_ok = true;
            }
            (ManchesterState::Start0, ManchesterEvent::ShortLow) => {
                self.manchester_state = ManchesterState::Mid0;
                data_ok = true;
            }
            (ManchesterState::Start0, ManchesterEvent::LongLow) => {
                self.manchester_state = ManchesterState::Mid1;
                data = Some(false);
                data_ok = true;
            }
            _ => {
                self.manchester_state = ManchesterState::Start1;
            }
        }

        if data_ok {
            data
        } else {
            None
        }
    }

    fn crc16(message: &[u8], init: u16, polynomial: u16) -> u16 {
        let mut remainder = init;
        for byte in message {
            remainder ^= (*byte as u16) << 8;
            for _ in 0..8 {
                if (remainder & 0x8000) != 0 {
                    remainder = (remainder << 1) ^ polynomial;
                } else {
                    remainder <<= 1;
                }
            }
        }
        remainder
    }
}

impl ProtocolDecoder for HoneywellDecoder {
    fn name(&self) -> &'static str {
        "Honeywell Sec"
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
        &[315_000_000, 433_920_000, 868_350_000]
    }

    fn reset(&mut self) {
        self.manchester_state = ManchesterState::Start1;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut event = ManchesterEvent::Reset;

        if !level {
            if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                event = ManchesterEvent::ShortLow;
            } else if duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                event = ManchesterEvent::LongLow;
            }
        } else {
            if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                event = ManchesterEvent::ShortHigh;
            } else if duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                event = ManchesterEvent::LongHigh;
            }
        }

        if event != ManchesterEvent::Reset {
            if let Some(data_bit) = self.manchester_advance(event) {
                self.decode_data = (self.decode_data << 1) | (data_bit as u64);
                self.decode_count_bit += 1;

                if self.decode_count_bit >= 62 && self.decode_count_bit < 64 {
                    let preamble = ((self.decode_data >> 48) & 0xFFFF) as u16;

                    if preamble == 0x3FFE || preamble == 0x7FFE || preamble == 0xFFFE {
                        let datatocrc = [
                            ((self.decode_data >> 40) & 0xFF) as u8,
                            ((self.decode_data >> 32) & 0xFF) as u8,
                            ((self.decode_data >> 24) & 0xFF) as u8,
                            ((self.decode_data >> 16) & 0xFF) as u8,
                        ];

                        let channel = ((self.decode_data >> 44) & 0xF) as u8;
                        let crc_calc = if channel == 0x2 || channel == 0x4 || channel == 0xA {
                            Self::crc16(&datatocrc, 0, 0x8050)
                        } else if channel == 0x8 {
                            Self::crc16(&datatocrc, 0, 0x8005)
                        } else {
                            0xFFFF // Force mismatch
                        };

                        let crc = (self.decode_data & 0xFFFF) as u16;

                        if crc == crc_calc {
                            // Normalize data
                            let mut data = self.decode_data;
                            data = ((((((0xFFu64 << 16) | ((data >> 40) & 0xFFFF)) << 16) |
                                     ((data >> 24) & 0xFFFF)) << 16) |
                                   ((data >> 8) & 0xFFFF)) << 8 | (data & 0xFF);

                            let serial = ((data >> 24) & 0xFFFFF) as u32;

                            let result = DecodedSignal {
                                serial: Some(serial),
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data,
                                data_count_bit: 64,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            return Some(result);
                        } else {
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                        }
                    }
                } else if self.decode_count_bit >= 64 {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                }
            }
        } else {
            self.decode_data = 0;
            self.decode_count_bit = 0;
            self.manchester_state = ManchesterState::Start1;
        }

        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut out = Vec::with_capacity(64 * 2 + 10);
        let mut enc_state = ManchesterState::Start1;

        let advance = |state: &mut ManchesterState, bit: bool| -> Option<ManchesterEvent> {
            match (*state, bit) {
                (ManchesterState::Start1, true) => {
                    *state = ManchesterState::Mid1;
                    Some(ManchesterEvent::ShortLow)
                }
                (ManchesterState::Start1, false) => {
                    *state = ManchesterState::Mid0;
                    Some(ManchesterEvent::LongLow)
                }
                (ManchesterState::Mid1, true) => {
                    *state = ManchesterState::Start1;
                    Some(ManchesterEvent::ShortHigh)
                }
                (ManchesterState::Mid1, false) => {
                    *state = ManchesterState::Start0;
                    Some(ManchesterEvent::LongHigh)
                }
                (ManchesterState::Mid0, true) => {
                    *state = ManchesterState::Start0;
                    Some(ManchesterEvent::ShortHigh)
                }
                (ManchesterState::Mid0, false) => {
                    *state = ManchesterState::Start1;
                    Some(ManchesterEvent::LongHigh)
                }
                (ManchesterState::Start0, true) => {
                    *state = ManchesterState::Mid0;
                    Some(ManchesterEvent::ShortLow)
                }
                (ManchesterState::Start0, false) => {
                    *state = ManchesterState::Mid1;
                    Some(ManchesterEvent::LongLow)
                }
            }
        };

        let to_level_dur = |event: ManchesterEvent| -> LevelDuration {
            match event {
                ManchesterEvent::ShortLow => LevelDuration::new(false, TE_SHORT),
                ManchesterEvent::LongLow => LevelDuration::new(false, TE_LONG),
                ManchesterEvent::ShortHigh => LevelDuration::new(true, TE_SHORT),
                ManchesterEvent::LongHigh => LevelDuration::new(true, TE_LONG),
                _ => LevelDuration::new(false, TE_SHORT),
            }
        };

        for i in (1..=64).rev() {
            let bit = ((decoded.data >> (i - 1)) & 1) == 1;

            if let Some(event) = advance(&mut enc_state, bit) {
                out.push(to_level_dur(event));
            } else {
                // If it returned None, we need to advance again
                if let Some(event) = advance(&mut enc_state, bit) {
                    out.push(to_level_dur(event));
                }
            }
        }

        let final_event = match enc_state {
            ManchesterState::Mid1 => ManchesterEvent::ShortHigh,
            ManchesterState::Mid0 => ManchesterEvent::ShortHigh,
            ManchesterState::Start1 => ManchesterEvent::LongHigh,
            ManchesterState::Start0 => ManchesterEvent::ShortLow,
        };
        out.push(to_level_dur(final_event));

        if let Some(last) = out.last() {
            if last.level {
                // Flipper code: "if(level_duration_get_level(instance->encoder.upload[index])) { index++; }"
                out.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        out.push(LevelDuration::new(false, TE_LONG * 300));
        Some(out)
    }
}

impl Default for HoneywellDecoder {
    fn default() -> Self {
        Self::new()
    }
}
