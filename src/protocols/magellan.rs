//! Magellan protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/magellan.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 200;
const TE_LONG: u32 = 400;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 32;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreambula,
    FoundPreambula,
    SaveDuration,
    CheckDuration,
}

pub struct MagellanDecoder {
    step: DecoderStep,
    te_last: u32,
    header_count: u16,
    decode_data: u64,
    decode_count_bit: usize,
}

impl MagellanDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            header_count: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }

    fn crc8(data: &[u8]) -> u8 {
        let mut crc: u8 = 0x00;
        let poly: u8 = 0x31;
        for &byte in data {
            crc ^= byte;
            for _ in 0..8 {
                if (crc & 0x80) != 0 {
                    crc = (crc << 1) ^ poly;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }

    fn check_crc(&self) -> bool {
        let d = self.decode_data;
        let data = [
            (d >> 24) as u8,
            (d >> 16) as u8,
            (d >> 8) as u8,
        ];
        (d & 0xFF) as u8 == Self::crc8(&data)
    }
}

impl ProtocolDecoder for MagellanDecoder {
    fn name(&self) -> &'static str {
        "Magellan"
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
        self.te_last = 0;
        self.header_count = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
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
                    self.te_last = duration;
                } else {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.header_count += 1;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                              duration_diff!(duration, TE_LONG) < TE_DELTA * 2 &&
                              self.header_count > 10 {
                        self.step = DecoderStep::FoundPreambula;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }
            }
            DecoderStep::FoundPreambula => {
                if level {
                    self.te_last = duration;
                } else {
                    if duration_diff!(self.te_last, TE_SHORT * 6) < TE_DELTA * 3 &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                        self.step = DecoderStep::SaveDuration;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration >= TE_LONG * 3 {
                        if self.decode_count_bit == MIN_COUNT_BIT && self.check_crc() {
                            let decoded_sig = DecodedSignal {
                                serial: None, // Just a raw data block usually for sensors
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            self.step = DecoderStep::Reset;
                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            return Some(decoded_sig);
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
        let mut upload = Vec::new();

        // Preamble logic mapped exactly to Flipper ARF
        upload.push(LevelDuration::new(true, TE_SHORT * 4));
        upload.push(LevelDuration::new(false, TE_SHORT));
        for _ in 0..12 {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_SHORT));
        }
        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_LONG));

        // Custom sync spacing
        upload.push(LevelDuration::new(true, TE_SHORT * 6));
        upload.push(LevelDuration::new(false, TE_LONG));

        for i in (0..decoded.data_count_bit).rev() {
            if ((decoded.data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        // Stop bit
        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_LONG * 100));

        Some(upload)
    }
}

impl Default for MagellanDecoder {
    fn default() -> Self {
        Self::new()
    }
}
