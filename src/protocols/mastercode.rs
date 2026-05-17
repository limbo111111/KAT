//! Mastercode protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/mastercode.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 1072;
const TE_LONG: u32 = 2145;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 36;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct MastercodeDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl MastercodeDecoder {
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

impl ProtocolDecoder for MastercodeDecoder {
    fn name(&self) -> &'static str {
        "Mastercode"
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
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 15) < TE_DELTA * 15 {
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA * 8 {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 8 &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_SHORT * 15) < TE_DELTA * 15 {
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(0);
                        } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 8 {
                            self.add_bit(1);
                        } else {
                            self.step = DecoderStep::Reset;
                            return None;
                        }

                        let mut decoded_sig = None;

                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let serial = ((self.decode_data >> 4) & 0xFFFF) as u32;
                            let button = ((self.decode_data >> 2) & 0x03) as u8;

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

                        self.step = DecoderStep::SaveDuration;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;

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
            // Reconstruct data, assuming bits 0-1 are unknown (just use existing)
            let base = decoded.data & 0x03; // bits 0,1
            // In Flipper ARF:
            // instance->serial = (instance->data >> 4) & 0xFFFF;
            // instance->btn = (instance->data >> 2 & 0x03);

            ((serial as u64) << 4) | ((button as u64 & 0x03) << 2) | base
        } else {
            decoded.data
        };

        for i in (1..MIN_COUNT_BIT).rev() {
            if ((data >> i) & 1) == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        if (data & 1) == 1 {
            upload.push(LevelDuration::new(true, TE_LONG));
            upload.push(LevelDuration::new(false, TE_SHORT + TE_SHORT * 13));
        } else {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_LONG + TE_SHORT * 13));
        }

        Some(upload)
    }
}

impl Default for MastercodeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
