use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 400;
const TE_LONG: u32 = 800;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT_FOR_FOUND: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Initial,
    Recording,
}

pub struct DickertMahsDecoder {
    step: DecoderStep,
    decode_data: u64,
    decode_count_bit: usize,
    tmp: [u32; 2],
    tmp_cnt: usize,
}

impl DickertMahsDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            decode_data: 0,
            decode_count_bit: 0,
            tmp: [0; 2],
            tmp_cnt: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for DickertMahsDecoder {
    fn name(&self) -> &'static str {
        "Dickert MAHS"
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
        &[433_920_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.tmp_cnt = 0;
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        let mut result = None;

        match self.step {
            DecoderStep::Reset => {
                if self.decode_count_bit >= MIN_COUNT_BIT_FOR_FOUND {
                    let data = self.decode_data;
                    let data_count_bit = self.decode_count_bit;

                    let decoded = DecodedSignal {
                        serial: Some(0),
                        button: Some(0),
                        counter: None,
                        crc_valid: true,
                        data,
                        data_count_bit,
                        encoder_capable: true,
                        extra: None,
                        protocol_display_name: None,
                    };

                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    result = Some(decoded);
                }

                if !level && duration_diff!(duration_us, TE_LONG * 50) < TE_DELTA * 70 {
                    self.step = DecoderStep::Initial;
                }
            }
            DecoderStep::Initial => {
                if level {
                    if duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.step = DecoderStep::Recording;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.tmp_cnt = 0;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }
            }
            DecoderStep::Recording => {
                if (!level && self.tmp_cnt == 0) || (level && self.tmp_cnt == 1) {
                    self.tmp[self.tmp_cnt] = duration_us;
                    self.tmp_cnt += 1;

                    if self.tmp_cnt == 2 {
                        if duration_diff!(self.tmp[0] + self.tmp[1], 1200) < TE_DELTA {
                            if duration_diff!(self.tmp[0], TE_LONG) < TE_DELTA {
                                self.add_bit(1);
                            } else {
                                self.add_bit(0);
                            }
                            self.tmp_cnt = 0;
                        } else {
                            self.tmp_cnt = 0;
                            self.step = DecoderStep::Reset;
                        }
                    }
                } else {
                    self.tmp_cnt = 0;
                    self.step = DecoderStep::Reset;
                }
            }
        }

        result
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        upload.push(LevelDuration::new(false, TE_SHORT * 112));
        upload.push(LevelDuration::new(true, TE_SHORT));

        for i in (0..decoded.data_count_bit).rev() {
            if (decoded.data >> i) & 1 == 1 {
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for DickertMahsDecoder {
    fn default() -> Self {
        Self::new()
    }
}
