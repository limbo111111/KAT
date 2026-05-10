use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 350;
const TE_LONG: u32 = 750;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT_FOR_FOUND: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct FeronDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl FeronDecoder {
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

impl ProtocolDecoder for FeronDecoder {
    fn name(&self) -> &'static str {
        "Feron"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: TE_DELTA,
            min_count_bit: MIN_COUNT_BIT_FOR_FOUND,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration_us, TE_LONG * 6) < TE_DELTA * 4 {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.step = DecoderStep::SaveDuration;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration_us;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration_us, TE_LONG) < TE_DELTA {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                              duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration_us, TE_SHORT + 150) < TE_DELTA {
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(0);
                        } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.add_bit(1);
                        }

                        if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                            let data = self.decode_data;
                            let data_count_bit = self.decode_count_bit;

                            let decoded = DecodedSignal {
                                serial: None,
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data,
                                data_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            self.step = DecoderStep::Reset;

                            return Some(decoded);
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

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        for i in (0..decoded.data_count_bit).rev() {
            if (decoded.data >> i) & 1 == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                if i == 0 {
                    upload.push(LevelDuration::new(false, TE_SHORT + 150));
                    upload.push(LevelDuration::new(true, TE_SHORT + 150));
                    upload.push(LevelDuration::new(false, TE_LONG * 6));
                } else {
                    upload.push(LevelDuration::new(false, TE_SHORT));
                }
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                if i == 0 {
                    upload.push(LevelDuration::new(false, TE_SHORT + 150));
                    upload.push(LevelDuration::new(true, TE_SHORT + 150));
                    upload.push(LevelDuration::new(false, TE_LONG * 6));
                } else {
                    upload.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }

        Some(upload)
    }
}

impl Default for FeronDecoder {
    fn default() -> Self {
        Self::new()
    }
}
