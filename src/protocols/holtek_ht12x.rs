//! Holtek HT12X protocol decoder
//!
//! Aligned with Flipper-ARF holtek_ht12x.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 320;
const TE_LONG: u32 = 640;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct HoltekHt12xDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    te: u32,
    last_data: u32,
}

impl HoltekHt12xDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            te: 0,
            last_data: 0,
        }
    }
}

impl ProtocolDecoder for HoltekHt12xDecoder {
    fn name(&self) -> &'static str {
        "Holtek_HT12X"
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
        self.te_last = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 28) < TE_DELTA * 20 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.te = duration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= TE_SHORT * 10 + TE_DELTA {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.last_data == (self.decode_data as u32) && self.last_data != 0 {
                                self.te /= (self.decode_count_bit as u32 * 3) + 1;

                                let btn = (self.decode_data & 0x0F) as u8;
                                let cnt = ((self.decode_data >> 4) & 0xFF) as u16;

                                let result = DecodedSignal {
                                    serial: None,
                                    button: Some(btn),
                                    counter: Some(cnt),
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: self.decode_count_bit,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: None,
                                };

                                self.last_data = self.decode_data as u32;
                                self.decode_data = 0;
                                self.decode_count_bit = 0;
                                self.te = 0;
                                self.step = DecoderStep::FoundStartBit;
                                return Some(result);
                            }
                            self.last_data = self.decode_data as u32;
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.te = 0;
                        self.step = DecoderStep::FoundStartBit;
                    } else {
                        self.te_last = duration;
                        self.te += duration;
                        self.step = DecoderStep::CheckDuration;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    self.te += duration;
                    if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA * 2
                    {
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
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
        let mut out = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        let te = if self.te > 0 { self.te } else { TE_SHORT };

        // Send header
        out.push(LevelDuration::new(false, te * 36));
        // Send start bit
        out.push(LevelDuration::new(true, te));

        let data = (decoded.data & 0xFFFFFFF0) | (button as u64 & 0x0F);

        for i in (1..=decoded.data_count_bit).rev() {
            if ((data >> (i - 1)) & 1) == 1 {
                out.push(LevelDuration::new(false, te * 2));
                out.push(LevelDuration::new(true, te));
            } else {
                out.push(LevelDuration::new(false, te));
                out.push(LevelDuration::new(true, te * 2));
            }
        }

        Some(out)
    }
}

impl Default for HoltekHt12xDecoder {
    fn default() -> Self {
        Self::new()
    }
}
