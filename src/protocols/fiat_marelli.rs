use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};

use crate::radio::demodulator::LevelDuration;
use crate::protocols::common::{CommonManchesterState, common_manchester_advance};

const TE_SHORT: u32 = 260; // type A default, detect from preamble
const TE_LONG: u32 = 520;

const PREAMBLE_PULSE_MIN: u32 = 50;
const PREAMBLE_PULSE_MAX: u32 = 350;
const PREAMBLE_MIN: u16 = 80;
const MAX_DATA_BITS: usize = 104;
const MIN_DATA_BITS: usize = 80;
const GAP_TE_MULT: u32 = 4;
const SYNC_TE_MIN_MULT: u32 = 4;
const SYNC_TE_MAX_MULT: u32 = 12;
const RETX_GAP_MIN: u32 = 5000;
const RETX_SYNC_MIN: u32 = 400;
const RETX_SYNC_MAX: u32 = 3500;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    Preamble,
    Sync,
    RetxSync,
    Data,
}

pub struct FiatMarelliDecoder {
    step: DecoderStep,
    te_detected: u32,
    te_sum: u32,
    te_count: u32,
    te_last: u32,
    preamble_count: u16,
    manchester_state: CommonManchesterState,
    raw_data: [u8; 13],
    bit_count: usize,
    decode_data: u64,
    extra_data: u64,
}

