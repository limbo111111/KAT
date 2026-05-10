use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 400;
const TE_LONG: u32 = 1100;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT_FOR_FOUND: usize = 37;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct DoitrandDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl DoitrandDecoder {
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

impl ProtocolDecoder for DoitrandDecoder {
    fn name(&self) -> &'static str {
        "Doitrand"
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
                if !level && duration_diff!(duration_us, TE_SHORT * 62) < TE_DELTA * 30 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration_us, TE_SHORT * 2) < TE_DELTA * 3 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration_us >= TE_SHORT * 10 + TE_DELTA {
                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                            let data = self.decode_data;
                            let data_count_bit = self.decode_count_bit;
                            let cnt = ((data >> 24) | ((data >> 15) & 0x1)) as u16;
                            let btn = ((data >> 18) & 0x3) as u8;

                            let decoded = DecodedSignal {
                                serial: None,
                                button: Some(btn),
                                counter: Some(cnt),
                                crc_valid: true,
                                data,
                                data_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            self.decode_data = 0;
                            self.decode_count_bit = 0;

                            return Some(decoded);
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.te_last = duration_us;
                        self.step = DecoderStep::CheckDuration;
                    }
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration_us, TE_LONG) < TE_DELTA * 3 {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3 &&
                              duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.add_bit(1);
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
        let mut upload = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        upload.push(LevelDuration::new(false, TE_SHORT * 62));
        upload.push(LevelDuration::new(true, TE_SHORT * 2 - 100));

        for i in (0..decoded.data_count_bit).rev() {
            if (decoded.data >> i) & 1 == 1 {
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for DoitrandDecoder {
    fn default() -> Self {
        Self::new()
    }
}
