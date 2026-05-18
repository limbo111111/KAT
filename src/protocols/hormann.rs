//! Hormann HSM protocol decoder
//!
//! Aligned with Flipper-ARF hormann.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 44;

const HORMANN_HSM_PATTERN: u64 = 0xFF000000003;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct HormannDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl HormannDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn check_pattern(data: u64) -> bool {
        (data & HORMANN_HSM_PATTERN) == HORMANN_HSM_PATTERN
    }
}

impl ProtocolDecoder for HormannDecoder {
    fn name(&self) -> &'static str {
        "Hormann HSM"
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
        self.te_last = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_SHORT * 24) < TE_DELTA * 24 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if !level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration >= TE_SHORT * 5 && Self::check_pattern(self.decode_data) {
                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit >= MIN_COUNT_BIT {
                            let btn = ((self.decode_data >> 8) & 0xF) as u8;

                            let result = DecodedSignal {
                                serial: None, // No serial documented specifically extracted
                                button: Some(btn),
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            return Some(result);
                        }
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.decode_data = (self.decode_data << 1) | 1;
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

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let repeat_count = 20;
        let mut out = Vec::with_capacity((decoded.data_count_bit * 2 + 2) * repeat_count + 1);

        let data = decoded.data;

        for _ in 0..repeat_count {
            // Send start bit
            out.push(LevelDuration::new(true, TE_SHORT * 24));
            out.push(LevelDuration::new(false, TE_SHORT));

            // Send key data
            for i in (1..=decoded.data_count_bit).rev() {
                if ((data >> (i - 1)) & 1) == 1 {
                    out.push(LevelDuration::new(true, TE_LONG));
                    out.push(LevelDuration::new(false, TE_SHORT));
                } else {
                    out.push(LevelDuration::new(true, TE_SHORT));
                    out.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }

        out.push(LevelDuration::new(true, TE_SHORT * 24));

        Some(out)
    }
}

impl Default for HormannDecoder {
    fn default() -> Self {
        Self::new()
    }
}
