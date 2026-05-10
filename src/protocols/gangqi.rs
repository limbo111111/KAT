//! GangQi protocol decoder
//!
//! Aligned with Flipper-ARF gangqi.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1200;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 34;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct GangQiDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl GangQiDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for GangQiDecoder {
    fn name(&self) -> &'static str {
        "GangQi"
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
        &[433_920_000] // Typical frequency for this kind of remote
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
                if !level && duration_diff!(duration, TE_LONG * 2) < TE_DELTA * 3 {
                    // Found GAP
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        // Bit 0 is short and long timing
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        // Bit 1 is long and short timing
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(duration, TE_LONG * 2) < TE_DELTA * 3 {
                        // End of the key
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.decode_data = (self.decode_data << 1) | 0;
                            self.decode_count_bit += 1;
                        }
                        if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                            self.decode_data = (self.decode_data << 1) | 1;
                            self.decode_count_bit += 1;
                        }

                        if self.decode_count_bit >= MIN_COUNT_BIT {
                            let btn = ((self.decode_data >> 10) & 0xF) as u8;
                            let serial = ((self.decode_data & 0xFFFFF0000) >> 16) as u32;

                            let result = DecodedSignal {
                                serial: Some(serial),
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
                            self.step = DecoderStep::Reset;

                            return Some(result);
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

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut data = decoded.data;
        let serial = ((data >> 18) & 0xFFFF) as u16;
        let const_and_button = 0xD0 | button;
        let serial_high = (serial >> 8) as u8;
        let serial_low = (serial & 0xFF) as u8;
        let bytesum = 0xC8u8.wrapping_sub(serial_high).wrapping_sub(serial_low).wrapping_sub(const_and_button);

        data = ((data >> 14) << 14) | ((button as u64) << 10) | ((bytesum as u64) << 2);

        let mut out = Vec::new();

        for i in (1..=decoded.data_count_bit).rev() {
            let bit = (data >> (i - 1)) & 1;
            if bit == 1 {
                out.push(LevelDuration::new(true, TE_LONG));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 4 + TE_DELTA));
                } else {
                    out.push(LevelDuration::new(false, TE_SHORT));
                }
            } else {
                out.push(LevelDuration::new(true, TE_SHORT));
                if i == 1 {
                    out.push(LevelDuration::new(false, TE_SHORT * 4 + TE_DELTA));
                } else {
                    out.push(LevelDuration::new(false, TE_LONG));
                }
            }
        }
        Some(out)
    }
}

impl Default for GangQiDecoder {
    fn default() -> Self {
        Self::new()
    }
}
