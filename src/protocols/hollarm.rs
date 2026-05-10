//! Hollarm protocol decoder
//!
//! Aligned with Flipper-ARF hollarm.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 200;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 42;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct HollarmDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl HollarmDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for HollarmDecoder {
    fn name(&self) -> &'static str {
        "Hollarm"
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
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 12) < TE_DELTA * 2 {
                    // Found GAP
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        // Bit 0: short HIGH, long LOW
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT * 8) < TE_DELTA
                    {
                        // Bit 1: short HIGH, short*8 LOW
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_SHORT * 12) < TE_DELTA {
                        // GAP / End of key
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let data = self.decode_data >> 2;

                            let bytesum = ((data >> 32) & 0xFF) as u8 +
                                          ((data >> 24) & 0xFF) as u8 +
                                          ((data >> 16) & 0xFF) as u8 +
                                          ((data >> 8) & 0xFF) as u8;

                            if bytesum == (data & 0xFF) as u8 {
                                let btn = ((data >> 8) & 0xF) as u8;
                                let serial = ((data & 0xFFFFFFF0000) >> 16) as u32;

                                let result = DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn),
                                    counter: None,
                                    crc_valid: true,
                                    data,
                                    data_count_bit: self.decode_count_bit,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: None,
                                };

                                self.decode_data = 0;
                                self.decode_count_bit = 0;
                                self.step = DecoderStep::Reset;

                                return Some(result);
                            }
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
        let mut new_key = (decoded.data >> 12) << 12 | ((button as u64) << 8);
        let bytesum = ((new_key >> 32) & 0xFF) as u8 +
                      ((new_key >> 24) & 0xFF) as u8 +
                      ((new_key >> 16) & 0xFF) as u8 +
                      ((new_key >> 8) & 0xFF) as u8;
        new_key |= bytesum as u64;

        let mut out = Vec::new();

        for i in (1..=decoded.data_count_bit).rev() {
            let bit = ((new_key << 2) >> (i - 1)) & 1;
            if bit == 1 {
                out.push(LevelDuration::new(true, TE_SHORT));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 12));
                } else {
                    out.push(LevelDuration::new(false, TE_SHORT * 8));
                }
            } else {
                out.push(LevelDuration::new(true, TE_SHORT));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 12));
                } else {
                    out.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }

        Some(out)
    }
}

impl Default for HollarmDecoder {
    fn default() -> Self {
        Self::new()
    }
}
