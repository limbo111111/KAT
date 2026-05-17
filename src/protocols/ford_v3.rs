//! Ford V3 protocol decoder
//!
//! Aligned with Flipper-ARF reference: `ford_v3.c`.
//!
//! Protocol characteristics:
//! - 433.92 MHz FM
//! - 136 bits (17 bytes) Manchester encoding
//! - te_short = 200 µs, te_long = 400 µs
//! - Sync word: 0x7F, 0xA7
//! - CRC16 (poly 0x1021) on bytes 3..15
//! - Fields: [32:55] 24-bit serial
//! - Crypt chunk: bytes 7..14 (8 bytes). `btn` at crypt[4], `counter` at crypt[5..6].

use super::common::DecodedSignal;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};
use super::{ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 200;
const TE_LONG: u32 = 400;
const TE_DELTA: u32 = 260;
const MIN_COUNT_BIT: usize = 136;
const PREAMBLE_MIN: u16 = 64;
const DATA_BYTES: usize = 17;
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

pub struct FordV3Decoder {
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

impl FordV3Decoder {
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

    fn crc16(data: &[u8]) -> u16 {
        let mut crc: u16 = 0x0000;
        for &byte in data {
            crc ^= (byte as u16) << 8;
            for _ in 0..8 {
                if (crc & 0x8000) != 0 {
                    crc = (crc << 1) ^ 0x1021;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }

    fn decrypt(enc_block: &[u8; 8], crypt_out: &mut [u8; 8]) {
        let mut crypt = [0u8; 8];
        crypt.copy_from_slice(enc_block);

        let mut parity = 0;
        let mut tmp = crypt[1];
        while tmp > 0 {
            parity ^= tmp & 1;
            tmp >>= 1;
        }

        if parity != 0 {
            let mask = enc_block[6];
            for i in 1..=6 {
                crypt[i] ^= mask;
            }
        } else {
            let mask = enc_block[5];
            for i in 1..=5 {
                crypt[i] ^= mask;
            }
            crypt[7] ^= mask;
        }

        let c6 = crypt[6];
        let c7 = crypt[7];
        crypt[6] = (c6 & 0xAA) | (c7 & 0x55);
        crypt[7] = (c7 & 0xAA) | (c6 & 0x55);

        crypt_out.copy_from_slice(&crypt);
    }

    fn process_data(&self) -> Option<DecodedSignal> {
        let k = &self.raw_bytes;

        if k[0] != SYNC_0 || k[1] != SYNC_1 {
            return None;
        }

        let serial = ((k[4] as u32) << 16) | ((k[5] as u32) << 8) | (k[6] as u32);

        let mut crypt_buf = [0u8; 8];
        let mut enc_block = [0u8; 8];
        enc_block.copy_from_slice(&k[7..15]);
        Self::decrypt(&enc_block, &mut crypt_buf);

        let btn = crypt_buf[4];
        let counter16 = ((crypt_buf[5] as u16) << 8) | (crypt_buf[6] as u16);

        let crc_received = ((k[15] as u16) << 8) | (k[16] as u16);
        let crc_computed = Self::crc16(&k[3..15]);

        if crc_received != crc_computed {
            return None;
        }

        let button_name;
        match btn {
            0x10 => button_name = Some("Lock"),
            0x20 => button_name = Some("Unlock"),
            0x40 => button_name = Some("Trunk"),
            _ => button_name = Some("Unknown"),
        }

        let mut key1: u64 = 0;
        for i in 0..8 {
            key1 = (key1 << 8) | (k[i] as u64);
        }

        let mut sig = DecodedSignal {
            serial: Some(serial),
            button: Some(btn),
            counter: Some(counter16),
            crc_valid: true,
            data: key1,
            data_count_bit: MIN_COUNT_BIT,
            encoder_capable: false,
            protocol_display_name: Some("Ford V3".to_string()),
            extra: None,
        };

        if let Some(name) = button_name {
            sig.protocol_display_name = Some(format!("Ford V3 ({})", name));
        }

        Some(sig)
    }
}

impl ProtocolDecoder for FordV3Decoder {
    fn name(&self) -> &'static str {
        "Ford V3"
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
                    self.preamble_count = self.preamble_count.saturating_add(1);
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
                                self.decode_count_bit = 16; // Start counting from sync word
                            }
                        }
                    } else {
                        if let Some(bit) = self.manchester_advance(ev) {
                            self.decode_data = (self.decode_data << 1) | (if bit { 1 } else { 0 });
                            self.decode_count_bit += 1;

                            if self.decode_count_bit.is_multiple_of(8) {
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

impl Default for FordV3Decoder {
    fn default() -> Self {
        Self::new()
    }
}
