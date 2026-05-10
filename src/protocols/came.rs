use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 320;
const TE_LONG: u32 = 640;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 12;

const CAME_12_COUNT_BIT: usize = 12;
const CAME_24_COUNT_BIT: usize = 24;
const PRASTEL_25_COUNT_BIT: usize = 25;
const PRASTEL_42_COUNT_BIT: usize = 42;
const AIRFORCE_COUNT_BIT: usize = 18;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct CameDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl CameDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for CameDecoder {
    fn name(&self) -> &'static str {
        "CAME"
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
                if !level && duration_diff!(duration, TE_SHORT * 56) < TE_DELTA * 63 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if !level {
                    // Wait for high
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
                        if self.decode_count_bit == MIN_COUNT_BIT ||
                           self.decode_count_bit == AIRFORCE_COUNT_BIT ||
                           self.decode_count_bit == PRASTEL_25_COUNT_BIT ||
                           self.decode_count_bit == PRASTEL_42_COUNT_BIT ||
                           self.decode_count_bit == CAME_24_COUNT_BIT {

                            let mut name = "CAME".to_string();
                            if self.decode_count_bit == PRASTEL_25_COUNT_BIT || self.decode_count_bit == PRASTEL_42_COUNT_BIT {
                                name = "Prastel".to_string();
                            } else if self.decode_count_bit == AIRFORCE_COUNT_BIT {
                                name = "Airforce".to_string();
                            }

                            let result = DecodedSignal {
                                serial: Some(0),
                                button: Some(0),
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: Some(name),
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
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA && duration_diff!(duration, TE_SHORT) < TE_DELTA {
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
        let mut upload = Vec::new();
        let data = decoded.data;
        let count_bit = decoded.data_count_bit;

        let header_te = match count_bit {
            CAME_24_COUNT_BIT | PRASTEL_42_COUNT_BIT => 76,
            CAME_12_COUNT_BIT | AIRFORCE_COUNT_BIT => 47,
            PRASTEL_25_COUNT_BIT => 36,
            _ => 16,
        };

        upload.push(LevelDuration::new(false, TE_SHORT * header_te));
        upload.push(LevelDuration::new(true, TE_SHORT));

        for i in (0..count_bit).rev() {
            let bit = (data >> i) & 1;
            if bit == 1 {
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

impl Default for CameDecoder {
    fn default() -> Self {
        Self::new()
    }
}
