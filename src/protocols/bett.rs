use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 340;
const TE_LONG: u32 = 2000;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 18;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct BettDecoder {
    step: DecoderStep,
    decode_data: u64,
    decode_count_bit: usize,
}

impl BettDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for BettDecoder {
    fn name(&self) -> &'static str {
        "BETT"
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
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 44) < TE_DELTA * 15 {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.step = DecoderStep::CheckDuration;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration_diff!(duration, TE_SHORT * 44) < TE_DELTA * 15 {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let data = self.decode_data;
                            let count_bit = self.decode_count_bit;

                            self.step = DecoderStep::Reset;
                            self.decode_data = 0;
                            self.decode_count_bit = 0;

                            return Some(DecodedSignal {
                                serial: None,
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data,
                                data_count_bit: count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        } else {
                            self.step = DecoderStep::Reset;
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        if duration_diff!(duration, TE_SHORT) < TE_DELTA || duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                            self.step = DecoderStep::CheckDuration;
                        } else {
                            self.step = DecoderStep::Reset;
                        }
                    }
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_SHORT) < TE_DELTA {
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

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();
        let data = decoded.data;
        let count_bit = decoded.data_count_bit;

        for i in (1..count_bit).rev() {
            let bit = (data >> i) & 1;
            if bit == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        let bit_0 = data & 1;
        if bit_0 == 1 {
            upload.push(LevelDuration::new(true, TE_LONG));
            upload.push(LevelDuration::new(false, TE_SHORT + TE_LONG * 7));
        } else {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_LONG + TE_LONG * 7));
        }

        Some(upload)
    }
}

impl Default for BettDecoder {
    fn default() -> Self {
        Self::new()
    }
}
