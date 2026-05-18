//! Power Smart protocol decoder and encoder
//!
//! Aligned with Flipper-ARF reference: `Flipper-ARF/lib/subghz/protocols/power_smart.c`

use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::duration_diff;
use crate::protocols::common::{common_manchester_advance, CommonManchesterState};
use crate::radio::demodulator::LevelDuration;

const TE_SHORT: u32 = 225;
const TE_LONG: u32 = 450;
const TE_DELTA: u32 = 100;

const MIN_COUNT_BIT: usize = 64;

const POWER_SMART_PACKET_HEADER: u64 = 0xFD000000AA000000;
const POWER_SMART_PACKET_HEADER_MASK: u64 = 0xFF000000FF000000;

pub struct PowerSmartDecoder {
    manchester_saved_state: CommonManchesterState,
    decode_data: u64,
    decode_count_bit: usize,
}

impl PowerSmartDecoder {
    pub fn new() -> Self {
        Self {
            manchester_saved_state: CommonManchesterState::Mid1, // Matches typical Flipper Manchester initialization (usually Mid1 or Start1, reset will re-init)
            decode_data: 0,
            decode_count_bit: 0,
        }
    }

    fn check_valid(packet: u64) -> bool {
        let data_1 = ((packet >> 40) & 0xFFFF) as u32;
        let data_2 = (((!packet) >> 8) & 0xFFFF) as u32;
        let data_3 = ((packet >> 32) & 0xFF) as u8;
        let data_4 = (((!packet) & 0xFF) as u8).wrapping_sub(1);

        data_1 == data_2 && data_3 == data_4
    }
}

impl ProtocolDecoder for PowerSmartDecoder {
    fn name(&self) -> &'static str {
        "PowerSmart"
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
        self.manchester_saved_state = CommonManchesterState::Mid1;
        self.decode_data = 0;
        self.decode_count_bit = 0;
    }

    fn feed(&mut self, level: bool, duration: u32) -> Option<DecodedSignal> {
        let mut event = None;
        if !level {
            if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                event = Some(0); // ShortLow
            } else if duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                event = Some(2); // LongLow
            }
        } else {
            if duration_diff!(duration, TE_SHORT) < TE_DELTA {
                event = Some(1); // ShortHigh
            } else if duration_diff!(duration, TE_LONG) < TE_DELTA * 2 {
                event = Some(3); // LongHigh
            }
        }

        if let Some(ev) = event {
            let (new_state, bit_opt) = common_manchester_advance(self.manchester_saved_state, ev);
            self.manchester_saved_state = new_state;

            if let Some(bit) = bit_opt {
                self.decode_data = (self.decode_data << 1) | if bit { 0 } else { 1 };
                self.decode_count_bit += 1;
            }

            if (self.decode_data & POWER_SMART_PACKET_HEADER_MASK) == POWER_SMART_PACKET_HEADER
                && Self::check_valid(self.decode_data)
            {
                let packet = self.decode_data;

                let btn = (((packet >> 54) & 0x02) | ((packet >> 40) & 0x1)) as u8;
                let serial = (((packet >> 33) & 0x3FFF00) | ((packet >> 32) & 0xFF)) as u32;
                let cnt = ((packet >> 49) & 0x3F) as u16;

                let res = DecodedSignal {
                    serial: Some(serial),
                    button: Some(btn),
                    counter: Some(cnt),
                    crc_valid: true,
                    data: packet,
                    data_count_bit: MIN_COUNT_BIT,
                    encoder_capable: true,
                    extra: None,
                    protocol_display_name: None,
                };

                self.decode_data = 0;
                self.decode_count_bit = 0;
                return Some(res);
            }
        } else {
            self.decode_data = 0;
            self.decode_count_bit = 0;
            self.manchester_saved_state = CommonManchesterState::Mid1;
        }

        None
    }

    fn supports_encoding(&self) -> bool {
        true
    }

    fn encode(&self, decoded: &DecodedSignal, button: u8) -> Option<Vec<LevelDuration>> {
        let mut upload = Vec::new();
        let mut data = decoded.data;

        // Apply new button if needed (preserving the rest of the layout)
        if button != decoded.button.unwrap_or(0) {
            let b0 = button & 0x1;
            let b1 = (button >> 1) & 0x1;

            data &= !(1 << 40);
            data |= (b0 as u64) << 40;

            data &= !(1 << 55);
            data |= (b1 as u64) << 55;

            // Recompute checks
            let data_1 = ((data >> 40) & 0xFFFF) as u16;
            let data_2_val = (!data_1) as u64;

            data &= !(0xFFFF << 8);
            data |= (data_2_val & 0xFFFF) << 8;

            let data_3 = ((data >> 32) & 0xFF) as u8;
            let data_4_val = (!data_3).wrapping_add(1) as u64;

            data &= !(0xFF);
            data |= data_4_val & 0xFF;
        }

        // We need to implement a simple Manchester encoder:
        // Bit 0 = 01 (ShortHigh, ShortLow) or LongHigh if previous was 1
        // Bit 1 = 10 (ShortLow, ShortHigh) or LongLow if previous was 0
        // We will just do a generic approach:

        let mut last_level = false; // Start with LOW

        for i in (0..MIN_COUNT_BIT).rev() {
            let bit = ((data >> i) & 1) == 0; // Flipper code uses !data in decoder

            if bit {
                // Encode 1 (01) -> LOW then HIGH
                if !last_level {
                    upload.push(LevelDuration::new(false, TE_SHORT));
                    upload.push(LevelDuration::new(true, TE_SHORT));
                } else {
                    if let Some(last) = upload.last_mut() {
                        last.duration_us = TE_LONG;
                    }
                    upload.push(LevelDuration::new(true, TE_SHORT));
                }
                last_level = true;
            } else {
                // Encode 0 (10) -> HIGH then LOW
                if last_level {
                    upload.push(LevelDuration::new(true, TE_SHORT));
                    upload.push(LevelDuration::new(false, TE_SHORT));
                } else {
                    if upload.is_empty() {
                        upload.push(LevelDuration::new(true, TE_SHORT));
                        upload.push(LevelDuration::new(false, TE_SHORT));
                    } else {
                        if let Some(last) = upload.last_mut() {
                            last.duration_us = TE_LONG;
                        }
                        upload.push(LevelDuration::new(false, TE_SHORT));
                    }
                }
                last_level = false;
            }
        }

        // Ensure final level is complete
        if last_level {
            if let Some(last) = upload.last_mut() {
                last.duration_us = TE_LONG;
            }
        }
        upload.push(LevelDuration::new(false, TE_LONG * 1111));

        Some(upload)
    }
}

impl Default for PowerSmartDecoder {
    fn default() -> Self {
        Self::new()
    }
}
