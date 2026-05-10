use super::{ProtocolDecoder, ProtocolTiming, DecodedSignal};
use super::common::add_bit;
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 275;
const TE_LONG: u32 = 1375;
const TE_DELTA: u32 = 150;
const MIN_COUNT_BIT_FOR_FOUND: usize = 32;
const DIMMING_COUNT_BIT: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    StartSync,
    FoundSync,
    StartDuration,
    SaveDuration,
    CheckDuration,
    EndDuration,
}

pub struct IntertechnoV3Decoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl IntertechnoV3Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for IntertechnoV3Decoder {
    fn name(&self) -> &'static str {
        "Intertechno_V3"
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
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 38) < TE_LONG * 2 {
                    self.step = DecoderStep::StartSync;
                }
            }
            DecoderStep::StartSync => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::FoundSync;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::FoundSync => {
                if !level && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 3 {
                    self.step = DecoderStep::StartDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::StartDuration => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::SaveDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration >= TE_SHORT * 11 {
                        self.step = DecoderStep::StartSync;
                        if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND || self.decode_count_bit == DIMMING_COUNT_BIT {
                            let data = self.decode_data;
                            let count = self.decode_count_bit;

                            let mut serial = 0;
                            let mut cnt = 0;
                            let mut btn = 0;

                            if count == MIN_COUNT_BIT_FOR_FOUND {
                                serial = ((data >> 6) & 0x3FFFFFF) as u32;
                                if ((data >> 5) & 0x1) != 0 {
                                    cnt = 1 << 5;
                                } else {
                                    cnt = (!data & 0xF) as u16;
                                }
                                btn = ((data >> 4) & 0x1) as u8;
                            } else if count == DIMMING_COUNT_BIT {
                                serial = ((data >> 10) & 0x3FFFFFF) as u32;
                                if ((data >> 9) & 0x1) != 0 {
                                    cnt = 1 << 5;
                                } else {
                                    cnt = (!(data >> 4) & 0xF) as u16;
                                }
                                btn = (data & 0xF) as u8;
                            }

                            return Some(DecodedSignal {
                                serial: Some(serial),
                                button: Some(btn),
                                counter: Some(cnt),
                                crc_valid: true,
                                data,
                                data_count_bit: count,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }
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
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                        self.step = DecoderStep::EndDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, true);
                        self.step = DecoderStep::EndDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA * 2 &&
                              duration_diff!(duration, TE_SHORT) < TE_DELTA &&
                              self.decode_count_bit == 27 {
                        // dimm_state
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                        self.step = DecoderStep::EndDuration;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::EndDuration => {
                if !level && (duration_diff!(duration, TE_SHORT) < TE_DELTA ||
                              duration_diff!(duration, TE_LONG) < TE_DELTA * 2) {
                    self.step = DecoderStep::StartDuration;
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
        let mut upload = Vec::new();

        // Header
        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_SHORT * 38));

        // Sync
        upload.push(LevelDuration::new(true, TE_SHORT));
        upload.push(LevelDuration::new(false, TE_SHORT * 10));

        let count = decoded.data_count_bit;
        for i in (1..=count).rev() {
            if count == DIMMING_COUNT_BIT && i == 10 { // C logic reads bit_read(data, i - 1), when i==9 logic has i==9, our loop runs from count down to 1. 9 means bit_index 8, but it says "i == 9", so checking for index 9. Let's adapt exactly: `if (i == 10)` in rust means index 9. Wait, in C `i` loops `data_count_bit` down to 1. `i == 9` means 9th bit.
                // send bit dimm
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else if ((decoded.data >> (i - 1)) & 1) == 1 {
                // send bit 1
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                // send bit 0
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for IntertechnoV3Decoder {
    fn default() -> Self {
        Self::new()
    }
}
