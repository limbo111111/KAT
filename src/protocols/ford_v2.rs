//! Ford V2 protocol decoder
//!
//! Aligned with Flipper-ARF reference: `ford_v2.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz FM
//! - 104 bits (13 bytes) Manchester encoding
//! - te_short = 200 µs, te_long = 400 µs
//! - Sync word: 0x7F, 0xA7
//! - Fields: [16:47] 32-bit serial, [48:55] button, [56:71] counter (16 bit), tail
//! - Buttons: 0x10=Lock, 0x11=Unlock, 0x12=Trunk, 0x14=Panic, 0x15=RemoteStart

use super::common::DecodedSignal;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};
use super::{ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 200;
const TE_LONG: u32 = 400;
const TE_DELTA: u32 = 260; // From Flipper-ARF (it uses a large delta)
const MIN_COUNT_BIT: usize = 104;
const PREAMBLE_MIN: u16 = 64;
const DATA_BYTES: usize = 13;
const SYNC_0: u8 = 0x7F;
const SYNC_1: u8 = 0xA7;
const SYNC_SHIFT_INV: u16 = !(((SYNC_0 as u16) << 8) | (SYNC_1 as u16));
const SYNC_BITS: u16 = 16;
const GAP_US: u32 = 15000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    Sync,
    Data,
}

pub struct FordV2Decoder {
    step: DecoderStep,
    te_last: u32,
    preamble_count: u16,
    sync_shift: u16,
    sync_bit_count: u16,
    manchester_state: CommonManchesterState,
    decode_data: u64,
    decode_count_bit: usize,
    raw_bytes: [u8; DATA_BYTES],
    byte_count: usize,
}

impl FordV2Decoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            preamble_count: 0,
            sync_shift: 0,
            sync_bit_count: 0,
            manchester_state: CommonManchesterState::Start0,
            decode_data: 0,
            decode_count_bit: 0,
            raw_bytes: [0u8; DATA_BYTES],
            byte_count: 0,
        }
    }

    fn manchester_advance(&mut self, event: u8) -> Option<bool> {
        let (next_state, bit) = common_manchester_advance(self.manchester_state, event);
        self.manchester_state = next_state;
        bit
    }

    fn check_sync(&mut self, data_bit: bool) -> bool {
        self.sync_shift = (self.sync_shift << 1) | (if data_bit { 1 } else { 0 });
        if self.sync_bit_count < SYNC_BITS {
            self.sync_bit_count += 1;
        }
        self.sync_bit_count >= SYNC_BITS && self.sync_shift == SYNC_SHIFT_INV
    }

    fn process_data(&self) -> Option<DecodedSignal> {
        if self.raw_bytes[0] != SYNC_0 || self.raw_bytes[1] != SYNC_1 {
            return None;
        }

        let k = &self.raw_bytes;

        let serial = ((k[2] as u32) << 24) | ((k[3] as u32) << 16) | ((k[4] as u32) << 8) | (k[5] as u32);
        let btn = k[6];
        let cnt = (((k[7] & 0x7F) as u16) << 9) | ((k[8] as u16) << 1) | ((k[9] as u16) >> 7);
        let tail31 = (((k[9] & 0x7F) as u32) << 24) | ((k[10] as u32) << 16) | ((k[11] as u32) << 8) | (k[12] as u32);

        let button_name = match btn {
            0x10 => Some("Lock"),
            0x11 => Some("Unlock"),
            0x12 => Some("Trunk"),
            0x14 => Some("Panic"),
            0x15 => Some("RemoteStart"),
            _ => Some("Unknown"),
        };

        let mut key1: u64 = 0;
        for i in 0..8 {
            key1 = (key1 << 8) | (k[i] as u64);
        }

        let mut extra_data: u64 = 0;
        for i in 0..5 {
            extra_data = (extra_data << 8) | (k[8 + i] as u64);
        }

        let mut sig = DecodedSignal {
            serial: Some(serial),
            button: Some(btn),
            counter: Some(cnt),
            crc_valid: true, // No explicit CRC in Ford V2, relies on sync + structure
            data: key1,
            data_count_bit: MIN_COUNT_BIT,
            encoder_capable: false, // Encode support omitted to match protocol features mapping
            protocol_display_name: Some("Ford V2".to_string()),
            extra: Some(extra_data),
        };

        if let Some(name) = button_name {
            sig.protocol_display_name = Some(format!("Ford V2 ({})", name));
            sig.extra = Some(tail31 as u64); // Store tail in extra for info
        }

        Some(sig)
    }
}

