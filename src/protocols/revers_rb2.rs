//! Revers RB2 protocol decoder/encoder
//!
//! Aligned with ProtoPirate reference: `REFERENCES/ProtoPirate/protocols/revers_rb2.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz AM, 64 bits
//! - TE ~250us short, 500us long
//! - Manchester Encoding (ShortLow, LongLow, ShortHigh, LongHigh)
//! - Wait for GAP < 600, wait for 4 Header events, extract bits, check 0xFF and 0x200 markers

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 250;
const TE_LONG: u32 = 500;
const TE_DELTA: u32 = 160;
const MIN_COUNT_BIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ManchesterState {
    Mid0,
    Mid1,
    Start0,
    Start1,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Header,
    DecoderData,
}

pub struct ReversRb2Decoder {
    step: DecoderStep,
    header_count: u16,
    decode_data: u64,
    decode_count_bit: usize,
    manchester_saved_state: ManchesterState,
    te_last: bool,
}

impl ReversRb2Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            header_count: 0,
            decode_data: 0,
            decode_count_bit: 0,
            manchester_saved_state: ManchesterState::Mid1,
            te_last: false,
        }
    }

    fn manchester_advance(&mut self, event_is_short: bool, event_is_high: bool) -> Option<bool> {
        match self.manchester_saved_state {
            ManchesterState::Mid1 => {
                if event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Start1;
                    None
                } else if !event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Mid0 => {
                if event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Start0;
                    None
                } else if !event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start1 => {
                if event_is_short && event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    Some(true)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
            ManchesterState::Start0 => {
                if event_is_short && !event_is_high {
                    self.manchester_saved_state = ManchesterState::Mid0;
                    Some(false)
                } else {
                    self.manchester_saved_state = ManchesterState::Mid1;
                    None
                }
            }
        }
    }

    fn add_bit(&mut self, bit: bool) -> Option<DecodedSignal> {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;

        if self.decode_count_bit >= 65 {
            self.decode_data = 0;
            self.decode_count_bit = 0;
            return None;
        }

        if self.decode_count_bit < MIN_COUNT_BIT {
            return None;
        }

        let preamble = (self.decode_data >> 48) & 0xFF;
        let stop_code = self.decode_data & 0x3FF;

        if preamble == 0xFF && stop_code == 0x200 {
            let data = self.decode_data;
            let bit_count = self.decode_count_bit;

            // Revers RB2 / RB2M Decoder
            // instance->serial = (((instance->data << 16) >> 16) >> 10);
            let serial = (((data << 16) >> 16) >> 10) as u32;

            self.decode_data = 0;
            self.decode_count_bit = 0;
            self.manchester_saved_state = ManchesterState::Mid1;

            return Some(DecodedSignal {
                serial: Some(serial),
                button: None,
                counter: None,
                crc_valid: true,
                data,
                data_count_bit: bit_count,
                encoder_capable: true,
                extra: None,
                protocol_display_name: None,
            });
        }
        None
    }
}

impl ProtocolDecoder for ReversRb2Decoder {
    fn name(&self) -> &'static str {
        "Revers_RB2"
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
        self.header_count = 0;
        self.manchester_saved_state = ManchesterState::Mid1;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if !level && duration_diff!(duration, 600) < TE_DELTA {
                    self.step = DecoderStep::Header;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                    self.manchester_saved_state = ManchesterState::Mid1;
                }
            }
            DecoderStep::Header => {
                if !level {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        if self.te_last {
                            self.header_count += 1;
                        }
                        self.te_last = level;
                    } else {
                        self.header_count = 0;
                        self.te_last = false;
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        if !self.te_last {
                            self.header_count += 1;
                        }
                        self.te_last = level;
                    } else {
                        self.header_count = 0;
                        self.te_last = false;
                        self.step = DecoderStep::Reset;
                    }
                }

                if self.header_count == 4 {
                    self.header_count = 0;
                    self.decode_data = 0xF;
                    self.decode_count_bit = 4;
                    self.step = DecoderStep::DecoderData;
                }
            }
            DecoderStep::DecoderData => {
                let is_short = duration_diff!(duration, TE_SHORT) < TE_DELTA;
                let is_long = duration_diff!(duration, TE_LONG) < TE_DELTA;

                if is_short || is_long {
                    if let Some(bit) = self.manchester_advance(is_short, level) {
                        if let Some(signal) = self.add_bit(bit) {
                            return Some(signal);
                        }
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
        let data = decoded.data;
        let mut signal = Vec::with_capacity((decoded.data_count_bit * 2) + 2);

        let mut enc_state = ManchesterState::Mid1;

        let add_duration = |sig: &mut Vec<LevelDuration>, event_is_short: bool, event_is_high: bool| {
            let dur = if event_is_short { TE_SHORT } else { TE_LONG };
            sig.push(LevelDuration::new(event_is_high, dur));
        };

        for i in (0..decoded.data_count_bit).rev() {
            let bit = (data >> i) & 1 == 1;
            let (is_adv, is_short, is_high) = match enc_state {
                ManchesterState::Mid1 => {
                    if bit {
                        enc_state = ManchesterState::Start1;
                        (false, true, false) // ShortLow
                    } else {
                        enc_state = ManchesterState::Mid0;
                        (false, false, true) // LongHigh
                    }
                }
                ManchesterState::Mid0 => {
                    if bit {
                        enc_state = ManchesterState::Mid1;
                        (false, false, false) // LongLow
                    } else {
                        enc_state = ManchesterState::Start0;
                        (false, true, true) // ShortHigh
                    }
                }
                ManchesterState::Start1 => {
                    enc_state = ManchesterState::Mid1;
                    (true, true, true) // ShortHigh
                }
                ManchesterState::Start0 => {
                    enc_state = ManchesterState::Mid0;
                    (true, true, false) // ShortLow
                }
            };

            if is_adv {
                add_duration(&mut signal, is_short, is_high);
                let (_, s2, h2) = match enc_state {
                    ManchesterState::Mid1 => {
                        if bit {
                            enc_state = ManchesterState::Start1;
                            (false, true, false)
                        } else {
                            enc_state = ManchesterState::Mid0;
                            (false, false, true)
                        }
                    }
                    ManchesterState::Mid0 => {
                        if bit {
                            enc_state = ManchesterState::Mid1;
                            (false, false, false)
                        } else {
                            enc_state = ManchesterState::Start0;
                            (false, true, true)
                        }
                    }
                    _ => (false, true, false)
                };
                add_duration(&mut signal, s2, h2);
            } else {
                add_duration(&mut signal, is_short, is_high);
            }
        }

        let (is_short, is_high) = match enc_state {
            ManchesterState::Mid1 => (true, false),
            ManchesterState::Mid0 => (true, true),
            ManchesterState::Start1 => (true, true),
            ManchesterState::Start0 => (true, false),
        };
        add_duration(&mut signal, is_short, is_high);

        if let Some(last) = signal.last() {
            if last.level {
                // index++ emulation
            }
        }
        signal.push(LevelDuration::new(false, 320));

        Some(signal)
    }
}

impl Default for ReversRb2Decoder {
    fn default() -> Self {
        Self::new()
    }
}
