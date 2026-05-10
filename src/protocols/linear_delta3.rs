//! Linear Delta3 protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/linear_delta3.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 2000;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 8;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct LinearDelta3Decoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    last_data: u64,
}

impl LinearDelta3Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            last_data: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for LinearDelta3Decoder {
    fn name(&self) -> &'static str {
        "Linear Delta3"
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
        &[315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_last = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.last_data = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 70) < TE_DELTA * 24 {
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
                    if duration >= TE_SHORT * 10 {
                        self.step = DecoderStep::Reset;

                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(1);
                        } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.add_bit(0);
                        }

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.last_data != 0 && self.last_data == self.decode_data {
                                let decoded_sig = DecodedSignal {
                                    serial: None,
                                    button: None,
                                    counter: None,
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: self.decode_count_bit,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: None,
                                };

                                self.step = DecoderStep::SaveDuration;
                                self.last_data = self.decode_data;
                                return Some(decoded_sig);
                            }

                            self.step = DecoderStep::SaveDuration;
                            self.last_data = self.decode_data;
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            return None;
                        }
                        return None;
                    }

                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_SHORT * 7) < TE_DELTA * 2 {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                              duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.add_bit(0);
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
        let mut upload = Vec::new();

        for i in (1..decoded.data_count_bit).rev() {
            if ((decoded.data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT * 7));
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        if (decoded.data & 1) == 1 {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_SHORT * 73));
        } else {
            upload.push(LevelDuration::new(true, TE_LONG));
            upload.push(LevelDuration::new(false, TE_SHORT * 70));
        }

        Some(upload)
    }
}

impl Default for LinearDelta3Decoder {
    fn default() -> Self {
        Self::new()
    }
}
