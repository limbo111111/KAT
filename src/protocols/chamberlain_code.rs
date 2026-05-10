use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 1000;
const TE_LONG: u32 = 3000;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 10;

const CHAMBERLAIN_CODE_BIT_STOP: u64 = 0b0001;
const CHAMBERLAIN_CODE_BIT_1: u64 = 0b0011;
const CHAMBERLAIN_CODE_BIT_0: u64 = 0b0111;

const CHAMBERLAIN_7_CODE_MASK: u64 = 0xF000000FF0F;
const CHAMBERLAIN_8_CODE_MASK: u64 = 0xF00000F00F;
const CHAMBERLAIN_9_CODE_MASK: u64 = 0xF000000000F;

const CHAMBERLAIN_7_CODE_MASK_CHECK: u64 = 0x10000001101;
const CHAMBERLAIN_8_CODE_MASK_CHECK: u64 = 0x1000001001;
const CHAMBERLAIN_9_CODE_MASK_CHECK: u64 = 0x10000000001;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundStartBit,
    SaveDuration,
    CheckDuration,
}

pub struct ChamberlainCodeDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl ChamberlainCodeDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn check_mask_and_parse(&self, mut decode_data: u64, mut decode_count_bit: usize) -> Option<(u64, usize)> {
        if decode_count_bit > MIN_COUNT_BIT + 1 {
            return None;
        }

        if (decode_data & CHAMBERLAIN_7_CODE_MASK) == CHAMBERLAIN_7_CODE_MASK_CHECK {
            decode_count_bit = 7;
            decode_data &= !CHAMBERLAIN_7_CODE_MASK;
            decode_data = (decode_data >> 12) | ((decode_data >> 4) & 0xF);
        } else if (decode_data & CHAMBERLAIN_8_CODE_MASK) == CHAMBERLAIN_8_CODE_MASK_CHECK {
            decode_count_bit = 8;
            decode_data &= !CHAMBERLAIN_8_CODE_MASK;
            decode_data = (decode_data >> 4) | (CHAMBERLAIN_CODE_BIT_0 << 8); // DIP 6 no use
        } else if (decode_data & CHAMBERLAIN_9_CODE_MASK) == CHAMBERLAIN_9_CODE_MASK_CHECK {
            decode_count_bit = 9;
            decode_data &= !CHAMBERLAIN_9_CODE_MASK;
            decode_data >>= 4;
        } else {
            return None;
        }

        // Convert to bit
        let mut data_tmp = decode_data;
        let mut data_res = 0;
        for i in 0..decode_count_bit {
            if (data_tmp & 0xF) == CHAMBERLAIN_CODE_BIT_0 {
                // bit_write(data_res, i, 0)
                // keeping it 0
            } else if (data_tmp & 0xF) == CHAMBERLAIN_CODE_BIT_1 {
                // bit_write(data_res, i, 1)
                data_res |= 1 << i;
            } else {
                return None;
            }
            data_tmp >>= 4;
        }

        Some((data_res, decode_count_bit))
    }
}

impl ProtocolDecoder for ChamberlainCodeDecoder {
    fn name(&self) -> &'static str {
        "Chamberlain Code"
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
        self.te_last = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_SHORT * 39) < TE_DELTA * 20 {
                    self.step = DecoderStep::FoundStartBit;
                }
            }
            DecoderStep::FoundStartBit => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.decode_data = (self.decode_data << 4) | CHAMBERLAIN_CODE_BIT_STOP;
                    self.decode_count_bit += 1;
                    self.step = DecoderStep::SaveDuration;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if duration > TE_SHORT * 5 {
                        if self.decode_count_bit >= MIN_COUNT_BIT {
                            if let Some((parsed_data, parsed_count)) = self.check_mask_and_parse(self.decode_data, self.decode_count_bit) {
                                let result = DecodedSignal {
                                    serial: Some(0),
                                    button: Some(0),
                                    counter: None,
                                    crc_valid: true,
                                    data: parsed_data,
                                    data_count_bit: parsed_count,
                                    encoder_capable: true,
                                    extra: None,
                                    protocol_display_name: None,
                                };
                                self.step = DecoderStep::Reset;
                                return Some(result);
                            }
                        }
                        self.step = DecoderStep::Reset;
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
                    if duration_diff!(self.te_last, TE_SHORT * 3) < TE_DELTA && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.decode_data = (self.decode_data << 4) | CHAMBERLAIN_CODE_BIT_STOP;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT * 2) < TE_DELTA && duration_diff!(duration, TE_SHORT * 2) < TE_DELTA {
                        self.decode_data = (self.decode_data << 4) | CHAMBERLAIN_CODE_BIT_1;
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA && duration_diff!(duration, TE_SHORT * 3) < TE_DELTA {
                        self.decode_data = (self.decode_data << 4) | CHAMBERLAIN_CODE_BIT_0;
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
        let mut upload = Vec::new();

        let mut data_res = 0;
        for i in 0..decoded.data_count_bit {
            let bit = (decoded.data >> (decoded.data_count_bit - i - 1)) & 1;
            if bit == 0 {
                data_res = (data_res << 4) | CHAMBERLAIN_CODE_BIT_0;
            } else {
                data_res = (data_res << 4) | CHAMBERLAIN_CODE_BIT_1;
            }
        }

        let mut data = data_res;
        match decoded.data_count_bit {
            7 => {
                data = ((data >> 4) << 16) | ((data & 0xF) << 4) | CHAMBERLAIN_7_CODE_MASK_CHECK;
            }
            8 => {
                data = ((data >> 12) << 16) | ((data & 0xFF) << 4) | CHAMBERLAIN_8_CODE_MASK_CHECK;
            }
            9 => {
                data = (data << 4) | CHAMBERLAIN_9_CODE_MASK_CHECK;
            }
            _ => return None,
        }

        // Insert guard time (36 * 0s) -> gap
        upload.push(LevelDuration::new(false, TE_SHORT * 36));

        // Insert data bits
        let bit_length = match decoded.data_count_bit {
            7 | 9 => 44,
            8 => 40,
            _ => return None,
        };

        for i in (0..bit_length).rev() {
            let bit = (data >> i) & 1;
            if bit == 0 {
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
            }
        }

        // Fix missing logic in raw bit pushing: Flipper-ARF builds a bit array then converts to LevelDuration based on bit sequences.
        // It converts sequence of bits into TE_SHORT long levels.

        let mut final_upload: Vec<LevelDuration> = Vec::new();
        for level_dur in upload {
            // Because original C code uses `subghz_protocol_blocks_get_upload_from_bit_array`
            // we will combine consecutive same-level `TE_SHORT`s.
            if let Some(last) = final_upload.last_mut() {
                let current_level = level_dur.level;
                if current_level == last.level {
                    last.duration_us += level_dur.duration_us;
                    continue;
                }
            }
            final_upload.push(level_dur);
        }

        Some(final_upload)
    }
}

impl Default for ChamberlainCodeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
