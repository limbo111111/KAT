use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;
use crate::protocols::keeloq_common;

const TE_SHORT: u32 = 400;
const TE_LONG: u32 = 800;
const TE_DELTA: u32 = 140;
const MIN_COUNT_BIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SheriffCfmModel {
    ZX750 = 0,
    ZX930 = 1,
}

impl SheriffCfmModel {
    fn name(&self) -> &'static str {
        match self {
            Self::ZX750 => "ZX-750",
            Self::ZX930 => "ZX-930",
        }
    }
}

const CFM_PI_BYTES: [u8; 16] = [
    0xA4, 0x58, 0xFE, 0xA3, 0xF4, 0x93, 0x3D, 0x7E,
    0x0D, 0x95, 0x74, 0x8F, 0x72, 0x8E, 0xB6, 0x58,
];

const CFM_ZX750_ENCODED: [u8; 8] = [
    0x32, 0x4D, 0xCB, 0x84, 0x5F, 0xE9, 0x27, 0xCB,
];

const CFM_ZX930_ENCODED: [u8; 8] = [
    0x94, 0x3B, 0x63, 0xA5, 0xE8, 0xF3, 0xAB, 0x60,
];

fn cfm_pi_decode(encoded: &[u8; 8], pi_offset: usize) -> [u8; 8] {
    let mut out = [0u8; 8];
    for i in 0..8 {
        out[i] = encoded[i] ^ CFM_PI_BYTES[pi_offset + i];
    }
    out
}

fn cfm_rlf(in_val: u8) -> u8 {
    (in_val << 1) | (in_val >> 7)
}

fn cfm_rrf(in_val: u8) -> u8 {
    (in_val >> 1) | (in_val << 7)
}

fn cfm_swap(in_val: u8) -> u8 {
    (in_val << 4) | (in_val >> 4)
}

fn cfm_decrypt_transform(hop: &mut [u8; 4], model: SheriffCfmModel) {
    match model {
        SheriffCfmModel::ZX750 => {
            hop[0] = cfm_swap(hop[0]);
            hop[2] = cfm_swap(hop[2]);
        }
        SheriffCfmModel::ZX930 => {
            hop[0] = !hop[0];
            let temp = hop[1];
            hop[1] = hop[2];
            hop[2] = temp;
            hop[0] = cfm_rrf(hop[0]);
            hop[1] = cfm_swap(hop[1]);
            hop[1] = cfm_rlf(hop[1]);
            hop[1] = cfm_rlf(hop[1]);
        }
    }
}

fn cfm_encrypt_transform(hop: &mut [u8; 4], model: SheriffCfmModel) {
    match model {
        SheriffCfmModel::ZX750 => {
            hop[0] = cfm_swap(hop[0]);
            hop[2] = cfm_swap(hop[2]);
        }
        SheriffCfmModel::ZX930 => {
            hop[1] = cfm_rrf(hop[1]);
            hop[1] = cfm_rrf(hop[1]);
            hop[1] = cfm_swap(hop[1]);
            hop[0] = cfm_rlf(hop[0]);
            hop[0] = !hop[0];
            let temp = hop[1];
            hop[1] = hop[2];
            hop[2] = temp;
        }
    }
}

fn cfm_get_mfkey(model: SheriffCfmModel) -> u64 {
    let dkey = match model {
        SheriffCfmModel::ZX750 => cfm_pi_decode(&CFM_ZX750_ENCODED, 0),
        SheriffCfmModel::ZX930 => cfm_pi_decode(&CFM_ZX930_ENCODED, 8),
    };
    let mut key = 0u64;
    for i in (0..=7).rev() {
        key = (key << 8) | (dkey[i] as u64);
    }
    key
}

fn cfm_try_decrypt(data: u64) -> Option<(SheriffCfmModel, u8, u32, u16)> {
    let hop_encrypted = (data & 0xFFFFFFFF) as u32;
    let fix = (data >> 32) as u32;

    for &model in &[SheriffCfmModel::ZX750, SheriffCfmModel::ZX930] {
        let mut hop_bytes = [
            (hop_encrypted & 0xFF) as u8,
            ((hop_encrypted >> 8) & 0xFF) as u8,
            ((hop_encrypted >> 16) & 0xFF) as u8,
            ((hop_encrypted >> 24) & 0xFF) as u8,
        ];

        cfm_decrypt_transform(&mut hop_bytes, model);

        let hop_transformed = (hop_bytes[0] as u32)
            | ((hop_bytes[1] as u32) << 8)
            | ((hop_bytes[2] as u32) << 16)
            | ((hop_bytes[3] as u32) << 24);

        let mfkey = cfm_get_mfkey(model);
        let decrypted = keeloq_common::keeloq_decrypt(hop_transformed, mfkey);

        let dec_serial_lo = ((decrypted >> 16) & 0x3FF) as u16;
        let fix_serial_lo = (fix & 0x3FF) as u16;

        let btn_byte = ((decrypted >> 24) & 0xFF) as u8;
        let valid_btn = matches!(btn_byte, 0x10 | 0x20 | 0x40 | 0x80);

        if valid_btn && dec_serial_lo == fix_serial_lo {
            let cnt = (decrypted & 0xFFFF) as u16;
            return Some((model, btn_byte, fix, cnt));
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    CheckPreamble,
    SaveDuration,
    CheckDuration,
}

pub struct SheriffCfmDecoder {
    step: DecoderStep,
    header_count: u16,
    decode_data: u64,
    decode_count_bit: usize,
    te_last: u32,
    model: Option<SheriffCfmModel>,
}

impl SheriffCfmDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            header_count: 0,
            decode_data: 0,
            decode_count_bit: 0,
            te_last: 0,
            model: None,
        }
    }
}

