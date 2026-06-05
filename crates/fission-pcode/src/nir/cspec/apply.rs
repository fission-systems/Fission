//! Apply Ghidra `.cspec` data from `utils/sleigh-specs/languages` onto [`NirRenderOptions`].
//!
//! Production binaries get `(language_id, compiler_spec_id)` from the loader and a register
//! map from SLEIGH; unit tests supply a minimal [`SlaRegisterMap`] for the target arch.

use super::loader::load_cspec_for_pair;
use super::register_model::{apply_register_model_for_options, register_model_for_language};
use super::{ResolvedPrototype, SlaRegisterMap};
use crate::nir::{CallingConvention, NirRenderOptions};
use std::collections::HashMap;

/// Root of the checked-in Ghidra language tree (`utils/sleigh-specs/languages`).
///
/// Honors the `FISSION_SLEIGH_SPEC_DIR` override so this crate resolves to the same tree as
/// `fission-sleigh` / `fission-core`. The override may point at either the specs root (which
/// contains `languages/`) or the `languages/` directory itself.
pub fn sleigh_languages_root() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Some(path) = std::env::var_os("FISSION_SLEIGH_SPEC_DIR") {
        let path = PathBuf::from(path);
        if path.file_name().and_then(|name| name.to_str()) == Some("languages") {
            return path;
        }
        return path.join("languages");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("utils/sleigh-specs/languages")
}

/// Default `(language_id, compiler_spec_id)` for preview/tests when no binary load spec exists.
pub fn default_cspec_pair(options: &NirRenderOptions) -> Option<(String, String)> {
    let fmt = options.format.to_ascii_uppercase();
    let pair = match options.calling_convention {
        CallingConvention::WindowsX64 => ("x86:LE:64:default", "windows"),
        CallingConvention::SystemVAmd64 => ("x86:LE:64:default", "gcc"),
        CallingConvention::X86_32 => ("x86:LE:32:default", if fmt.contains("PE") { "windows" } else { "gcc" }),
        CallingConvention::AArch64 => {
            if options.is_big_endian {
                ("AARCH64:BE:64:v8A", "default")
            } else {
                ("AARCH64:LE:64:v8A", "default")
            }
        }
        CallingConvention::Arm32 => ("ARM:LE:32:v7", "default"),
        CallingConvention::PowerPc32 => ("PowerPC:BE:32:default", "default"),
        CallingConvention::PowerPc64 => ("PowerPC:BE:64:default", "default"),
        CallingConvention::LoongArch32 => ("Loongarch:LE:32:ilp32d", "default"),
        CallingConvention::LoongArch64 => ("Loongarch:LE:64:lp64d", "default"),
        CallingConvention::Mips32 => {
            if options.is_big_endian {
                ("MIPS:BE:32:default", "default")
            } else {
                ("MIPS:LE:32:default", "default")
            }
        }
        CallingConvention::Mips64 => {
            if options.is_big_endian {
                ("MIPS:BE:64:default", "default")
            } else {
                ("MIPS:LE:64:default", "default")
            }
        }
    };
    Some((pair.0.to_string(), pair.1.to_string()))
}

pub fn apply_resolved_proto_to_options(options: &mut NirRenderOptions, proto: &ResolvedPrototype) {
    if !proto.int_param_offsets.is_empty() {
        options.cspec_param_offsets = Some(proto.int_param_offsets.clone());
    }
    if let Some(base) = proto.stack_arg_base {
        options.cspec_stack_arg_base = Some(base);
    }
    options.cspec_extrapop = Some(proto.extrapop);
    options.cspec_return_offset = proto.return_offset;
}

/// Load `.cspec` for `options.calling_convention` and populate cspec override fields.
///
/// Returns `true` when a prototype was resolved and applied.
pub fn apply_cspec_for_options(options: &mut NirRenderOptions, reg_map: &SlaRegisterMap) -> bool {
    let Some((language_id, compiler_spec_id)) = default_cspec_pair(options) else {
        return false;
    };
    apply_cspec_for_pair(options, &language_id, &compiler_spec_id, reg_map)
}

/// Load `.cspec` for an explicit Ghidra pair and populate cspec override fields.
pub fn apply_cspec_for_pair(
    options: &mut NirRenderOptions,
    language_id: &str,
    compiler_spec_id: &str,
    reg_map: &SlaRegisterMap,
) -> bool {
    let languages_root = sleigh_languages_root();
    if !languages_root.is_dir() {
        return false;
    }
    let Some(resolved) = load_cspec_for_pair(&languages_root, language_id, compiler_spec_id, reg_map)
    else {
        return false;
    };
    let Some(proto) = resolved.default_proto.as_ref() else {
        return false;
    };
    apply_resolved_proto_to_options(options, proto);
    true
}

impl NirRenderOptions {
    /// Integer parameter register offsets from `.cspec` (empty when not loaded).
    pub(in crate::nir) fn int_param_offsets(&self) -> &[u64] {
        self.cspec_param_offsets.as_deref().unwrap_or(&[])
    }

    /// Ensure cspec fields are populated when missing.
    pub(in crate::nir) fn ensure_cspec(&mut self, reg_map: &SlaRegisterMap) {
        self.ensure_sla_register_map();
        if self.cspec_param_offsets.is_some() {
            return;
        }
        let map = self
            .sla_register_map
            .as_ref()
            .map(|offset_map| sla_register_map_from_offset_map(offset_map))
            .unwrap_or_else(|| reg_map.clone());
        let _ = apply_cspec_for_options(self, &map);
    }

    /// Populate `sla_register_map` from the checked-in SLA register model when absent.
    pub(in crate::nir) fn ensure_sla_register_map(&mut self) {
        if self.sla_register_map.is_none() {
            let _ = apply_register_model_for_options(self);
        }
    }

    /// Inverted `(offset,size)→name` map as name→`(offset,size)` for cspec resolution.
    pub(in crate::nir) fn sla_name_index(&self) -> Option<SlaRegisterMap> {
        self.sla_register_map
            .as_ref()
            .map(sla_register_map_from_offset_map)
    }

    /// Cached SLA register model for the preview/default language pair.
    pub(in crate::nir) fn register_model(
        &self,
    ) -> Option<std::sync::Arc<super::register_model::RegisterModel>> {
        default_cspec_pair(self).and_then(|(lang, _)| register_model_for_language(&lang))
    }
}

pub(in crate::nir) fn sla_register_map_from_offset_map(map: &HashMap<(u64, u32), String>) -> SlaRegisterMap {
    map.iter()
        .map(|((off, sz), name)| (name.to_ascii_uppercase(), (*off, *sz)))
        .collect()
}
