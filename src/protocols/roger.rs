//! Roger protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/roger.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz AM, 28 bits
//! - TE ~500us short, 1000us long
//! - Bit 0: high for te_short, low for te_long
//! - Bit 1: high for te_long, low for te_short
//! - GAP: low for te_short * 19

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 270;
const MIN_COUNT_BIT: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct RogerDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl RogerDecoder {
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
}

impl ProtocolDecoder for RogerDecoder {
    fn name(&self) -> &'static str {
        "Roger"
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
        &[433_920_000, 868_350_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 19) < TE_DELTA * 5 {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.step = DecoderStep::SaveDuration;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_SHORT * 19) < TE_DELTA * 5 {
                        if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.add_bit(1);
                        }
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(0);
                        }

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let data = self.decode_data;
                            let bit_count = self.decode_count_bit;

                            let serial = (data >> 12) as u32;
                            let btn = ((data >> 8) & 0xF) as u8;

                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            self.step = DecoderStep::Reset;

                            return Some(DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: None,
                                crc_valid: true,
                                data,
                                data_count_bit: bit_count,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
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
        let mut btn = button;

        let original_btn = decoded.button.unwrap_or(0);
        if btn == 0 {
            btn = original_btn;
        }

        let mut data = decoded.data;

        if (data & 0xFF) == original_btn as u64 {
            data = ((serial as u64) << 12) | ((btn as u64) << 8) | (btn as u64);
        } else if (data & 0xFF) == 0x23 && btn == 0x1 {
            data = ((serial as u64) << 12) | ((btn as u64) << 8) | 0x20;
        } else if (data & 0xFF) == 0x20 && btn == 0x2 {
            data = ((serial as u64) << 12) | ((btn as u64) << 8) | 0x23;
        }

        let mut signal = Vec::with_capacity((decoded.data_count_bit * 2));

        for i in (0..decoded.data_count_bit).rev() {
            if (data >> i) & 1 == 1 {
                signal.push(LevelDuration::new(true, TE_LONG));
                if i == 0 {
                    signal.push(LevelDuration::new(false, TE_SHORT * 19));
                } else {
                    signal.push(LevelDuration::new(false, TE_SHORT));
                }
            } else {
                signal.push(LevelDuration::new(true, TE_SHORT));
                if i == 0 {
                    signal.push(LevelDuration::new(false, TE_SHORT * 19));
                } else {
                    signal.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }

        Some(signal)
    }
}

impl Default for RogerDecoder {
    fn default() -> Self {
        Self::new()
    }
}