impl ProtocolDecoder for FordV2Decoder {
    fn name(&self) -> &'static str {
        "Ford V2"
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
        &[433_920_000, 868_350_000] // Typical Ford frequencies
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_last = 0;
        self.preamble_count = 0;
        self.sync_shift = 0;
        self.sync_bit_count = 0;
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.byte_count = 0;
        self.raw_bytes = [0u8; DATA_BYTES];
        self.manchester_state = CommonManchesterState::Start0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let is_short = duration_diff!(duration, TE_SHORT) < TE_DELTA;
        let is_long = duration_diff!(duration, TE_LONG) < TE_DELTA;

        match self.step {
            DecoderStep::Reset => {
                if is_short {
                    self.preamble_count = 1;
                    self.step = DecoderStep::Preamble;
                }
            }

            DecoderStep::Preamble => {
                if is_short {
                    if self.preamble_count < 0xFFFF {
                        self.preamble_count += 1;
                    }
                } else if !level && is_long {
                    if self.preamble_count >= PREAMBLE_MIN {
                        self.step = DecoderStep::Sync;
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.byte_count = 0;
                        self.sync_shift = 0;
                        self.sync_bit_count = 0;
                        self.raw_bytes = [0u8; DATA_BYTES];
                        self.manchester_state = CommonManchesterState::Start0;

                        let ev = if is_short {
                            if level { 1 } else { 0 }
                        } else if is_long {
                            if level { 3 } else { 2 }
                        } else {
                            self.reset();
                            return None;
                        };

                        if ev == 0 || ev == 2 {
                            self.manchester_state = CommonManchesterState::Mid0;
                        }

                        self.manchester_advance(ev);
                    } else {
                        self.reset();
                    }
                } else {
                    self.reset();
                }
            }

            DecoderStep::Sync | DecoderStep::Data => {
                let event = if is_short {
                    Some(if level { 1 } else { 0 })
                } else if is_long {
                    Some(if level { 3 } else { 2 })
                } else {
                    None
                };

                if let Some(ev) = event {
                    if self.step == DecoderStep::Sync {
                        if let Some(bit) = self.manchester_advance(ev) {
                            if self.check_sync(bit) {
                                self.raw_bytes[0] = SYNC_0;
                                self.raw_bytes[1] = SYNC_1;
                                self.byte_count = 2;
                                self.step = DecoderStep::Data;
                                self.decode_data = 0;
                                self.decode_count_bit = 16;
                            }
                        }
                    } else {
                        if let Some(bit) = self.manchester_advance(ev) {
                            // In Flipper-ARF the logic is: bit is added directly, no inversion.
                            self.decode_data = (self.decode_data << 1) | (if bit { 1 } else { 0 });
                            self.decode_count_bit += 1;

                            if self.decode_count_bit % 8 == 0 {
                                if self.byte_count < DATA_BYTES {
                                    self.raw_bytes[self.byte_count] = (self.decode_data & 0xFF) as u8;
                                    self.byte_count += 1;
                                }
                                self.decode_data = 0;

                                if self.byte_count == DATA_BYTES {
                                    let res = self.process_data();
                                    self.reset();
                                    return res;
                                }
                            }
                        }
                    }
                } else {
                    if duration >= GAP_US {
                        self.reset();
                    } else {
                        self.reset();
                    }
                }
            }
        }

        self.te_last = duration;
        None
    }

    fn supports_encoding(&self) -> bool {
        false
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for FordV2Decoder {
    fn default() -> Self {
        Self::new()
    }
}
