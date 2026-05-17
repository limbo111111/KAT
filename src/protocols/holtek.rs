//! Holtek protocol decoder
//!
//! Aligned with Flipper-ARF holtek.c

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 430;
const TE_LONG: u32 = 870;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT: usize = 40;

const HOLTEK_HEADER_MASK: u64 = 0xF000000000;
const HOLTEK_HEADER: u64 = 0x5000000000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct HoltekDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl HoltekDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for HoltekDecoder {
    fn name(&self) -> &'static str {
        "Holtek"
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
        &[433_920_000, 868_350_000, 315_000_000]
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
                if !level && duration_diff!(duration, TE_SHORT * 36) < TE_DELTA * 36 {
                    // Found Preambula
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    // Found StartBit
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
                        if self.decode_count_bit == MIN_COUNT_BIT
                            && (self.decode_data & HOLTEK_HEADER_MASK) == HOLTEK_HEADER {
                                let serial_raw = ((self.decode_data >> 16) & 0xFFFFF) as u32;
                                let serial = serial_raw.reverse_bits() >> (32 - 20); // reverse 20 bits

                                let btn_raw = (self.decode_data & 0xFFFF) as u16;
                                let btn = if (btn_raw & 0xF) != 0xA {
                                    (1 << 4) | (btn_raw & 0xF)
                                } else if ((btn_raw >> 4) & 0xF) != 0xA {
                                    (2 << 4) | ((btn_raw >> 4) & 0xF)
                                } else if ((btn_raw >> 8) & 0xF) != 0xA {
                                    (3 << 4) | ((btn_raw >> 8) & 0xF)
                                } else if ((btn_raw >> 12) & 0xF) != 0xA {
                                    (4 << 4) | ((btn_raw >> 12) & 0xF)
                                } else {
                                    0
                                };

                                let result = DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn as u8),
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
                                self.step = DecoderStep::FoundStartBit;
                                return Some(result);
                            }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.step = DecoderStep::FoundStartBit;
                    } else {
                        self.te_last = duration;
                        self.step = DecoderStep::CheckDuration;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA * 2
                    {
                        self.decode_data <<= 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2
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

        // Send header
        out.push(LevelDuration::new(false, TE_SHORT * 36));
        // Send start bit
        out.push(LevelDuration::new(true, TE_SHORT));

        // Send key data
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

impl Default for HoltekDecoder {
    fn default() -> Self {
        Self::new()
    }
}
