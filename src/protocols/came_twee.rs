use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::protocols::common::{common_manchester_advance, CommonManchesterState};
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 250;
const MIN_COUNT_BIT: usize = 54;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    DecoderData,
}

pub struct CameTweeDecoder {
    step: DecoderStep,
    manchester_saved_state: CommonManchesterState,
    decode_data: u64,
    decode_count_bit: usize,
}

impl CameTweeDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            manchester_saved_state: CommonManchesterState::Mid1,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for CameTweeDecoder {
    fn name(&self) -> &'static str {
        "CAME TWEE"
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
        self.manchester_saved_state = CommonManchesterState::Mid1;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut event = None;
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_LONG * 51) < TE_DELTA * 20 {
                    self.step = DecoderStep::DecoderData;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;

                    self.manchester_saved_state = CommonManchesterState::Mid1; // Reset equivalent
                    let (st1, _) = common_manchester_advance(self.manchester_saved_state, 2); // LongLow
                    let (st2, _) = common_manchester_advance(st1, 3); // LongHigh
                    let (st3, _) = common_manchester_advance(st2, 0); // ShortLow
                    self.manchester_saved_state = st3;
                }
            }
            DecoderStep::DecoderData => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = Some(0); // ShortLow
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = Some(2); // LongLow
                    } else if duration >= TE_LONG * 2 + TE_DELTA {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let result = DecodedSignal {
                                serial: None,
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
                        self.decode_data = 0;
                        self.decode_count_bit = 0;

                        self.manchester_saved_state = CommonManchesterState::Mid1; // Reset equivalent
                        let (st1, _) = common_manchester_advance(self.manchester_saved_state, 2); // LongLow
                        let (st2, _) = common_manchester_advance(st1, 3); // LongHigh
                        let (st3, _) = common_manchester_advance(st2, 0); // ShortLow
                        self.manchester_saved_state = st3;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = Some(1); // ShortHigh
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = Some(3); // LongHigh
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }

                if let Some(ev) = event {
                    let (new_state, bit_opt) = common_manchester_advance(self.manchester_saved_state, ev);
                    self.manchester_saved_state = new_state;

                    if let Some(bit) = bit_opt {
                        let bit_val = if bit { 1 } else { 0 };
                        // Flipper logic does `!data`
                        self.decode_data = (self.decode_data << 1) | (1 - bit_val);
                        self.decode_count_bit += 1;
                    }
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        // Encoder requires XOR magic numbers, omitted for simplicity unless fully implemented.
        false
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for CameTweeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
