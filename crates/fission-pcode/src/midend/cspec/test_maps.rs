//! Test helpers: populate SLA register map and cspec from checked-in specs.

use super::apply::{apply_cspec_for_options, default_cspec_pair, sla_register_map_from_offset_map};
use super::register_model::{apply_register_model_for_options, register_model_for_language};
use crate::midend::NirRenderOptions;

/// Re-apply SLA register map and cspec after mutating `calling_convention`, endianness, or format.
pub(crate) fn sync_preview_cspec(options: &mut NirRenderOptions) {
    apply_preview_cspec(options);
}

pub(crate) fn apply_preview_cspec(options: &mut NirRenderOptions) {
    let _ = apply_register_model_for_options(options);
    let Some((lang, _)) = default_cspec_pair(options) else {
        return;
    };
    let Some(model) = register_model_for_language(&lang) else {
        return;
    };
    let reg_map = options
        .sla_register_map
        .as_ref()
        .map(sla_register_map_from_offset_map)
        .unwrap_or_else(|| model.to_sla_register_map());
    let _ = apply_cspec_for_options(options, &reg_map);
    options.cspec_return_target = model.return_target_offset();
    if options.cspec_return_offset.is_none() {
        if let Some(offset) = model.return_offset() {
            options.cspec_return_offset = Some(offset);
        }
    }
}
