use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 300;
const TE_LONG: u32 = 900;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct Smc5326Decoder {
    step: DecoderStep,
    te_last: u32,
    te_sum: u32,
    decode_data: u64,
    decode_count_bit: usize,
    last_data: u64,
}

impl Smc5326Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            te_sum: 0,
            decode_data: 0,
            decode_count_bit: 0,
            last_data: 0,
        }
    }
}

impl ProtocolDecoder for Smc5326Decoder {
    fn name(&self) -> &'static str {
        "SMC5326"
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
                if !level && duration_diff!(duration, TE_SHORT * 24) < TE_DELTA * 12 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.te_sum = 0;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.te_sum += duration;
                    self.step = DecoderStep::CheckDuration;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration >= TE_LONG * 2 {
                        self.step = DecoderStep::SaveDuration;
                        let mut result = None;

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            if self.last_data == self.decode_data && self.last_data != 0 {
                                let te_avg = self.te_sum / (self.decode_count_bit as u32 * 4 + 1);
                                result = Some(DecodedSignal {
                                    serial: None,
                                    button: None,
                                    counter: None,
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: self.decode_count_bit,
                                    encoder_capable: true,
                                    extra: Some(te_avg as u64),
                                    protocol_display_name: None,
                                });
                            }
                            self.last_data = self.decode_data;
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.te_sum = 0;
                        return result;
                    }

                    self.te_sum += duration;

                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA * 3
                    {
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3
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
        let te = match decoded.extra {
            Some(t) => t as u32,
            None => TE_SHORT,
        };
        let data = decoded.data;
        let data_count_bit = decoded.data_count_bit;

        let mut out = Vec::new();

        for i in (0..data_count_bit).rev() {
            if (data >> i) & 1 == 1 {
                out.push(LevelDuration::new(true, te * 3));
                out.push(LevelDuration::new(false, te));
            } else {
                out.push(LevelDuration::new(true, te));
                out.push(LevelDuration::new(false, te * 3));
            }
        }

        // Stop bit
        out.push(LevelDuration::new(true, te));
        // PT_GUARD
        out.push(LevelDuration::new(false, te * 25));

        Some(out)
    }
}

impl Default for Smc5326Decoder {
    fn default() -> Self {
        Self::new()
    }
}
