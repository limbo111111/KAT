//! Nero Sketch protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/nero_sketch.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 330;
const TE_LONG: u32 = 660;
const TE_DELTA: u32 = 150;

const MIN_COUNT_BIT: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreambula,
    SaveDuration,
    CheckDuration,
}

pub struct NeroSketchDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    header_count: u16,
}

impl NeroSketchDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            header_count: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for NeroSketchDecoder {
    fn name(&self) -> &'static str {
        "Nero Sketch"
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
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::CheckPreambula;
                    self.te_last = duration;
                    self.header_count = 0;
                }
            }
            DecoderStep::CheckPreambula => {
                if level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA
                        || duration_diff!(duration, TE_SHORT * 4) < TE_DELTA
                    {
                        self.te_last = duration;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                        self.header_count += 1;
                    } else if duration_diff!(self.te_last, TE_SHORT * 4) < TE_DELTA {
                        if self.header_count > 40 {
                            self.step = DecoderStep::SaveDuration;
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                        } else {
                            self.step = DecoderStep::Reset;
                        }
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration >= (TE_SHORT * 2 + TE_DELTA * 2) {
                        // Found stop bit
                        self.step = DecoderStep::Reset;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let res = DecodedSignal {
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
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            return Some(res);
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
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
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
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
        let mut upload = Vec::new();
        let data = decoded.data;
        let data_count_bit = decoded.data_count_bit;

        // Header
        for _ in 0..47 {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_SHORT));
        }

        // Start bit
        upload.push(LevelDuration::new(true, TE_SHORT * 4));
        upload.push(LevelDuration::new(false, TE_SHORT));

        // Key data
        for i in (0..data_count_bit).rev() {
            if ((data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        // Stop bit
        upload.push(LevelDuration::new(true, TE_SHORT * 3));
        upload.push(LevelDuration::new(false, TE_SHORT));

        Some(upload)
    }
}

impl Default for NeroSketchDecoder {
    fn default() -> Self {
        Self::new()
    }
}
