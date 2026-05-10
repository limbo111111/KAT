//! Legrand protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/legrand.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 375;
const TE_LONG: u32 = 1125;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 18;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    FirstBit,
    SaveDuration,
    CheckDuration,
}

pub struct LegrandDecoder {
    step: DecoderStep,
    te_last: u32,
    te: u32,
    decode_data: u64,
    decode_count_bit: usize,
    last_data: u64,
}

impl LegrandDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            te: 0,
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

impl ProtocolDecoder for LegrandDecoder {
    fn name(&self) -> &'static str {
        "Legrand"
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
        &[433_920_000] // Or whatever is standard, Flipper lists 433 and sensors
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_last = 0;
        self.te = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 16) < TE_DELTA * 8 {
                    self.step = DecoderStep::FirstBit;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.te = 0;
                }
            }
            DecoderStep::FirstBit => {
                if level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.add_bit(0);
                        self.te += duration * 4;
                    }

                    if duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                        self.add_bit(1);
                        self.te += duration / 3 * 4;
                    }

                    if self.decode_count_bit > 0 {
                        self.step = DecoderStep::SaveDuration;
                        return None;
                    }
                }
                self.step = DecoderStep::Reset;
            }
            DecoderStep::SaveDuration => {
                if !level {
                    self.te_last = duration;
                    self.te += duration;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    let mut found = false;

                    if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3 &&
                       duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        found = true;
                        self.add_bit(0);
                    }

                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                        found = true;
                        self.add_bit(1);
                    }

                    if found {
                        self.te += duration;

                        if self.decode_count_bit < MIN_COUNT_BIT {
                            self.step = DecoderStep::SaveDuration;
                            return None;
                        }

                        if self.last_data != 0 && self.last_data == self.decode_data {
                            self.te /= (self.decode_count_bit as u32) * 4;

                            let decoded_sig = DecodedSignal {
                                serial: None, // Just a raw signal
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: Some(self.te as u64),
                                protocol_display_name: None,
                            };

                            self.step = DecoderStep::Reset;
                            return Some(decoded_sig);
                        } else {
                            self.last_data = self.decode_data;
                            self.step = DecoderStep::Reset;
                            return None;
                        }
                    }
                }

                self.step = DecoderStep::Reset;
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();

        let mut te = TE_SHORT;
        if let Some(t) = decoded.extra {
            if t > 0 {
                te = t as u32;
            }
        }

        upload.push(LevelDuration::new(false, te * 16));

        for i in (0..decoded.data_count_bit).rev() {
            if ((decoded.data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(false, te));
                upload.push(LevelDuration::new(true, te * 3));
            } else {
                upload.push(LevelDuration::new(false, te * 3));
                upload.push(LevelDuration::new(true, te));
            }
        }

        Some(upload)
    }
}

impl Default for LegrandDecoder {
    fn default() -> Self {
        Self::new()
    }
}
