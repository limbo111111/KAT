use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 555;
const TE_LONG: u32 = 1111;
const TE_DELTA: u32 = 120;
const MIN_COUNT_BIT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct AnsonicDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl AnsonicDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for AnsonicDecoder {
    fn name(&self) -> &'static str {
        "Ansonic"
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
        &[433_920_000, 315_000_000]
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
                if !level && duration_diff!(duration, TE_SHORT * 35) < TE_DELTA * 35 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if !level {
                    // Do nothing, wait for level
                } else if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= TE_SHORT * 4 {
                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit >= MIN_COUNT_BIT {
                            let btn = ((self.decode_data >> 1) & 0x3) as u8;
                            let cnt = (self.decode_data & 0xFFF) as u16;

                            let result = DecodedSignal {
                                serial: Some(0),
                                button: Some(btn),
                                counter: Some(cnt),
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
                if level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA && duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.decode_data = (self.decode_data << 1) | 0;
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
        // Send header
        upload.push(LevelDuration::new(false, TE_SHORT * 35));
        // Send start bit
        upload.push(LevelDuration::new(true, TE_SHORT));

        let data = decoded.data;
        let count_bit = decoded.data_count_bit;

        for i in (0..count_bit).rev() {
            let bit = (data >> i) & 1;
            if bit == 1 {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_LONG));
            } else {
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_SHORT));
            }
        }

        Some(upload)
    }
}

impl Default for AnsonicDecoder {
    fn default() -> Self {
        Self::new()
    }
}
