use super::{DecodedSignal, ProtocolDecoder, ProtocolTiming};
use crate::radio::demodulator::LevelDuration;

// BinRAW protocol is used for capturing/encoding raw signals that have been
// dynamically classified (binarized). The implementation in Flipper-ARF does
// memory-heavy processing of an entire capture buffer to classify gap, gaps,
// rolling components, and find TE dynamically based on heuristics.
// Since KAT framework uses real-time pulse feeding, replicating full BinRAW
// classification logic is quite complex and memory intensive. For now, we will
// implement a basic skeleton matching the interface that ignores the feed or
// fails to decode (since Flipper-ARF relies on RSSI and capturing 2048 buffers
// before classification).

const TE_SHORT: u32 = 30;
const TE_LONG: u32 = 65000;
const TE_DELTA: u32 = 0;
const MIN_COUNT_BIT: usize = 0;

pub struct BinRawDecoder {
    // Basic fields since full buffer analysis is out of scope for real-time `feed`
}

impl BinRawDecoder {
    pub fn new() -> Self {
        Self {}
    }
}

impl ProtocolDecoder for BinRawDecoder {
    fn name(&self) -> &'static str {
        "BinRAW"
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
        &[433_920_000, 315_000_000, 868_350_000]
    }

    fn reset(&mut self) {
    }

    fn feed(&mut self, _level: bool, _duration: u32) -> Option<DecodedSignal> {
        // Real BinRAW decode depends on RSSI thresholding and filling a 2048 pulse buffer,
        // then analyzing the whole buffer using statistical clustering. This doesn't map
        // well to the real-time event-driven `feed` loop in KAT which doesn't provide RSSI
        // per pulse. We will just return None for real-time decoding for now.
        None
    }

    fn supports_encoding(&self) -> bool {
        false
    }

    fn encode(&self, _decoded: &DecodedSignal, _button: u8) -> Option<Vec<LevelDuration>> {
        None
    }
}

impl Default for BinRawDecoder {
    fn default() -> Self {
        Self::new()
    }
}
