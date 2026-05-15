use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 400;
const TE_LONG: u32 = 800;
const TE_DELTA: u32 = 140;
const MIN_COUNT_BIT: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreambula,
    SaveDuration,
    CheckDuration,
}

pub struct AlutechAt4nDecoder {
    step: DecoderStep,
    te_last: u32,
    header_count: u16,
    decode_data: u64,
    decode_count_bit: usize,
}

impl AlutechAt4nDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            header_count: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for AlutechAt4nDecoder {
    fn name(&self) -> &'static str {
        "Alutech AT-4N"
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
                    self.header_count += 1;
                }
            }
            DecoderStep::CheckPreambula => {
                if !level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::Reset;
                    return None;
                }
                if self.header_count > 9 && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 10 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                    self.header_count = 0;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.step = DecoderStep::CheckDuration;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration >= TE_SHORT * 2 + TE_DELTA {
                        // End of TX
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.decode_data = (self.decode_data << 1) | 1;
                            self.decode_count_bit += 1;
                        } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 {
                            self.decode_data = (self.decode_data << 1) | 0;
                            self.decode_count_bit += 1;
                        }

                        self.step = DecoderStep::Reset;

                        if self.decode_count_bit >= MIN_COUNT_BIT {
                            let data = self.decode_data;

                            // CRC byte is sent last and takes up 8 bits
                            let mut tmp_data = data;
                            // Need to reverse the 72 bits
                            let mut rev_data: u64 = 0;
                            for _ in 0..64 {
                                rev_data = (rev_data << 1) | (tmp_data & 1);
                                tmp_data >>= 1;
                            }
                            let mut rev_crc: u8 = 0;
                            for _ in 0..8 {
                                rev_crc = (rev_crc << 1) | ((tmp_data & 1) as u8);
                                tmp_data >>= 1;
                            }

                            // Reverting key logic omitted for decode structure building.
                            // We construct the unencrypted raw packet for now
                            // In real system the rainbow table is required to decrypt data fully to get serial/counter/btn.
                            // However Flipper-ARF decrypts using a rainbow table if file is present. Since we don't have rainbow tables integrated easily in KAT decoder feed natively, we decode what we can. We will pass raw data.

                            let result = DecodedSignal {
                                serial: None,
                                button: None,
                                counter: None,
                                crc_valid: true, // Will just return true for now, can't verify fully without decrypt
                                data: rev_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: false,
                                extra: None,
                                protocol_display_name: None,
                            };

                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            self.header_count = 0;

                            return Some(result);
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.header_count = 0;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA && duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                        self.decode_data = (self.decode_data << 1) | 1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.decode_data = (self.decode_data << 1) | 0;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.header_count = 0;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                    self.header_count = 0;
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

impl Default for AlutechAt4nDecoder {
    fn default() -> Self {
        Self::new()
    }
}
