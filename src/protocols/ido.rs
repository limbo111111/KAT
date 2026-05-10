use super::{ProtocolDecoder, ProtocolTiming, DecodedSignal};
use super::common::{add_bit, reverse_key};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 450;
const TE_LONG: u32 = 1450;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT_FOR_FOUND: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundPreambula,
    SaveDuration,
    CheckDuration,
}

pub struct IDoDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl IDoDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for IDoDecoder {
    fn name(&self) -> &'static str {
        "IDo117/111"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: TE_DELTA,
            min_count_bit: MIN_COUNT_BIT_FOR_FOUND,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000] // SubGhzProtocolFlag_433
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 5 {
                    self.step = DecoderStep::FoundPreambula;
                }
            }
            DecoderStep::FoundPreambula => {
                if !level && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 5 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration >= TE_SHORT * 5 + TE_DELTA {
                        self.step = DecoderStep::FoundPreambula;
                        if self.decode_count_bit >= MIN_COUNT_BIT_FOR_FOUND {
                            let data = self.decode_data;
                            let count = self.decode_count_bit;

                            self.decode_data = 0;
                            self.decode_count_bit = 0;

                            let code_found_reverse = reverse_key(data, count);
                            let code_fix = (code_found_reverse & 0xFFFFFF) as u32;
                            let serial = code_fix & 0xFFFFF;
                            let btn = ((code_fix >> 20) & 0x0F) as u8;

                            return Some(DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: None,
                                crc_valid: true, // No CRC check
                                data,
                                data_count_bit: count,
                                encoder_capable: false,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA * 3 {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA * 3 &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, true);
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
        false
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for IDoDecoder {
    fn default() -> Self {
        Self::new()
    }
}
