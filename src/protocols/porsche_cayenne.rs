//! Porsche Cayenne protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/porsche_cayenne.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 1680;
const TE_LONG: u32 = 3370;
const TE_DELTA: u32 = 500;

const PC_TE_SYNC: u32 = 3370;
const PC_TE_GAP: u32 = 5930;
const PC_SYNC_MIN: u32 = 15;
const PC_SYNC_COUNT: usize = 73;

const MIN_COUNT_BIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    GapHigh,
    GapLow,
    Data,
}

pub struct PorscheCayenneDecoder {
    step: DecoderStep,
    te_last: u32,
    raw_data: u64,
    bit_count: usize,
    sync_count: u32,
}

impl PorscheCayenneDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            raw_data: 0,
            bit_count: 0,
            sync_count: 0,
        }
    }

    fn compute_frame(serial24: u32, btn: u8, counter: u16, frame_type: u8, pkt: &mut [u8; 8]) {
        let b0 = (btn << 4) | (frame_type & 0x07);
        let b1 = ((serial24 >> 16) & 0xFF) as u8;
        let b2 = ((serial24 >> 8) & 0xFF) as u8;
        let b3 = (serial24 & 0xFF) as u8;

        let cnt = counter.wrapping_add(1);
        let cnt_lo = (cnt & 0xFF) as u8;
        let cnt_hi = ((cnt >> 8) & 0xFF) as u8;

        let mut r_h = b3;
        let mut r_m = b1;
        let mut r_l = b2;

        let mut rotate24 = || {
            let ch = (r_h >> 7) & 1;
            let cm = (r_m >> 7) & 1;
            let cl = (r_l >> 7) & 1;
            r_h = (r_h << 1) | cm;
            r_m = (r_m << 1) | cl;
            r_l = (r_l << 1) | ch;
        };

        for _ in 0..4 {
            rotate24();
        }

        for _ in 0..cnt_lo {
            rotate24();
        }

        let a9a = r_h ^ b0;

        let mut nb9b_p1 = ((!cnt_lo) << 2) & 0xFC ^ r_m;
        nb9b_p1 &= 0xCC;
        let mut nb9b_p2 = ((!cnt_hi) << 2) & 0xFC ^ r_m;
        nb9b_p2 &= 0x30;
        let mut nb9b_p3 = ((!cnt_hi) >> 6) & 0x03 ^ r_m;
        nb9b_p3 &= 0x03;
        let a9b = nb9b_p1 | nb9b_p2 | nb9b_p3;

        let mut nb9c_p1 = ((!cnt_lo) >> 2) & 0x3F ^ r_l;
        nb9c_p1 &= 0x33;
        let mut nb9c_p2 = ((!cnt_hi) & 0x03) << 6 ^ r_l;
        nb9c_p2 &= 0xC0;
        let mut nb9c_p3 = ((!cnt_hi) >> 2) & 0x3F ^ r_l;
        nb9c_p3 &= 0x0C;
        let a9c = nb9c_p1 | nb9c_p2 | nb9c_p3;

        pkt[0] = b0;
        pkt[1] = b1;
        pkt[2] = b2;
        pkt[3] = b3;
        pkt[4] = (!a9a) ^ a9b ^ a9c;
        pkt[5] = a9a ^ (!a9b) ^ a9c;
        pkt[6] = a9a ^ a9b ^ (!a9c);
        pkt[7] = (!a9a) ^ (!a9b) ^ (!a9c);
    }
}

