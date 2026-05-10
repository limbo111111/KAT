//! Land Rover RKE protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/landrover_rke.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 400; // Actually bit parts are 400/600/300/700 but let's use base
const TE_LONG: u32 = 9600;

const LR_PREAMBLE_HIGH_US: u32 = 400;
const LR_PREAMBLE_LOW_US: u32 = 600;
const LR_PREAMBLE_COUNT: u32 = 20;

const LR_SYNC_HIGH_US: u32 = 400;
const LR_SYNC_LOW_US: u32 = 9600;

const LR_BIT1_HIGH_US: u32 = 700;
const LR_BIT1_LOW_US: u32 = 300;
const LR_BIT0_HIGH_US: u32 = 300;
const LR_BIT0_LOW_US: u32 = 700;

const LR_REPEAT_GAP_US: u32 = 12000;
const LR_REPEAT_COUNT: u32 = 4;
const LR_TOLERANCE_PCT: u32 = 20;

const LR_FRAME_BITS: usize = 66;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    SaveDuration,
    CheckDuration,
}

pub struct LandRoverRkeDecoder {
    step: DecoderStep,
    te_last: u32,
    bits: [u8; LR_FRAME_BITS],
    bit_count: usize,
}

impl LandRoverRkeDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            bits: [0; LR_FRAME_BITS],
            bit_count: 0,
        }
    }

    fn in_range(measured: u32, ref_val: u32) -> bool {
        let diff = if measured > ref_val { measured - ref_val } else { ref_val - measured };
        (diff * 100) <= (ref_val * LR_TOLERANCE_PCT)
    }
}

impl ProtocolDecoder for LandRoverRkeDecoder {
    fn name(&self) -> &'static str {
        "Land Rover RKE"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: 200, // Approximate
            min_count_bit: LR_FRAME_BITS,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000, 315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_last = 0;
        self.bit_count = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level {
                    if Self::in_range(duration, LR_SYNC_HIGH_US) {
                        self.te_last = duration;
                        self.step = DecoderStep::SaveDuration;
                    }
                }
            }
            DecoderStep::SaveDuration => {
                if !level {
                    if Self::in_range(self.te_last, LR_SYNC_HIGH_US) && Self::in_range(duration, LR_SYNC_LOW_US) {
                        self.bit_count = 0;
                        self.step = DecoderStep::CheckDuration;
                        return None;
                    }
                }
                self.step = DecoderStep::Reset;
            }
            DecoderStep::CheckDuration => {
                if level {
                    self.te_last = duration;
                } else {
                    let hi = self.te_last;
                    let lo = duration;

                    if Self::in_range(hi, LR_BIT1_HIGH_US) && Self::in_range(lo, LR_BIT1_LOW_US) {
                        if self.bit_count < LR_FRAME_BITS {
                            self.bits[65 - self.bit_count] = 1;
                        }
                        self.bit_count += 1;
                    } else if Self::in_range(hi, LR_BIT0_HIGH_US) && Self::in_range(lo, LR_BIT0_LOW_US) {
                        if self.bit_count < LR_FRAME_BITS {
                            self.bits[65 - self.bit_count] = 0;
                        }
                        self.bit_count += 1;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.bit_count = 0;
                        return None;
                    }

                    if self.bit_count == LR_FRAME_BITS {
                        let mut hop_code: u32 = 0;
                        for k in 0..32 {
                            hop_code |= (self.bits[65 - k] as u32) << (31 - k);
                        }

                        let mut serial: u32 = 0;
                        for k in 0..24 {
                            serial |= (self.bits[33 - k] as u32) << (23 - k);
                        }

                        let mut button: u8 = 0;
                        for k in 0..4 {
                            button |= self.bits[9 - k] << (3 - k);
                        }

                        let mut func_bits: u8 = 0;
                        for k in 0..4 {
                            func_bits |= self.bits[5 - k] << (3 - k);
                        }

                        let status = (self.bits[1] << 1) | self.bits[0];

                        let extra = ((func_bits as u64) << 8) | (status as u64);

                        let decoded_sig = DecodedSignal {
                            serial: Some(serial),
                            button: Some(button),
                            counter: None, // Requires Keeloq dec for counter
                            crc_valid: true, // No CRC, if geometry passed it's valid
                            data: hop_code as u64,
                            data_count_bit: LR_FRAME_BITS,
                            encoder_capable: true,
                            extra: Some(extra),
                            protocol_display_name: None,
                        };

                        self.step = DecoderStep::Reset;
                        self.bit_count = 0;
                        return Some(decoded_sig);
                    }
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();

        let hop_code = decoded.data as u32;
        let serial = decoded.serial.unwrap_or(0);
        let func_bits = (decoded.extra.unwrap_or(0) >> 8) as u8;
        let status = (decoded.extra.unwrap_or(0) & 0xFF) as u8;

        let mut bits = [0u8; LR_FRAME_BITS];

        for k in 0..32 {
            bits[65 - k] = ((hop_code >> (31 - k)) & 1) as u8;
        }
        for k in 0..24 {
            bits[33 - k] = ((serial >> (23 - k)) & 1) as u8;
        }
        for k in 0..4 {
            bits[9 - k] = (button >> (3 - k)) & 1;
        }
        for k in 0..4 {
            bits[5 - k] = (func_bits >> (3 - k)) & 1;
        }
        bits[1] = (status >> 1) & 1;
        bits[0] = status & 1;

        for rep in 0..LR_REPEAT_COUNT {
            // Preamble
            for _ in 0..LR_PREAMBLE_COUNT {
                upload.push(LevelDuration::new(true, LR_PREAMBLE_HIGH_US));
                upload.push(LevelDuration::new(false, LR_PREAMBLE_LOW_US));
            }

            // Sync
            upload.push(LevelDuration::new(true, LR_SYNC_HIGH_US));
            upload.push(LevelDuration::new(false, LR_SYNC_LOW_US));

            // Data
            for b in (0..LR_FRAME_BITS).rev() {
                if bits[b] == 1 {
                    upload.push(LevelDuration::new(true, LR_BIT1_HIGH_US));
                    upload.push(LevelDuration::new(false, LR_BIT1_LOW_US));
                } else {
                    upload.push(LevelDuration::new(true, LR_BIT0_HIGH_US));
                    upload.push(LevelDuration::new(false, LR_BIT0_LOW_US));
                }
            }

            // Inter-repetition gap
            if rep < LR_REPEAT_COUNT - 1 {
                upload.push(LevelDuration::new(false, LR_REPEAT_GAP_US));
            }
        }

        Some(upload)
    }
}

impl Default for LandRoverRkeDecoder {
    fn default() -> Self {
        Self::new()
    }
}
