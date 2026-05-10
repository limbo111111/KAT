//! Marantec protocol decoder/encoder
//!
//! Aligned with Flipper-ARF reference: `lib/subghz/protocols/marantec.c`.

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;
use crate::protocols::common::{common_manchester_advance, CommonManchesterState};

const TE_SHORT: u32 = 1000;
const TE_LONG: u32 = 2000;
const TE_DELTA: u32 = 200;
const MIN_COUNT_BIT: usize = 49;

#[derive(Debug, Clone, PartialEq)]
enum DecoderStep {
    Reset,
    DecoderData,
}

pub struct MarantecDecoder {
    step: DecoderStep,
    decode_data: u64,
    decode_count_bit: usize,
    manchester_saved_state: CommonManchesterState,
}

impl MarantecDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            decode_data: 1,
            decode_count_bit: 1,
            manchester_saved_state: CommonManchesterState::Mid1,
        }
    }
}

impl ProtocolDecoder for MarantecDecoder {
    fn name(&self) -> &'static str {
        "Marantec"
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
        self.decode_data = 1;
        self.decode_count_bit = 1;
        self.manchester_saved_state = CommonManchesterState::Mid1;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut event = 4; // 4 means Reset

        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, TE_LONG * 5) < TE_DELTA * 8 {
                    self.step = DecoderStep::DecoderData;
                    self.decode_data = 1;
                    self.decode_count_bit = 1;
                    self.manchester_saved_state = CommonManchesterState::Mid1;
                }
            }
            DecoderStep::DecoderData => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = 0; // ShortLow
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = 2; // LongLow
                    } else if duration >= TE_LONG * 2 + TE_DELTA {
                        let mut decoded_sig = None;
                        if self.decode_count_bit == MIN_COUNT_BIT {
                            decoded_sig = Some(DecodedSignal {
                                serial: None, // No specific serial/btn mapping in Flipper C, just raw
                                button: None,
                                counter: None,
                                crc_valid: true,
                                data: self.decode_data,
                                data_count_bit: self.decode_count_bit,
                                encoder_capable: true,
                                extra: None,
                                protocol_display_name: None,
                            });
                        }

                        self.decode_data = 1;
                        self.decode_count_bit = 1;
                        self.manchester_saved_state = CommonManchesterState::Mid1;

                        return decoded_sig;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        event = 1; // ShortHigh
                    } else if duration_diff!(duration, TE_LONG) < TE_DELTA {
                        event = 3; // LongHigh
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }

                if event != 4 {
                    let (new_state, bit_opt) = common_manchester_advance(
                        self.manchester_saved_state,
                        event,
                    );
                    self.manchester_saved_state = new_state;

                    if let Some(bit) = bit_opt {
                        self.decode_data = (self.decode_data << 1) | (bit as u64);
                        self.decode_count_bit += 1;
                    }
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

        // Preamble logic via ManchesterEncoder. For simplicity we can hardcode the Manchester encoding
        // since Marantec uses standard GE Thomas Manchester where 1 = ShortLow + ShortHigh (if prev was 0),
        // or LongLow + ShortHigh etc depending on previous state. We can track level state manually.

        // Marantec starts with TE_LONG * 5 space (sync)
        upload.push(LevelDuration::new(false, TE_LONG * 5));

        // Standard manchester encode:
        // bit 1 => true then false (each TE_SHORT)
        // bit 0 => false then true
        // But Marantec decoder outputs data. Flipper uses standard `manchester_advance`.
        // GE Thomas (1 = high->low pulse, 0 = low->high pulse) or vice versa.
        // Let's implement basic manchester encoding for the 49 bits.

        // We know that `data` contains 49 bits and starts with a '1' implicitly added on sync.
        // The decoder sets `decode_data = 1`, `decode_count_bit = 1` after header.

        let data = decoded.data;
        let bits = decoded.data_count_bit;

        // Let's just build it using the standard sequence
        for i in (0..bits).rev() {
            let bit = ((data >> i) & 1) == 1;
            if bit {
                upload.push(LevelDuration::new(false, TE_SHORT));
                upload.push(LevelDuration::new(true, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        // Compress the upload (combine adjacent levels)
        let mut compressed = Vec::new();
        if let Some(mut current) = upload.first().cloned() {
            for ld in upload.into_iter().skip(1) {
                if ld.level == current.level {
                    current.duration_us += ld.duration_us;
                } else {
                    compressed.push(current);
                    current = ld;
                }
            }
            compressed.push(current);
        }

        Some(compressed)
    }
}

impl Default for MarantecDecoder {
    fn default() -> Self {
        Self::new()
    }
}