impl ProtocolDecoder for PorscheCayenneDecoder {
    fn name(&self) -> &'static str {
        "Porsche Cayenne"
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
        &[433_920_000, 315_000_000] // Common VAG frequencies
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.sync_count = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level {
                    if duration_diff!(duration, PC_TE_SYNC) < TE_DELTA {
                        // wait for LOW
                    } else if self.sync_count >= PC_SYNC_MIN && duration_diff!(duration, PC_TE_GAP) < TE_DELTA {
                        self.step = DecoderStep::GapLow;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.sync_count = 0;
                    }
                } else {
                    if duration_diff!(duration, PC_TE_SYNC) < TE_DELTA {
                        self.sync_count += 1;
                    } else if self.sync_count >= PC_SYNC_MIN && duration_diff!(duration, PC_TE_GAP) < TE_DELTA {
                        self.step = DecoderStep::GapHigh;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.sync_count = 0;
                    }
                }
            }
            DecoderStep::GapHigh => {
                if level && duration_diff!(duration, PC_TE_GAP) < TE_DELTA {
                    self.raw_data = 0;
                    self.bit_count = 0;
                    self.step = DecoderStep::Data;
                } else {
                    self.step = DecoderStep::Reset;
                    self.sync_count = 0;
                }
            }
            DecoderStep::GapLow => {
                if !level && duration_diff!(duration, PC_TE_GAP) < TE_DELTA {
                    self.raw_data = 0;
                    self.bit_count = 0;
                    self.step = DecoderStep::Data;
                } else {
                    self.step = DecoderStep::Reset;
                    self.sync_count = 0;
                }
            }
            DecoderStep::Data => {
                if level {
                    let bit;
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA
                        && duration_diff!(duration, TE_LONG) < TE_DELTA
                    {
                        bit = false;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA
                        && duration_diff!(duration, TE_SHORT) < TE_DELTA
                    {
                        bit = true;
                    } else {
                        self.step = DecoderStep::Reset;
                        self.sync_count = 0;
                        return None;
                    }

                    self.raw_data = (self.raw_data << 1) | (if bit { 1 } else { 0 });
                    self.bit_count += 1;

                    if self.bit_count == 64 {
                        let mut pkt = [0u8; 8];
                        let mut raw = self.raw_data;
                        for i in (0..=7).rev() {
                            pkt[i] = (raw & 0xFF) as u8;
                            raw >>= 8;
                        }

                        let serial = ((pkt[1] as u32) << 16) | ((pkt[2] as u32) << 8) | (pkt[3] as u32);
                        let btn = pkt[0] >> 4;

                        let mut found_cnt = 0;
                        let mut try_pkt = [0u8; 8];
                        for try_cnt in 1..=256 {
                            let ft = pkt[0] & 0x07;
                            Self::compute_frame(serial, btn, (try_cnt - 1) as u16, ft, &mut try_pkt);
                            if try_pkt[4..=7] == pkt[4..=7] {
                                found_cnt = try_cnt as u16;
                                break;
                            }
                        }

                        let res = DecodedSignal {
                            serial: Some(serial),
                            button: Some(btn),
                            counter: Some(found_cnt),
                            crc_valid: found_cnt > 0, // Valid if we could brute force the counter
                            data: self.raw_data,
                            data_count_bit: 64,
                            encoder_capable: true,
                            extra: None,
                            protocol_display_name: None,
                        };

                        self.step = DecoderStep::Reset;
                        self.sync_count = 0;
                        return Some(res);
                    }
                } else {
                    self.te_last = duration;
                }
            }
        }
        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let serial = decoded.serial.unwrap_or(0);
        let cnt = decoded.counter.unwrap_or(0);

        let frame_types = [0b010, 0b001, 0b100, 0b100];
        let mut upload = Vec::new();

        for f in 0..4 {
            let mut pkt = [0u8; 8];
            Self::compute_frame(serial, button, cnt.wrapping_add(f as u16), frame_types[f], &mut pkt);

            // Preamble
            for _ in 0..PC_SYNC_COUNT {
                upload.push(LevelDuration::new(false, TE_LONG));
                upload.push(LevelDuration::new(true, TE_LONG));
            }

            // Gap
            upload.push(LevelDuration::new(false, PC_TE_GAP));
            upload.push(LevelDuration::new(true, PC_TE_GAP));

            // Data
            for byte in 0..8 {
                for bit in (0..=7).rev() {
                    let b = (pkt[byte] >> bit) & 1;
                    if b == 1 {
                        upload.push(LevelDuration::new(false, TE_LONG));
                        upload.push(LevelDuration::new(true, TE_SHORT));
                    } else {
                        upload.push(LevelDuration::new(false, TE_SHORT));
                        upload.push(LevelDuration::new(true, TE_LONG));
                    }
                }
            }
        }

        Some(upload)
    }
}

impl Default for PorscheCayenneDecoder {
    fn default() -> Self {
        Self::new()
    }
}
