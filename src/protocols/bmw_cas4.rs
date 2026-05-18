use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::protocols::common::{common_manchester_advance, CommonManchesterState};
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT: usize = 64;

const PREAMBLE_PULSE_MIN: u32 = 300;
const PREAMBLE_PULSE_MAX: u32 = 700;
const PREAMBLE_MIN: u16 = 10;
const DATA_BITS: usize = 64;
const GAP_MIN: u32 = 1800;
const BYTE0_MARKER: u8 = 0x30;
const BYTE6_MARKER: u8 = 0xC5;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    Data,
}

pub struct BmwCas4Decoder {
    step: DecoderStep,
    manchester_state: CommonManchesterState,
    preamble_count: u16,
    raw_data: [u8; 8],
    bit_count: usize,
    decode_data: u64,
}

impl BmwCas4Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            manchester_state: CommonManchesterState::Mid1,
            preamble_count: 0,
            raw_data: [0; 8],
            bit_count: 0,
            decode_data: 0,
        }
    }
}

impl ProtocolDecoder for BmwCas4Decoder {
    fn name(&self) -> &'static str {
        "BMW CAS4"
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
        self.manchester_state = CommonManchesterState::Mid1;
        self.preamble_count = 0;
        self.raw_data = [0; 8];
        self.bit_count = 0;
        self.decode_data = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let diff_short = duration.abs_diff(TE_SHORT);
        let diff_long = duration.abs_diff(TE_LONG);

        match self.step {
            DecoderStep::Reset => {
                if level && (PREAMBLE_PULSE_MIN..=PREAMBLE_PULSE_MAX).contains(&duration) {
                    self.step = DecoderStep::Preamble;
                    self.preamble_count = 1;
                }
            }
            DecoderStep::Preamble => {
                if (PREAMBLE_PULSE_MIN..=PREAMBLE_PULSE_MAX).contains(&duration) {
                    self.preamble_count += 1;
                } else if !level && duration >= GAP_MIN {
                    if self.preamble_count >= PREAMBLE_MIN {
                        self.bit_count = 0;
                        self.decode_data = 0;
                        self.raw_data = [0; 8];
                        self.manchester_state = CommonManchesterState::Mid1;
                        self.step = DecoderStep::Data;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::Data => {
                if self.bit_count >= DATA_BITS {
                    self.step = DecoderStep::Reset;
                    return None;
                }

                let mut event = None;
                if diff_short < TE_DELTA {
                    event = Some(if level { 0 } else { 1 });
                } else if diff_long < TE_DELTA {
                    event = Some(if level { 2 } else { 3 });
                }

                if let Some(ev) = event {
                    let (new_state, bit_opt) = common_manchester_advance(self.manchester_state, ev);
                    self.manchester_state = new_state;

                    if let Some(bit) = bit_opt {
                        let new_bit = if bit { 1 } else { 0 };

                        if self.bit_count < DATA_BITS {
                            let byte_idx = self.bit_count / 8;
                            let bit_pos = 7 - (self.bit_count % 8);
                            if new_bit == 1 {
                                self.raw_data[byte_idx] |= 1 << bit_pos;
                            }
                            self.decode_data = (self.decode_data << 1) | new_bit;
                        }

                        self.bit_count += 1;

                        if self.bit_count == DATA_BITS {
                            if self.raw_data[0] == BYTE0_MARKER && self.raw_data[6] == BYTE6_MARKER {
                                let result = DecodedSignal {
                                    serial: None,
                                    button: None,
                                    counter: None,
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: DATA_BITS,
                                    encoder_capable: false,
                                    extra: None,
                                    protocol_display_name: None,
                                };
                                self.step = DecoderStep::Reset;
                                return Some(result);
                            }
                            self.step = DecoderStep::Reset;
                        }
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

impl Default for BmwCas4Decoder {
    fn default() -> Self {
        Self::new()
    }
}
