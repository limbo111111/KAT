//! GateTX protocol decoder
//!
//! Aligned with Flipper-ARF gate_tx.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 350;
const TE_LONG: u32 = 700;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct GateTxDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl GateTxDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for GateTxDecoder {
    fn name(&self) -> &'static str {
        "GateTX"
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
                if !level && duration_diff!(duration, TE_SHORT * 47) < TE_DELTA * 47 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= TE_SHORT * 10 + TE_DELTA {
                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let code_found_reverse = self.decode_data.reverse_bits() >> (64 - self.decode_count_bit);

                            let serial = ((code_found_reverse & 0xFF) << 12) |
                                       (((code_found_reverse >> 8) & 0xFF) << 4) |
                                       ((code_found_reverse >> 20) & 0x0F);
                            let btn = ((code_found_reverse >> 16) & 0x0F) as u8;

                            let result = DecodedSignal {
                                serial: Some(serial as u32),
                                button: Some(btn),
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
                            return Some(result);
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    }
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 3 &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
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
        let mut out = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        out.push(LevelDuration::new(false, TE_SHORT * 49));
        out.push(LevelDuration::new(true, TE_LONG));

        let data = decoded.data;
        for i in (1..=decoded.data_count_bit).rev() {
            if ((data >> (i - 1)) & 1) == 1 {
                out.push(LevelDuration::new(false, TE_LONG));
                out.push(LevelDuration::new(true, TE_SHORT));
            } else {
                out.push(LevelDuration::new(false, TE_SHORT));
                out.push(LevelDuration::new(true, TE_LONG));
            }
        }

        Some(out)
    }
}

impl Default for GateTxDecoder {
    fn default() -> Self {
        Self::new()
    }
}
