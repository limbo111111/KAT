use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 300;
const TE_LONG: u32 = 600;
const TE_DELTA: u32 = 150;

const BIT_PERIOD: u32 = 4000;
const BIT_TOLERANCE: u32 = 800;
const PREAMBLE_MIN: u16 = 15;
const PREAMBLE_GAP: u32 = 10000;
const DATA_BITS: usize = 80;
const SHORT_MAX: u32 = 450;
const LONG_MIN: u32 = 450;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    Gap,
    Data,
}

pub struct ChryslerDecoder {
    step: DecoderStep,
    te_last: u32,
    preamble_count: u16,
    raw_data: [u8; 10],
    bit_count: usize,
}

impl ChryslerDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            preamble_count: 0,
            raw_data: [0; 10],
            bit_count: 0,
        }
    }

    fn validate(&self) -> bool {
        let d = &self.raw_data;
        let msb = (d[0] >> 7) & 1;

        if msb == 0 {
            if d[5] != (d[1] ^ 0xC3) { return false; }
        } else {
            if d[5] != d[1] { return false; }
        }

        let b1_xor_b6 = d[1] ^ d[6];
        if msb == 0 {
            if b1_xor_b6 != 0x04 && b1_xor_b6 != 0x08 { return false; }
        } else {
            if b1_xor_b6 != 0x62 { return false; }
        }

        let mask2 = d[2] ^ d[7];
        let mask3 = d[3] ^ d[8];
        let mask4 = d[4] ^ d[9];

        if msb == 0 {
            if mask2 != 0x63 || mask3 != 0x59 || mask4 != 0x46 { return false; }
        } else {
            if mask2 != 0x9A || mask3 != 0xC6 { return false; }
            if mask4 != 0x20 && mask4 != 0x10 { return false; }
        }

        true
    }

    fn parse_data(&self) -> DecodedSignal {
        let d = &self.raw_data;

        let cnt_raw = (d[0] >> 4) & 0xF;
        let cnt = Self::reverse_nibble(cnt_raw);
        let dev_id = d[0] & 0xF;
        let msb = (d[0] >> 7) & 1;

        let b1_xor_b6 = d[1] ^ d[6];
        let btn = if msb == 0 {
            if b1_xor_b6 == 0x04 {
                0x01
            } else if b1_xor_b6 == 0x08 {
                0x02
            } else {
                0x00
            }
        } else {
            0xFF // Can't distinguish from MSB=1 mask
        };

        let serial = ((d[1] ^ d[2]) as u32) << 24 |
                     ((d[1] ^ d[3]) as u32) << 16 |
                     ((d[1] ^ d[4]) as u32) << 8 |
                     (dev_id as u32);

        let data = ((d[0] as u64) << 56) | ((d[1] as u64) << 48) |
                   ((d[2] as u64) << 40) | ((d[3] as u64) << 32) |
                   ((d[4] as u64) << 24) | ((d[5] as u64) << 16) |
                   ((d[6] as u64) << 8) | (d[7] as u64);

        let extra = ((d[8] as u64) << 8) | (d[9] as u64);

        DecodedSignal {
            serial: Some(serial),
            button: Some(if btn != 0xFF { btn } else { 0 }),
            counter: Some(cnt as u16),
            crc_valid: true,
            data,
            data_count_bit: DATA_BITS,
            encoder_capable: true,
            extra: Some(extra),
            protocol_display_name: None,
        }
    }

    fn reverse_nibble(n: u8) -> u8 {
        ((n & 1) << 3) | ((n & 2) << 1) | ((n & 4) >> 1) | ((n & 8) >> 3)
    }

    fn advance_rolling(d: &mut [u8]) {
        let msb = (d[0] >> 7) & 1;
        let mut rolling = [0u8; 4];
        let mut button = [0u8; 4];

        for i in 0..4 {
            if msb == 0 {
                rolling[i] = (d[1 + i] >> 4) & 0xF;
                button[i] = d[1 + i] & 0xF;
            } else {
                rolling[i] = d[1 + i] & 0xF;
                button[i] = (d[1 + i] >> 4) & 0xF;
            }
        }

        let cnt_raw = (d[0] >> 4) & 0xF;
        let mut cnt = Self::reverse_nibble(cnt_raw);
        cnt = cnt.wrapping_sub(1) & 0xF;
        let new_cnt_raw = Self::reverse_nibble(cnt);
        let new_msb = (new_cnt_raw >> 3) & 1;

        d[0] = (new_cnt_raw << 4) | (d[0] & 0x0F);

        for i in 0..4 {
            if new_msb == 0 {
                d[1 + i] = (rolling[i] << 4) | (button[i] & 0xF);
            } else {
                d[1 + i] = ((button[i] & 0xF) << 4) | rolling[i];
            }
        }
    }

    fn rebuild_encoder(d: &mut [u8], btn: u8) {
        let msb = (d[0] >> 7) & 1;

        let b1_xor_b6 = if msb == 0 {
            if btn == 0x01 { 0x04 } else { 0x08 }
        } else {
            0x62
        };

        d[5] = if msb == 0 { d[1] ^ 0xC3 } else { d[1] };
        d[6] = d[1] ^ b1_xor_b6;

        if msb == 0 {
            d[7] = d[2] ^ 0x63;
            d[8] = d[3] ^ 0x59;
            d[9] = d[4] ^ 0x46;
        } else {
            d[7] = d[2] ^ 0x9A;
            d[8] = d[3] ^ 0xC6;
            d[9] = d[4] ^ if btn == 0x01 { 0x20 } else { 0x10 };
        }
    }
}

