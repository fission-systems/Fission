/// Sentinel used to tag branch targets that reference pcode-internal relative labels.
/// Convention follows Ghidra: `-(label_num + 1)` as i64, stored as u64.
/// Any branch target constant with value > RELATIVE_LABEL_SENTINEL_THRESHOLD is a sentinel.
const RELATIVE_LABEL_SENTINEL_THRESHOLD: u64 = u64::MAX - 0x10000;

fn encode_relative_sentinel(label_num: u64) -> u64 {
    (-(label_num as i64 + 1)) as u64
}

fn decode_relative_sentinel(sentinel: u64) -> Option<u64> {
    if sentinel > RELATIVE_LABEL_SENTINEL_THRESHOLD {
        let label_num = (-(sentinel as i64) - 1) as u64;
        Some(label_num)
    } else {
        None
    }
}
