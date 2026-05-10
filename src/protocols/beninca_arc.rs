use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 300;
const TE_LONG: u32 = 600;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Start,
    HighLevel,
    LowLevel,
}

pub struct BenincaArcDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_data_2: u64,
    decode_count_bit: usize,
}

impl BenincaArcDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Start,
            te_last: 0,
            decode_data: 0,
            decode_data_2: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for BenincaArcDecoder {
    fn name(&self) -> &'static str {
        "Beninca ARC"
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
        self.step = DecoderStep::Start;
        self.te_last = 0;
        self.decode_data = 0;
        self.decode_data_2 = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Start => {
                if !level && duration_diff!(duration, TE_LONG * 16) < TE_DELTA * 15 {
                    self.decode_data = 0;
                    self.decode_data_2 = 0;
                    self.decode_count_bit = 0;
                    self.step = DecoderStep::HighLevel;
                }
            }
            DecoderStep::HighLevel => {
                if level {
                    self.te_last = duration;
                    self.step = DecoderStep::LowLevel;
                    if self.decode_count_bit == MIN_COUNT_BIT / 2 && self.decode_data != 0 {
                        self.decode_data_2 = self.decode_data;
                        self.decode_data = 0;
                    } else if self.decode_count_bit == MIN_COUNT_BIT {
                        let data = self.decode_data_2;
                        let data_2 = self.decode_data;
                        let count_bit = self.decode_count_bit;

                        self.step = DecoderStep::Start;

                        let result = DecodedSignal {
                            serial: None,
                            button: None,
                            counter: None,
                            crc_valid: true,
                            data,
                            data_count_bit: count_bit,
                            encoder_capable: true,
                            extra: Some(data_2),
                            protocol_display_name: None,
                        };

                        return Some(result);
                    }
                } else {
                    self.step = DecoderStep::Start;
                }
            }
            DecoderStep::LowLevel => {
                if !level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA && duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::HighLevel;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::HighLevel;
                    } else {
                        self.step = DecoderStep::Start;
                    }
                } else {
                    self.step = DecoderStep::Start;
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

        let data = decoded.data;
        let data_2 = decoded.extra.unwrap_or(0);

        for i in (0..64).rev() {
            let bit = (data >> i) & 1;
            if bit == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        for i in (0..64).rev() {
            let bit = (data_2 >> i) & 1;
            if bit == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_LONG * 15));

        Some(upload)
    }
}

impl Default for BenincaArcDecoder {
    fn default() -> Self {
        Self::new()
    }
}
