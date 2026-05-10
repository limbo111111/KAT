use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};

const TE_SHORT: u32 = 640;
const TE_LONG: u32 = 1280;
const TE_DELTA: u32 = 250;
const MIN_COUNT_BIT: usize = 56;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreamble,
    FoundPreamble,
    DecodeData,
}

pub struct SomfyTelisDecoder {
    step: DecoderStep,
    header_count: u16,
    decode_data: u64,
    decode_count_bit: usize,
    manchester_state: CommonManchesterState,
}

impl SomfyTelisDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            header_count: 0,
            decode_data: 0,
            decode_count_bit: 0,
            manchester_state: CommonManchesterState::Mid1,
        }
    }

    fn crc(data: u64) -> u8 {
        let mut crc = 0;
        let d = data & 0xFFF0FFFFFFFFFF;
        for i in (0..56).step_by(8) {
            crc = crc ^ (d >> i) ^ (d >> (i + 4));
        }
        (crc & 0xF) as u8
    }

    fn check_remote_controller(data: u64) -> (u8, u16, u32) {
        let decrypted = data ^ (data >> 8);
        let btn = ((decrypted >> 44) & 0xF) as u8;
        let cnt = ((decrypted >> 24) & 0xFFFF) as u16;
        let serial = (decrypted & 0xFFFFFF) as u32;
        (btn, cnt, serial)
    }

    pub fn get_button_name(btn: u8) -> &'static str {
        match btn {
            0x01 => "My",
            0x02 => "Up",
            0x03 => "My+Up",
            0x04 => "Down",
            0x05 => "My+Down",
            0x06 => "Up+Down",
            0x08 => "Prog",
            0x09 => "Sun+Flag",
            0x0A => "Flag",
            _ => "Unknown",
        }
    }
}

impl ProtocolDecoder for SomfyTelisDecoder {
    fn name(&self) -> &'static str {
        "Somfy Telis"
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
        &[433_920_000, 868_350_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.header_count = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.manchester_state = CommonManchesterState::Mid1;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut event: i32 = -1;

        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_SHORT * 4) < TE_DELTA * 4 {
                    self.step = DecoderStep::FoundPreamble;
                    self.header_count += 1;
                }
            }
            DecoderStep::FoundPreamble => {
                if !level && duration_diff!(duration, TE_SHORT * 4) < TE_DELTA * 4 {
                    self.step = DecoderStep::CheckPreamble;
                } else {
                    self.header_count = 0;
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckPreamble => {
                if level {
                    if duration_diff!(duration, TE_SHORT * 4) < TE_DELTA * 4 {
                        self.step = DecoderStep::FoundPreamble;
                        self.header_count += 1;
                    } else if self.header_count > 1 && duration_diff!(duration, TE_SHORT * 7) < TE_DELTA * 4 {
                        self.step = DecoderStep::DecodeData;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.manchester_state = CommonManchesterState::Mid1;
                        let (new_state, _) = common_manchester_advance(self.manchester_state, 3);
                        self.manchester_state = new_state;
                    }
                }
            }
            DecoderStep::DecodeData => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = 0;
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = 2;
                    } else if duration >= TE_LONG + TE_DELTA {
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            let data_tmp = self.decode_data ^ (self.decode_data >> 8);
                            let crc_recv = ((data_tmp >> 40) & 0xF) as u8;
                            let crc_calc = Self::crc(data_tmp);

                            if crc_recv == crc_calc {
                                let (btn, cnt, serial) = Self::check_remote_controller(self.decode_data);
                                let result = DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn),
                                    counter: Some(cnt),
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: self.decode_count_bit,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: Some(format!("Somfy Telis [{}]", Self::get_button_name(btn))),
                                };
                                self.step = DecoderStep::Reset;
                                return Some(result);
                            }
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.manchester_state = CommonManchesterState::Mid1;
                        let (new_state, _) = common_manchester_advance(self.manchester_state, 3);
                        self.manchester_state = new_state;
                        self.step = DecoderStep::Reset;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = 1;
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = 3;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }

                if event != -1 {
                    let (new_state, data_bit) = common_manchester_advance(self.manchester_state, event as u8);
                    self.manchester_state = new_state;
                    if let Some(data_bit) = data_bit {
                        self.decode_data = (self.decode_data << 1) | (data_bit as u64);
                        self.decode_count_bit += 1;
                    }
                }
            }
        }

        None
    }

    fn supports_encoding(&self) -> bool {
        false // Requires complex generation not fully implemented
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for SomfyTelisDecoder {
    fn default() -> Self {
        Self::new()
    }
}
