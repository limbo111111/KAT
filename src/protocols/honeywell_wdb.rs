//! Honeywell WDB protocol decoder
//!
//! Aligned with Flipper-ARF honeywell_wdb.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 160;
const TE_LONG: u32 = 320;
const TE_DELTA: u32 = 60;
const MIN_COUNT_BIT: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct HoneywellWdbDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl HoneywellWdbDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn get_parity(mut data: u64, count: usize) -> u8 {
        let mut parity = 0;
        for _ in 0..count {
            parity ^= data & 1;
            data >>= 1;
        }
        parity as u8
    }
}

impl ProtocolDecoder for HoneywellWdbDecoder {
    fn name(&self) -> &'static str {
        "Honeywell"
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
        &[315_000_000, 433_920_000]
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
                if !level && duration_diff!(duration, TE_SHORT * 3) < TE_DELTA {
                    self.decode_count_bit = 0;
                    self.decode_data = 0;
                    self.step = DecoderStep::SaveDuration;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration_diff!(duration, TE_SHORT * 3) < TE_DELTA {
                        if self.decode_count_bit == MIN_COUNT_BIT
                            && (self.decode_data & 1) as u8
                                == Self::get_parity(self.decode_data >> 1, MIN_COUNT_BIT - 1)
                        {
                            let serial = ((self.decode_data >> 28) & 0xFFFFF) as u32;

                            let result = DecodedSignal {
                                serial: Some(serial),
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
                            return Some(result);
                        }
                        self.step = DecoderStep::Reset;
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
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
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

        out.push(LevelDuration::new(false, TE_SHORT * 3));

        let data = decoded.data;
        for i in (1..=decoded.data_count_bit).rev() {
            if ((data >> (i - 1)) & 1) == 1 {
                out.push(LevelDuration::new(true, TE_LONG));
                out.push(LevelDuration::new(false, TE_SHORT));
            } else {
                out.push(LevelDuration::new(true, TE_SHORT));
                out.push(LevelDuration::new(false, TE_LONG));
            }
        }

        out.push(LevelDuration::new(true, TE_SHORT * 3));

        Some(out)
    }
}

impl Default for HoneywellWdbDecoder {
    fn default() -> Self {
        Self::new()
    }
}
