//! Marantec24 protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/marantec24.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 800;
const TE_LONG: u32 = 1600;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 24;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct Marantec24Decoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl Marantec24Decoder {
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

impl ProtocolDecoder for Marantec24Decoder {
    fn name(&self) -> &'static str {
        "Marantec24"
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
        &[868_350_000, 433_920_000] // Flipper ARF says 868 but let's include 433 just in case
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
                if !level && duration_diff!(duration, TE_LONG * 9) < TE_DELTA * 4 {
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
                    if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                       duration_diff!(duration, TE_SHORT * 3) < TE_DELTA {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                              duration_diff!(duration, TE_LONG * 2) < TE_DELTA {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_LONG * 9) < TE_DELTA * 4 {
                        if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.add_bit(0);
                        } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(1);
                        }

                        let mut decoded_sig = None;

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let serial = (self.decode_data >> 4) as u32;
                            let button = (self.decode_data & 0xF) as u8;

                            decoded_sig = Some(DecodedSignal {
                                serial: Some(serial),
                                button: Some(button),
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.step = DecoderStep::Reset;

                        return decoded_sig;
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

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();

        let data = if let Some(serial) = decoded.serial {
            ((serial as u64) << 4) | (button as u64 & 0xF)
        } else {
            decoded.data
        };

        for i in (0..MIN_COUNT_BIT).rev() {
            let is_last = i == 0;
            if ((data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                if is_last {
                    upload.push(LevelDuration::new(false, TE_LONG * 9 + TE_SHORT));
                } else {
                    upload.push(LevelDuration::new(false, TE_LONG * 2));
                }
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                if is_last {
                    upload.push(LevelDuration::new(false, TE_LONG * 9 + TE_SHORT));
                } else {
                    upload.push(LevelDuration::new(false, TE_SHORT * 3));
                }
            }
        }

        Some(upload)
    }
}

impl Default for Marantec24Decoder {
    fn default() -> Self {
        Self::new()
    }
}
