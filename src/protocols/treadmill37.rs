use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 300;
const TE_LONG: u32 = 900;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 37;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct Treadmill37Decoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl Treadmill37Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for Treadmill37Decoder {
    fn name(&self) -> &'static str {
        "Treadmill37"
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
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 20) < TE_DELTA * 4 {
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
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_SHORT * 20) < TE_DELTA * 4 {
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.decode_data <<= 1;
                            self.decode_count_bit += 1;
                        }
                        if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.decode_data = (self.decode_data << 1) | 1;
                            self.decode_count_bit += 1;
                        }

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let serial = (self.decode_data >> 17) as u32;
                            let cnt = ((self.decode_data >> 1) & 0xFFFF) as u16;

                            let result = DecodedSignal {
                                serial: Some(serial),
                                button: None,
                                counter: Some(cnt),
                                crc_valid: true,
                                data: self.decode_data,
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
        let mut out = Vec::new();
        let data = decoded.data;

        for i in (1..=decoded.data_count_bit).rev() {
            if (data >> (i - 1)) & 1 == 1 {
                out.push(LevelDuration::new(true, TE_LONG));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 20));
                } else {
                    out.push(LevelDuration::new(false, TE_SHORT));
                }
            } else {
                out.push(LevelDuration::new(true, TE_SHORT));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 20));
                } else {
                    out.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }
        Some(out)
    }
}

impl Default for Treadmill37Decoder {
    fn default() -> Self {
        Self::new()
    }
}
