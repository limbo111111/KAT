use super::{ProtocolDecoder, ProtocolTiming, DecodedSignal};
use super::common::{add_bit};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 400;
const TE_LONG: u32 = 1200;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT_FOR_FOUND: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
    Finish,
}

pub struct KeyFinderDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
    end_count: u8,
}

impl KeyFinderDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
            end_count: 0,
        }
    }
}

impl ProtocolDecoder for KeyFinderDecoder {
    fn name(&self) -> &'static str {
        "KeyFinder"
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
        &[433_920_000] // AM
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.end_count = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 5 {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.step = DecoderStep::SaveDuration;
                }
            }
            DecoderStep::SaveDuration => {
                if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                    if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.end_count += 1;
                        if self.end_count == 4 {
                            self.step = DecoderStep::Finish;
                            self.end_count = 0;
                        }
                    } else if !level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        // wait for next level
                    } else {
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.end_count = 0;
                        self.step = DecoderStep::Reset;
                    }
                } else if level {
                    self.te_last = duration;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_LONG) < TE_DELTA {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, true);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::Finish => {
                let data = self.decode_data;
                let count = self.decode_count_bit;
                self.decode_data = 0;
                self.decode_count_bit = 0;
                self.end_count = 0;
                self.step = DecoderStep::Reset;

                if count == MIN_COUNT_BIT_FOR_FOUND {
                    let serial = (data >> 4) as u32;
                    let btn = (data & 0xF) as u8;

                    return Some(DecodedSignal {
                        serial: Some(serial),
                        button: Some(btn),
                        counter: None,
                        crc_valid: true,
                        data,
                        data_count_bit: count,
                        encoder_capable: true,
                        extra: None,
                        protocol_display_name: None,
                    });
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();
        let data = decoded.data;
        let count = decoded.data_count_bit;

        // key data 24 bit
        for i in (1..=count).rev() {
            if (data >> (i - 1)) & 1 == 1 {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            } else {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        // End bits (3 times then 1 more with gap 4k us)
        for _ in 0..3 {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, TE_SHORT));
        }
        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_SHORT * 10)); // 400 * 10 = 4000us gap

        Some(upload)
    }
}

impl Default for KeyFinderDecoder {
    fn default() -> Self {
        Self::new()
    }
}
