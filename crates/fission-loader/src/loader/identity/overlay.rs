//! Detect appended bytes after the mapped file image (overlay-style tail).

use crate::loader::LoadedBinary;

use super::model::OverlayInfo;

#[must_use]
pub fn detect_overlay(binary: &LoadedBinary) -> Option<OverlayInfo> {
    let data = binary.data.as_slice();
    let total = data.len() as u64;
    let mut max_end = 0_u64;
    for sec in &binary.sections {
        let end = sec.file_offset.saturating_add(sec.file_size);
        max_end = max_end.max(end);
    }
    const MIN_OVERLAY_BYTES: u64 = 16;
    if total > max_end && total.saturating_sub(max_end) >= MIN_OVERLAY_BYTES {
        Some(OverlayInfo {
            file_offset_start: max_end,
            size: total.saturating_sub(max_end),
        })
    } else {
        None
    }
}