impl ProtocolDecoder for ChryslerDecoder {
    fn name(&self) -> &'static str {
        "Chrysler"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: TE_DELTA,
            min_count_bit: 80,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000, 315_000_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.preamble_count = 0;
        self.bit_count = 0;
        self.raw_data.fill(0);
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        let mut result = None;

        match self.step {
            DecoderStep::Reset => {
                if level && duration_us <= SHORT_MAX && duration_us > 100 {
                    self.te_last = duration_us;
                    self.step = DecoderStep::Preamble;
                    self.preamble_count = 1;
                }
            }
            DecoderStep::Preamble => {
                if !level {
                    let total = self.te_last + duration_us;
                    if duration_diff!(total, BIT_PERIOD) < BIT_TOLERANCE && self.te_last <= SHORT_MAX {
                        self.preamble_count += 1;
                    } else if duration_us > PREAMBLE_GAP && self.preamble_count >= PREAMBLE_MIN {
                        self.step = DecoderStep::Gap;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    if duration_us <= SHORT_MAX && duration_us > 100 {
                        self.te_last = duration_us;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }
            }
            DecoderStep::Gap => {
                if level {
                    self.te_last = duration_us;
                    self.bit_count = 0;
                    self.raw_data.fill(0);
                    self.step = DecoderStep::Data;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::Data => {
                if level {
                    self.te_last = duration_us;
                } else {
                    let total = self.te_last + duration_us;
                    if duration_diff!(total, BIT_PERIOD) < BIT_TOLERANCE {
                        let bit_val = self.te_last >= LONG_MIN;

                        if self.bit_count < DATA_BITS {
                            let byte_idx = self.bit_count / 8;
                            let bit_pos = 7 - (self.bit_count % 8);
                            if bit_val {
                                self.raw_data[byte_idx] |= 1 << bit_pos;
                            }
                            self.bit_count += 1;
                        }

                        if self.bit_count == DATA_BITS {
                            if self.validate() {
                                result = Some(self.parse_data());
                            }
                            self.step = DecoderStep::Reset;
                        }
                    } else {
                        if self.bit_count >= DATA_BITS {
                            if self.validate() {
                                result = Some(self.parse_data());
                            }
                        }
                        self.step = DecoderStep::Reset;
                    }
                }
            }
        }
        result
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut d = [0u8; 10];
        let data = decoded.data;
        for i in 0..8 {
            d[i] = (data >> (56 - i * 8)) as u8;
        }
        if let Some(extra) = decoded.extra {
            d[8] = (extra >> 8) as u8;
            d[9] = extra as u8;
        }

        Self::advance_rolling(&mut d);

        let custom_btn = if button == 0x01 {
            0x01
        } else if button == 0x02 {
            0x02
        } else {
            decoded.button.unwrap_or(0)
        };

        Self::rebuild_encoder(&mut d, custom_btn);

        let mut upload = Vec::new();

        // Preamble: 24 zero bits
        for _ in 0..24 {
            upload.push(LevelDuration::new(true, TE_SHORT));
            upload.push(LevelDuration::new(false, BIT_PERIOD - TE_SHORT));
        }

        // Gap
        if let Some(last) = upload.last_mut() {
            *last = LevelDuration::new(false, 15600);
        }

        // Data
        for bit_i in 0..DATA_BITS {
            let byte_idx = bit_i / 8;
            let bit_pos = 7 - (bit_i % 8);
            let data_bit = (d[byte_idx] >> bit_pos) & 1;

            let high_dur = if data_bit == 1 { 600 } else { TE_SHORT };
            let low_dur = BIT_PERIOD - high_dur;

            upload.push(LevelDuration::new(true, high_dur));
            upload.push(LevelDuration::new(false, low_dur));
        }

        // Final gap
        if let Some(last) = upload.last_mut() {
            *last = LevelDuration::new(false, 15600);
        }

        Some(upload)
    }
}

impl Default for ChryslerDecoder {
    fn default() -> Self {
        Self::new()
    }
}
