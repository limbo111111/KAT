use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};

const TE_SHORT: u32 = 200;
const TE_LONG: u32 = 400;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT_FOR_FOUND: usize = 64;

const PREAMBLE_PAIRS: u16 = 150;
const GAP_US: u32 = 800;
const TOTAL_BURSTS: u8 = 3;
const INTER_BURST_GAP: u32 = 25000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    Data,
}

pub struct FiatSpaDecoder {
    step: DecoderStep,
    preamble_count: u16,
    manchester_state: CommonManchesterState,
    data_low: u32,
    data_high: u32,
    bit_count: u8,
    hop: u32,
    fix: u32,
    endbyte: u8,
}

impl FiatSpaDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            preamble_count: 0,
            manchester_state: CommonManchesterState::Mid1,
            data_low: 0,
            data_high: 0,
            bit_count: 0,
            hop: 0,
            fix: 0,
            endbyte: 0,
        }
    }
}

impl ProtocolDecoder for FiatSpaDecoder {
    fn name(&self) -> &'static str {
        "Fiat Spa"
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
        self.preamble_count = 0;
        self.data_low = 0;
        self.data_high = 0;
        self.bit_count = 0;
        self.hop = 0;
        self.fix = 0;
        self.endbyte = 0;
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        let mut result = None;

        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                    self.data_low = 0;
                    self.data_high = 0;
                    self.step = DecoderStep::Preamble;
                    self.preamble_count = 0;
                    self.bit_count = 0;
                    self.manchester_state = CommonManchesterState::Mid1; // ManchesterEventReset maps roughly to Mid1 conceptually in this context
                }
            }
            DecoderStep::Preamble => {
                if level {
                    if duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.preamble_count += 1;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.preamble_count += 1;
                    } else {
                        if self.preamble_count >= PREAMBLE_PAIRS
                            && duration_diff!(duration_us, GAP_US) < TE_DELTA {
                                self.step = DecoderStep::Data;
                                self.preamble_count = 0;
                                self.data_low = 0;
                                self.data_high = 0;
                                self.bit_count = 0;
                                self.manchester_state = CommonManchesterState::Mid1;
                                return None;
                            }
                        self.step = DecoderStep::Reset;
                    }
                }
            }
            DecoderStep::Data => {
                let mut event = 0xFF;
                if duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                    event = if level { 0 } else { 1 };
                } else if duration_diff!(duration_us, TE_LONG) < TE_DELTA {
                    event = if level { 2 } else { 3 };
                }

                if event != 0xFF {
                    let (new_state, data_bit_opt) = common_manchester_advance(self.manchester_state, event);
                    self.manchester_state = new_state;

                    if let Some(data_bit) = data_bit_opt {
                        let new_bit = if data_bit { 1 } else { 0 };
                        let carry = (self.data_low >> 31) & 1;
                        self.data_low = (self.data_low << 1) | new_bit;
                        self.data_high = (self.data_high << 1) | carry;
                        self.bit_count += 1;

                        if self.bit_count == 64 {
                            self.fix = self.data_low;
                            self.hop = self.data_high;
                            self.data_low = 0;
                            self.data_high = 0;
                        }

                        if self.bit_count == 0x47 { // 71 bits
                            self.endbyte = (self.data_low & 0x3F) as u8;
                            let data = ((self.hop as u64) << 32) | (self.fix as u64);

                            result = Some(DecodedSignal {
                                serial: Some(self.fix),
                                button: Some(self.endbyte),
                                counter: Some(self.hop as u16),
                                crc_valid: true,
                                data,
                                data_count_bit: 71,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                            self.step = DecoderStep::Reset;
                        }
                    }
                } else {
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
        if decoded.data_count_bit != 71 {
            return None;
        }

        let fix = (decoded.data & 0xFFFFFFFF) as u32;
        let hop = (decoded.data >> 32) as u32;
        let endbyte = decoded.button.unwrap_or(0);

        let data = ((hop as u64) << 32) | (fix as u64);
        let endbyte_to_send = endbyte >> 1; // Assuming `instance->endbyte >> 1` matching C logic

        let mut upload = Vec::new();

        for burst in 0..TOTAL_BURSTS {
            if burst > 0 {
                upload.push(LevelDuration::new(false, INTER_BURST_GAP));
            }

            for _ in 0..PREAMBLE_PAIRS {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }

            if let Some(last) = upload.last_mut() {
                *last = LevelDuration::new(false, GAP_US);
            }

            let first_bit = ((data >> 63) & 1) == 1;
            if first_bit {
                upload.push(LevelDuration::new(true, TE_LONG));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }

            let mut prev_bit = first_bit;

            for bit in (0..=62).rev() {
                let curr_bit = ((data >> bit) & 1) == 1;
                if !prev_bit && !curr_bit {
                    upload.push(LevelDuration::new(true, TE_SHORT));
                    upload.push(LevelDuration::new(false, TE_SHORT));
                } else if !prev_bit && curr_bit {
                    upload.push(LevelDuration::new(true, TE_LONG));
                } else if prev_bit && !curr_bit {
                    upload.push(LevelDuration::new(false, TE_LONG));
                } else {
                    upload.push(LevelDuration::new(false, TE_SHORT));
                    upload.push(LevelDuration::new(true, TE_SHORT));
                }
                prev_bit = curr_bit;
            }

            for bit in (0..=5).rev() {
                let curr_bit = ((endbyte_to_send >> bit) & 1) == 1;
                if !prev_bit && !curr_bit {
                    upload.push(LevelDuration::new(true, TE_SHORT));
                    upload.push(LevelDuration::new(false, TE_SHORT));
                } else if !prev_bit && curr_bit {
                    upload.push(LevelDuration::new(true, TE_LONG));
                } else if prev_bit && !curr_bit {
                    upload.push(LevelDuration::new(false, TE_LONG));
                } else {
                    upload.push(LevelDuration::new(false, TE_SHORT));
                    upload.push(LevelDuration::new(true, TE_SHORT));
                }
                prev_bit = curr_bit;
            }
        }

        Some(upload)
    }
}

impl Default for FiatSpaDecoder {
    fn default() -> Self {
        Self::new()
    }
}