impl ProtocolDecoder for SheriffCfmDecoder {
    fn name(&self) -> &'static str {
        "Sheriff CFM"
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
        self.decode_data = 0;
        self.decode_count_bit = 0;
        self.te_last = 0;
        self.model = None;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::CheckPreamble;
                    self.header_count += 1;
                }
            }
            DecoderStep::CheckPreamble => {
                if !level && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                    self.step = DecoderStep::Reset;
                } else if self.header_count > 2 && duration_diff!(duration, TE_SHORT * 10) < TE_DELTA * 10 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                    self.header_count = 0;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    self.te_last = duration;
                    self.step = DecoderStep::CheckDuration;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration >= (TE_SHORT * 2 + TE_DELTA) {
                        self.step = DecoderStep::Reset;
                        if self.decode_count_bit >= MIN_COUNT_BIT && self.decode_count_bit <= MIN_COUNT_BIT + 2 {
                            if let Some((model, btn, serial, cnt)) = cfm_try_decrypt(self.decode_data) {
                                self.model = Some(model);
                                let result = DecodedSignal {
                                    serial: Some(serial),
                                    button: Some(btn),
                                    counter: Some(cnt),
                                    crc_valid: true,
                                    data: self.decode_data,
                                    data_count_bit: MIN_COUNT_BIT,
                                    encoder_capable: true,
                                    extra: Some(model as u64),
                                    protocol_display_name: None,
                                };
                                self.decode_data = 0;
                                self.decode_count_bit = 0;
                                self.header_count = 0;
                                return Some(result);
                            }
                        }
                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                        self.header_count = 0;
                    } else if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA && duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                        if self.decode_count_bit < MIN_COUNT_BIT {
                            self.decode_data = (self.decode_data << 1) | 1;
                        }
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA * 2 && duration_diff!(duration, TE_SHORT) < TE_DELTA {
                        if self.decode_count_bit < MIN_COUNT_BIT {
                            self.decode_data = (self.decode_data << 1) | 0;
                        }
                        self.decode_count_bit += 1;
                        self.step = DecoderStep::SaveDuration;
                    } else {
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
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let serial = decoded.serial?;
        let cnt = decoded.counter.unwrap_or(0).wrapping_add(1);

        let fix = serial;
        let serial_lo = (fix & 0x3FF) as u16;
        let hop_plain = ((button as u32) << 24) | ((serial_lo as u32) << 16) | (cnt as u32);

        let model = self.model.unwrap_or(SheriffCfmModel::ZX750);
        let mfkey = cfm_get_mfkey(model);
        let hop_encrypted = keeloq_common::keeloq_encrypt(hop_plain, mfkey);

        let mut hop_bytes = [
            (hop_encrypted & 0xFF) as u8,
            ((hop_encrypted >> 8) & 0xFF) as u8,
            ((hop_encrypted >> 16) & 0xFF) as u8,
            ((hop_encrypted >> 24) & 0xFF) as u8,
        ];
        cfm_encrypt_transform(&mut hop_bytes, model);

        let hop_transformed = (hop_bytes[0] as u32)
            | ((hop_bytes[1] as u32) << 8)
            | ((hop_bytes[2] as u32) << 16)
            | ((hop_bytes[3] as u32) << 24);

        let data = ((fix as u64) << 32) | (hop_transformed as u64);

        let mut out = Vec::new();
        // Preamble: 11 pairs of short high, short low
        for _ in 0..11 {
            out.push(LevelDuration::new(true, TE_SHORT));
            out.push(LevelDuration::new(false, TE_SHORT));
        }
        out.push(LevelDuration::new(true, TE_SHORT));
        out.push(LevelDuration::new(false, TE_SHORT * 10));

        // Data: 64 bits (MSB first)
        for i in (0..MIN_COUNT_BIT).rev() {
            if (data >> i) & 1 == 1 {
                out.push(LevelDuration::new(true, TE_SHORT));
                out.push(LevelDuration::new(false, TE_LONG));
            } else {
                out.push(LevelDuration::new(true, TE_LONG));
                out.push(LevelDuration::new(false, TE_SHORT));
            }
        }

        out.push(LevelDuration::new(true, TE_SHORT));
        out.push(LevelDuration::new(false, TE_SHORT * 40));

        Some(out)
    }
}

impl Default for SheriffCfmDecoder {
    fn default() -> Self {
        Self::new()
    }
}
