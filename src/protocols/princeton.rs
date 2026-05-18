//! Princeton protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/princeton.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz AM, 24 bits
//! - TE ~390us short, 1170us long
//! - Bit 0: high for te, low for te*3
//! - Bit 1: high for te*3, low for te
//! - Preamble: low for te*36

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 390;
const TE_LONG: u32 = 1170;
const TE_DELTA: u32 = 300;
const MIN_COUNT_BIT: usize = 24;
const PRINCETON_GUARD_TIME_DEFAULT: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct PrincetonDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    te: u32,
    guard_time: u32,
    last_data: u64,
}

impl PrincetonDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            te: 0,
            guard_time: PRINCETON_GUARD_TIME_DEFAULT,
            last_data: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for PrincetonDecoder {
    fn name(&self) -> &'static str {
        "Princeton"
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
        &[433_920_000, 868_350_000, 315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.last_data = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 36) < TE_DELTA * 36 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.te = 0;
                    self.guard_time = PRINCETON_GUARD_TIME_DEFAULT;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.te += duration;
                    self.step = DecoderStep::CheckDuration;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration >= TE_LONG * 2 {
                        self.step = DecoderStep::SaveDuration;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.last_data == self.decode_data && self.last_data != 0 {
                                self.te /= (self.decode_count_bit as u32 * 4) + 1;

                                let mut guard_time =
                                    (duration as f32 / self.te as f32).round() as u32;
                                if !(15..=72).contains(&guard_time) {
                                    guard_time = PRINCETON_GUARD_TIME_DEFAULT;
                                }
                                self.guard_time = guard_time;

                                let data = self.decode_data;
                                let bit_count = self.decode_count_bit;

                                let (serial, btn) =
                                    if (data & 0xFF) == 0x30 || (data & 0xFF) == 0xC0 {
                                        ((data >> 8) as u32, (data & 0xFF) as u8)
                                    } else if (data & 0xFF) == 0x03 || (data & 0xFF) == 0x0C {
                                        ((data >> 8) as u32, ((data & 0xFF) | 0xF0) as u8)
                                    } else {
                                        ((data >> 4) as u32, (data & 0xF) as u8)
                                    };

                                let signal = DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn),
                                    counter: None,
                                    crc_valid: true,
                                    data,
                                    data_count_bit: bit_count,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: None,
                                };

                                self.decode_data = 0;
                                self.decode_count_bit = 0;
                                self.te = 0;
                                self.last_data = data;
                                return Some(signal);
                            }
                            self.last_data = self.decode_data;
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.te = 0;
                        return None;
                    }

                    self.te += duration;

                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA * 3
                    {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.add_bit(1);
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
        let data;
        let serial = decoded.serial.unwrap_or(0);
        let mut btn = button;

        let original_btn = decoded.button.unwrap_or(0);

        // Simple mapping based on the original button
        // In the original C code this uses the "subghz_custom_btn_get" overrides
        // to map UP/DOWN/LEFT/RIGHT depending on the original button code.
        // For the rust framework we just take the requested button or fallback.
        if btn == 0 {
            btn = original_btn;
        }

        if btn == 0x30 || btn == 0xC0 {
            data = ((serial as u64) << 8) | (btn as u64);
        } else if btn == 0xF3 || btn == 0xFC {
            data = ((serial as u64) << 8) | ((btn & 0xF) as u64);
        } else {
            data = ((serial as u64) << 4) | (btn as u64);
        }

        let mut signal = Vec::with_capacity((decoded.data_count_bit * 2) + 2);

        let te = if self.te > 0 { self.te } else { TE_SHORT };
        let guard_time = if self.guard_time > 0 {
            self.guard_time
        } else {
            PRINCETON_GUARD_TIME_DEFAULT
        };

        for i in (0..decoded.data_count_bit).rev() {
            if (data >> i) & 1 == 1 {
                signal.push(LevelDuration::new(true, te * 3));
                signal.push(LevelDuration::new(false, te));
            } else {
                signal.push(LevelDuration::new(true, te));
                signal.push(LevelDuration::new(false, te * 3));
            }
        }

        signal.push(LevelDuration::new(true, te));
        signal.push(LevelDuration::new(false, te * guard_time));

        Some(signal)
    }
}

impl Default for PrincetonDecoder {
    fn default() -> Self {
        Self::new()
    }
}
