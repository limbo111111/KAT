//! Nice FloR-S protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/nice_flor_s.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 300;

const MIN_COUNT_BIT: usize = 52;
const NICE_ONE_COUNT_BIT: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckHeader,
    FoundHeader,
    SaveDuration,
    CheckDuration,
}

pub struct NiceFlorSDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_data_2: u64,
    decode_count_bit: usize,
}

impl NiceFlorSDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_data_2: 0,
            decode_count_bit: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for NiceFlorSDecoder {
    fn name(&self) -> &'static str {
        "Nice FloR-S"
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
        &[433_920_000, 868_350_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 38) < TE_DELTA * 38 {
                    self.step = DecoderStep::CheckHeader;
                }
            }
            DecoderStep::CheckHeader => {
                if level && duration_diff!(duration, TE_SHORT * 3) < TE_DELTA * 3 {
                    self.step = DecoderStep::FoundHeader;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::FoundHeader => {
                if !level && duration_diff!(duration, TE_SHORT * 3) < TE_DELTA * 3 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration_diff!(duration, TE_SHORT * 3) < TE_DELTA {
                        self.step = DecoderStep::Reset;
                        if self.decode_count_bit == MIN_COUNT_BIT || self.decode_count_bit == NICE_ONE_COUNT_BIT {
                            let data = self.decode_data_2;
                            let data_2 = self.decode_data;

                            // We're omitting full decryption because rainbow table isn't accessible,
                            // so we return the raw encrypted data.

                            let res = DecodedSignal {
                                serial: None, // needs decryption table
                                button: None, // needs decryption table
                                counter: None, // needs decryption table
                                crc_valid: true,
                                data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: false, // Cannot encode without keystore/rainbow table
                                extra: if self.decode_count_bit == NICE_ONE_COUNT_BIT {
                                    Some(data_2)
                                } else {
                                    None
                                },
                                protocol_display_name: if self.decode_count_bit == NICE_ONE_COUNT_BIT {
                                    Some("Nice One".to_string())
                                } else {
                                    None
                                },
                            };
                            return Some(res);
                        }
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    }
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

                if self.decode_count_bit == MIN_COUNT_BIT {
                    self.decode_data_2 = self.decode_data;
                    self.decode_data = 0;
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        false // Needs rainbow table / keystore decryption
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for NiceFlorSDecoder {
    fn default() -> Self {
        Self::new()
    }
}
