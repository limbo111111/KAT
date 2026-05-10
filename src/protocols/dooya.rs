use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 366;
const TE_LONG: u32 = 733;
const TE_DELTA: u32 = 120;
const MIN_COUNT_BIT_FOR_FOUND: usize = 40;

const DOOYA_SINGLE_CHANNEL: u16 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct DooyaDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl DooyaDecoder {
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

impl ProtocolDecoder for DooyaDecoder {
    fn name(&self) -> &'static str {
        "Dooya"
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
        &[433_920_000, 315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration_us, TE_LONG * 12) < TE_DELTA * 20 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if !level {
                    if duration_diff!(duration_us, TE_LONG * 2) < TE_DELTA * 3 {
                        self.step = DecoderStep::SaveDuration;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else if duration_diff!(duration_us, TE_SHORT * 13) < TE_DELTA * 5 {
                    // Do nothing
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration_us;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_us >= TE_LONG * 4 {
                        // Add last bit
                        if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                            self.add_bit(0);
                        } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 {
                            self.add_bit(1);
                        } else {
                            self.step = DecoderStep::Reset;
                            return None;
                        }

                        self.step = DecoderStep::FoundStartBit;
                        if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                            let data = self.decode_data;
                            let data_count_bit = self.decode_count_bit;
                            let serial = (data >> 16) as u32;
                            let cnt = if ((data >> 12) & 0x0F) != 0 {
                                ((data >> 8) & 0x0F) as u16
                            } else {
                                DOOYA_SINGLE_CHANNEL
                            };
                            let btn = (data & 0xFF) as u8;

                            let decoded = DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: Some(cnt),
                                crc_valid: true,
                                data,
                                data_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            };

                            return Some(decoded);
                        }
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                              duration_diff!(duration_us, TE_LONG) < TE_DELTA * 2 {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 &&
                              duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.add_bit(1);
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
        let mut upload = Vec::with_capacity(decoded.data_count_bit * 2 + 3);

        if (decoded.data & 1) == 1 {
            upload.push(LevelDuration::new(false, TE_LONG * 12 + TE_LONG));
        } else {
            upload.push(LevelDuration::new(false, TE_LONG * 12 + TE_SHORT));
        }

        upload.push(LevelDuration::new(true, TE_SHORT * 13));
        upload.push(LevelDuration::new(false, TE_LONG * 2));

        for i in (0..decoded.data_count_bit).rev() {
            if (decoded.data >> i) & 1 == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for DooyaDecoder {
    fn default() -> Self {
        Self::new()
    }
}
