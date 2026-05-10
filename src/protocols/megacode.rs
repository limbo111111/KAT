//! Megacode protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/megacode.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 1000;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 200;

const MIN_COUNT_BIT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct MegaCodeDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    last_bit: u8,
}

impl MegaCodeDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            last_bit: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }

    fn check_remote_controller(&self, data: u64) -> (u32, u8, u16) {
        if (data >> 23) == 1 {
            let serial = ((data >> 3) & 0xFFFF) as u32;
            let btn = (data & 0b111) as u8;
            let cnt = ((data >> 19) & 0b1111) as u16;
            (serial, btn, cnt)
        } else {
            (0, 0, 0)
        }
    }
}

impl ProtocolDecoder for MegaCodeDecoder {
    fn name(&self) -> &'static str {
        "MegaCode"
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
        &[315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 13) < TE_DELTA * 17 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.add_bit(1);
                    self.last_bit = 1;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= (TE_SHORT * 10) {
                        self.step = DecoderStep::Reset;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let (serial, btn, cnt) = self.check_remote_controller(self.decode_data);
                            return Some(DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: Some(cnt),
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }
                    } else {
                        if self.last_bit == 0 {
                            self.te_last = duration.saturating_sub(TE_SHORT * 3);
                        } else {
                            self.te_last = duration;
                        }
                        self.step = DecoderStep::CheckDuration;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(self.te_last, TE_SHORT * 5) < TE_DELTA * 5
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.add_bit(1);
                        self.last_bit = 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT * 2) < TE_DELTA * 2
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        self.add_bit(0);
                        self.last_bit = 0;
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

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut data = decoded.data;

        if button != 0 {
            data = (data & !0b111) | (button as u64 & 0b111);
        }

        let mut upload = Vec::new();

        for bit in (0..MIN_COUNT_BIT).rev() {
            let is_one = ((data >> bit) & 1) == 1;

            if is_one {
                upload.push(LevelDuration::new(false, TE_SHORT * 5));
                upload.push(LevelDuration::new(true, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(false, TE_SHORT * 2));
                upload.push(LevelDuration::new(true, TE_SHORT));
            }
        }

        // Flipper sends this tail at the end
        upload.push(LevelDuration::new(false, TE_SHORT * 14));

        Some(upload)
    }
}

impl Default for MegaCodeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