impl FiatMarelliDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_detected: 0,
            te_sum: 0,
            te_count: 0,
            te_last: 0,
            preamble_count: 0,
            manchester_state: CommonManchesterState::Mid1,
            raw_data: [0; 13],
            bit_count: 0,
            decode_data: 0,
            extra_data: 0,
        }
    }

    fn prepare_data(&mut self) {
        self.step = DecoderStep::Data;
        self.manchester_state = CommonManchesterState::Mid1;
        self.raw_data.fill(0);
        self.bit_count = 0;
        self.decode_data = 0;
        self.extra_data = 0;
    }

    fn crc8(data: &[u8], len: usize) -> u8 {
        let mut crc = 0xFF;
        for i in 0..len {
            crc ^= data[i];
            for _ in 0..8 {
                if (crc & 0x80) != 0 {
                    crc = (crc << 1) ^ 0x31;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }
}

impl ProtocolDecoder for FiatMarelliDecoder {
    fn name(&self) -> &'static str {
        "Fiat Marelli"
    }

    fn timing(&self) -> ProtocolTiming {
        ProtocolTiming {
            te_short: TE_SHORT,
            te_long: TE_LONG,
            te_delta: 130,
            min_count_bit: MIN_DATA_BITS,
        }
    }

    fn supported_frequencies(&self) -> &[u32] {
        &[433_920_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.te_detected = 0;
        self.te_sum = 0;
        self.te_count = 0;
        self.te_last = 0;
        self.preamble_count = 0;
        self.bit_count = 0;
        self.decode_data = 0;
        self.extra_data = 0;
        self.raw_data.fill(0);
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        let te_short = if self.te_detected > 0 { self.te_detected } else { TE_SHORT };
        let te_long = te_short * 2;
        let mut te_delta = te_short / 2;
        if te_delta < 30 { te_delta = 30; }

        let mut result = None;

        match self.step {
            DecoderStep::Reset => {
                if level {
                    if (PREAMBLE_PULSE_MIN..=PREAMBLE_PULSE_MAX).contains(&duration_us) {
                        self.step = DecoderStep::Preamble;
                        self.preamble_count = 1;
                        self.te_sum = duration_us;
                        self.te_count = 1;
                        self.te_last = duration_us;
                    }
                } else {
                    if duration_us > RETX_GAP_MIN {
                        self.step = DecoderStep::RetxSync;
                        self.te_last = duration_us;
                    }
                }
            }
            DecoderStep::Preamble => {
                if (PREAMBLE_PULSE_MIN..=PREAMBLE_PULSE_MAX).contains(&duration_us) {
                    self.preamble_count += 1;
                    self.te_sum += duration_us;
                    self.te_count += 1;
                    self.te_last = duration_us;
                } else if !level {
                    if self.preamble_count >= PREAMBLE_MIN && self.te_count > 0 {
                        self.te_detected = self.te_sum / self.te_count;
                        let gap_threshold = self.te_detected * GAP_TE_MULT;
                        if duration_us > gap_threshold {
                            self.step = DecoderStep::Sync;
                            self.te_last = duration_us;
                        } else {
                            self.step = DecoderStep::Reset;
                        }
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::Sync => {
                let sync_min = self.te_detected * SYNC_TE_MIN_MULT;
                let sync_max = self.te_detected * SYNC_TE_MAX_MULT;
                if level && duration_us >= sync_min && duration_us <= sync_max {
                    self.prepare_data();
                    self.te_last = duration_us;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::RetxSync => {
                if level && (RETX_SYNC_MIN..=RETX_SYNC_MAX).contains(&duration_us) {
                    if self.te_detected == 0 {
                        self.te_detected = duration_us / 8;
                        if self.te_detected < 70 { self.te_detected = 100; }
                        if self.te_detected > 350 { self.te_detected = 260; }
                    }
                    self.prepare_data();
                    self.te_last = duration_us;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::Data => {
                let mut event = 0xFF;
                let mut frame_complete = false;

                let diff_short = duration_us.abs_diff(te_short);
                if diff_short < te_delta {
                    event = if level { 0 } else { 1 }; // ShortLow : ShortHigh
                } else {
                    let diff_long = duration_us.abs_diff(te_long);
                    if diff_long < te_delta {
                        event = if level { 2 } else { 3 }; // LongLow : LongHigh
                    }
                }

                if event != 0xFF {
                    let (new_state, data_bit_opt) = common_manchester_advance(self.manchester_state, event);
                    self.manchester_state = new_state;

                    if let Some(data_bit) = data_bit_opt {
                        let new_bit = if data_bit { 1 } else { 0 };

                        if self.bit_count < MAX_DATA_BITS {
                            let byte_idx = self.bit_count / 8;
                            let bit_pos = 7 - (self.bit_count % 8);
                            if new_bit == 1 {
                                self.raw_data[byte_idx] |= 1 << bit_pos;
                            }
                        }

                        if self.bit_count < 64 {
                            self.decode_data = (self.decode_data << 1) | new_bit;
                        } else {
                            self.extra_data = (self.extra_data << 1) | new_bit;
                        }

                        self.bit_count += 1;

                        if self.bit_count >= MAX_DATA_BITS {
                            frame_complete = true;
                        }
                    }
                } else {
                    if self.bit_count >= MIN_DATA_BITS {
                        frame_complete = true;
                    } else {
                        self.step = DecoderStep::Reset;
                    }
                }

                if frame_complete {
                    let mut crc_ok = true;
                    if self.bit_count >= 104 {
                        let calc = Self::crc8(&self.raw_data, 12);
                        crc_ok = calc == self.raw_data[12];
                    }

                    if crc_ok {
                        let serial = ((self.raw_data[2] as u32) << 24) |
                                     ((self.raw_data[3] as u32) << 16) |
                                     ((self.raw_data[4] as u32) << 8) |
                                     (self.raw_data[5] as u32);
                        let btn = (self.raw_data[6] >> 4) & 0xF;
                        let cnt = (self.raw_data[7] >> 3) & 0x1F;

                        result = Some(DecodedSignal {
                            serial: Some(serial),
                            button: Some(btn),
                            counter: Some(cnt as u16),
                            crc_valid: crc_ok,
                            data: self.decode_data,
                            data_count_bit: self.bit_count,
                            encoder_capable: true,
                            extra: Some(self.extra_data),
                            protocol_display_name: None,
                        });
                    }
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
        let te = if self.te_detected > 0 { self.te_detected } else { TE_SHORT };
        let te_short = te;
        let te_long = te * 2;
        let gap_duration = te * 12;
        let sync_duration = te * 8;

        let data_bits = decoded.data_count_bit;
        if !(MIN_DATA_BITS..=MAX_DATA_BITS).contains(&data_bits) {
            return None;
        }

        // Rebuild raw_data
        let mut raw_data = [0u8; 13];
        let key = decoded.data;
        for i in 0..8 {
            raw_data[i] = (key >> (56 - i * 8)) as u8;
        }
        if let Some(extra) = decoded.extra {
            for i in 8..13 {
                raw_data[i] = (extra >> (32 - (i - 8) * 8)) as u8;
            }
        }

        let mut upload = Vec::new();

        for i in 0..100 {
            upload.push(LevelDuration::new(true, te_short));
            if i < 99 {
                upload.push(LevelDuration::new(false, te_short));
            }
        }

        upload.push(LevelDuration::new(false, te_short + gap_duration));
        upload.push(LevelDuration::new(true, sync_duration));

        let mut in_mid1 = true;

        for bit_i in 0..data_bits {
            let byte_idx = bit_i / 8;
            let bit_pos = 7 - (bit_i % 8);
            let data_bit = (raw_data[byte_idx] >> bit_pos) & 1 == 1;

            if in_mid1 {
                if data_bit {
                    upload.push(LevelDuration::new(false, te_short));
                    upload.push(LevelDuration::new(true, te_short));
                } else {
                    upload.push(LevelDuration::new(false, te_long));
                    in_mid1 = false;
                }
            } else {
                if data_bit {
                    upload.push(LevelDuration::new(true, te_long));
                    in_mid1 = true;
                } else {
                    upload.push(LevelDuration::new(true, te_short));
                    upload.push(LevelDuration::new(false, te_short));
                }
            }
        }

        Some(upload)
    }
}

impl Default for FiatMarelliDecoder {
    fn default() -> Self {
        Self::new()
    }
}
