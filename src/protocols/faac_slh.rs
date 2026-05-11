use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 255;
const TE_LONG: u32 = 595;
const TE_DELTA: u32 = 100;
const MIN_COUNT_BIT_FOR_FOUND: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecoderStep {
    Reset,
    FoundPreambula,
    SaveDuration,
    CheckDuration,
}

pub struct FaacSlhDecoder {
    step: DecoderStep,
    te_last: u32,
    decode_data: u64,
    decode_count_bit: usize,
}

impl FaacSlhDecoder {
    pub fn new() -> Self {
        Self {
            step: DecoderStep::Reset,
            te_last: 0,
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn add_bit(&mut self, bit: u8) {
        self.decode_data = (self.decode_data << 1) | (bit as u64);
        self.decode_count_bit += 1;
    }
}

impl ProtocolDecoder for FaacSlhDecoder {
    fn name(&self) -> &'static str {
        "FAAC SLH"
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
        &[433_920_000, 868_350_000]
    }

    fn reset(&mut self) {
        self.step = DecoderStep::Reset;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration_us: u32) -> Option<DecodedSignal> {
        match self.step {
            DecoderStep::Reset => {
                if level && duration_diff!(duration_us, TE_LONG * 2) < TE_DELTA * 3 {
                    self.step = DecoderStep::FoundPreambula;
                }
            }
            DecoderStep::FoundPreambula => {
                if !level && duration_diff!(duration_us, TE_LONG * 2) < TE_DELTA * 3 {
                    self.step = DecoderStep::SaveDuration;
                    self.decode_data = 0;
                    self.decode_count_bit = 0;
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::SaveDuration => {
                if level {
                    if duration_us >= TE_SHORT * 3 + TE_DELTA {
                        self.step = DecoderStep::FoundPreambula;
                        if self.decode_count_bit == MIN_COUNT_BIT_FOR_FOUND {
                            let data = self.decode_data;
                            let data_count_bit = self.decode_count_bit;

                            let code_fix = (data >> 32) as u32;
                            let code_hop = (data & 0xFFFFFFFF) as u32;

                            let mut data_prg = [0u8; 8];
                            data_prg[0] = (code_hop & 0xFF) as u8;
                            data_prg[1] = ((code_hop >> 8) & 0xFF) as u8;
                            data_prg[2] = ((code_hop >> 16) & 0xFF) as u8;
                            data_prg[3] = (code_hop >> 24) as u8;
                            data_prg[4] = (code_fix & 0xFF) as u8;
                            data_prg[5] = ((code_fix >> 8) & 0xFF) as u8;
                            data_prg[6] = ((code_fix >> 16) & 0xFF) as u8;
                            data_prg[7] = (code_fix >> 24) as u8;

                            let mut is_prog_mode = false;
                            let mut seed = 0u32;
                            let mut cnt = 0u16;

                            if data_prg[7] == 0x52 && data_prg[6] == 0x0F && data_prg[0] == 0x00 {
                                is_prog_mode = true;
                                for _ in 0..(data_prg[1] & 0xF) {
                                    let data_tmp = data_prg[2];
                                    data_prg[2] = (data_prg[2] >> 1) | ((data_prg[3] & 1) << 7);
                                    data_prg[3] = (data_prg[3] >> 1) | ((data_prg[4] & 1) << 7);
                                    data_prg[4] = (data_prg[4] >> 1) | ((data_prg[5] & 1) << 7);
                                    data_prg[5] = (data_prg[5] >> 1) | ((data_tmp & 1) << 7);
                                }
                                data_prg[2] ^= data_prg[1];
                                data_prg[3] ^= data_prg[1];
                                data_prg[4] ^= data_prg[1];
                                data_prg[5] ^= data_prg[1];
                                seed = (data_prg[5] as u32) << 24 | (data_prg[4] as u32) << 16 | (data_prg[3] as u32) << 8 | (data_prg[2] as u32);
                                cnt = data_prg[1] as u16;
                            } else {
                                // For normal remotes, if we have the FAAC SLH manufacturer key in the keystore,
                                // we can decrypt code_hop. However, real FAAC SLH requires the 'seed' to derive the learning key.
                                // If the keystore somehow contains a learning key or if we assume simple decryption:
                                let mf_key = crate::protocols::keys::get_keystore().get_faac_slh_key();
                                if mf_key != 0 {
                                    // Normally FAAC uses keeloq_faac_learning(seed, mf_key) but without seed we can't derive it.
                                    // If mf_key is already a learning key, we can try decrypting directly:
                                    let decrypted = crate::protocols::keeloq_common::keeloq_decrypt(code_hop, mf_key);
                                    cnt = (decrypted & 0xFFFF) as u16;
                                } else {
                                    cnt = 0;
                                }
                            }

                            let decoded = DecodedSignal {
                                serial: Some(code_fix >> 4),
                                button: Some((code_fix & 0xF) as u8),
                                counter: Some(cnt),
                                crc_valid: true,
                                data,
                                data_count_bit,
                                encoder_capable: true,
                                extra: if is_prog_mode { Some(seed as u64) } else { None }, // Send seed via extra if needed
                                protocol_display_name: Some("FAAC SLH".to_string()),
                            };

                            self.decode_data = 0;
                            self.decode_count_bit = 0;
                            return Some(decoded);
                        }

                        self.decode_data = 0;
                        self.decode_count_bit = 0;
                    } else {
                        self.te_last = duration_us;
                        self.step = DecoderStep::CheckDuration;
                    }
                } else {
                    self.step = DecoderStep::Reset;
                }
            }
            DecoderStep::CheckDuration => {
                if !level {
                    if duration_diff!(self.te_last, TE_SHORT) < TE_DELTA &&
                       duration_diff!(duration_us, TE_LONG) < TE_DELTA {
                        self.add_bit(0);
                        self.step = DecoderStep::SaveDuration;
                    } else if duration_diff!(self.te_last, TE_LONG) < TE_DELTA &&
                              duration_diff!(duration_us, TE_SHORT) < TE_DELTA {
                        self.add_bit(1);
                        self.step = DecoderStep::SaveDuration;
                    } else {
                        self.step = DecoderStep::Reset;
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
        let mut upload = Vec::with_capacity(decoded.data_count_bit * 2 + 2);

        // Preamble
        upload.push(LevelDuration::new(true, TE_LONG * 2));
        upload.push(LevelDuration::new(false, TE_LONG * 2));

        let mut encode_data = decoded.data;

        // In real FAAC SLH we would regenerate the Keeloq part if we know the seed and manufacture key
        // For simple replay without rolling, we just replay the data

        for i in (0..decoded.data_count_bit).rev() {
            if (encode_data >> i) & 1 == 1 {
                upload.push(LevelDuration::new(true, TE_LONG));
                upload.push(LevelDuration::new(false, TE_SHORT));
            } else {
                upload.push(LevelDuration::new(true, TE_SHORT));
                upload.push(LevelDuration::new(false, TE_LONG));
            }
        }

        Some(upload)
    }
}

impl Default for FaacSlhDecoder {
    fn default() -> Self {
        Self::new()
    }
}
