use super::common::{add_bit, reverse_key};
use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 500;
const TE_LONG: u32 = 1000;
const TE_DELTA: u32 = 250;
const MIN_COUNT_BIT_FOR_FOUND: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreamble,
    SaveDuration,
    CheckDuration,
}

pub struct JaroliftDecoder {
    step: DecoderStep,
    te_last: u32,
    header_count: u16,
    decode_data: u64,
    decode_data_2: u64,
    decode_count_bit: usize,
}

impl JaroliftDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            header_count: 0,
            decode_data: 0,
            decode_data_2: 0,
            decode_count_bit: 0,
        }
    }
}

impl ProtocolDecoder for JaroliftDecoder {
    fn name(&self) -> &'static str {
        "Jarolift"
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
        self.te_last = 0;
        self.header_count = 0;
        self.decode_data = 0;
        self.decode_data_2 = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_LONG) < TE_DELTA {
                    self.step = DecoderStep::CheckPreamble;
                    self.te_last = duration;
                    self.header_count = 0;
                }
            }
            DecoderStep::CheckPreamble => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.te_last = duration;
                    } else if duration >= TE_LONG * 3 {
                        if self.header_count > 6 {
                            self.step = DecoderStep::SaveDuration;
                            self.decode_data = 0;
                            self.decode_data_2 = 0;
                            self.decode_count_bit = 0;
                        } else {
                            self.step = DecoderStep::Reset;
                            self.header_count = 0;
                        }
                    } else {
                        self.step = DecoderStep::Reset;
                        self.header_count = 0;
                    }
                } else {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        self.header_count += 1;
                        self.te_last = duration;
                    } else if duration_diff!(duration, 1500) < TE_DELTA {
                        self.header_count += 1;
                        self.te_last = duration;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.header_count = 0;
                    }
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.step = DecoderStep::CheckDuration;
                } else {
                    self.header_count = 0;
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if self.decode_count_bit == 64 {
                        self.decode_data_2 = self.decode_data;
                        self.decode_data = 0;
                    }
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, true);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        if duration >= TE_LONG * 3 {
                            if duration_diff!(self.te_last, TE_LONG) < TE_DELTA {
                                add_bit(&mut self.decode_data, &mut self.decode_count_bit, false);
                            } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA {
                                add_bit(&mut self.decode_data, &mut self.decode_count_bit, true);
                            }

                            if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                                let data_2 = self.decode_data; // This is the last 8 bits
                                let data_1 = self.decode_data_2; // This is the first 64 bits

                                self.step = DecoderStep::Reset;
                                self.decode_data = 0;
                                self.decode_count_bit = 0;
                                self.header_count = 0;

                                // Process the received data like `subghz_protocol_jarolift_remote_controller` does:
                                let _group = reverse_key(data_2, 8) as u32;
                                let key = reverse_key(data_1, 64);

                                let serial = ((key >> 32) & 0xFFFFFFF) as u32;
                                let _hop = (key & 0xFFFFFFFF) as u32;
                                let btn = ((key >> 60) & 0xF) as u8;

                                // Since we don't have Keeloq learning/keystore decryption in KAT's DecodedSignal directly,
                                // we just return the raw payload as is common in KAT for Keeloq variants lacking keys.
                                // We can use the data fields.
                                // We pack data_1 as data and use extra for data_2 if needed. Actually we'll just populate what we can.

                                return Some(DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn),
                                    counter: None, // needs decryption
                                    crc_valid: true,
                                    data: data_1, // 64-bit part
                                    data_count_bit: MIN_COUNT_BIT_FOR_FOUND,
                                    encoder_capable: false,
                                    extra: Some(data_2), // use extra for data_2
                                    protocol_display_name: None,
                                });
                            }
                        }
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

impl Default for JaroliftDecoder {
    fn default() -> Self {
        Self::new()
    }
}
