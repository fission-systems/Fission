//! SLA-derived register naming model from checked-in `.slaspec` sources.
//!
//! Ghidra-style "names from SLA, slots from cspec" lookup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use super::SlaRegisterMap;
use super::apply::sleigh_languages_root;
use super::ldefs::global_language_slaspec_index;
use super::slaspec_parse::{ParsedRegister, parse_registers_from_slaspec};
use crate::arch::x86::{X86_REG_BASE, unique_x86_register_name, x86_register_space_flag_name};
use crate::midend::{
    REGISTER_SPACE_ID, RUST_SLEIGH_ALT_REGISTER_SPACE_ID, RUST_SLEIGH_REGISTER_SPACE_ID,
    UNIQUE_SPACE_ID, Varnode, is_register_space_id,
};
use fission_core::CallingConvention;

static REGISTER_MODEL_CACHE: OnceLock<RwLock<HashMap<String, Arc<RegisterModel>>>> =
    OnceLock::new();

fn model_cache() -> &'static RwLock<HashMap<String, Arc<RegisterModel>>> {
    REGISTER_MODEL_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// SLA register table for one Ghidra `language_id`.
#[derive(Debug, Clone)]
pub struct RegisterModel {
    /// `(offset, size)` → canonical lowercase hardware name.
    by_offset: HashMap<(u64, u32), String>,
    /// Uppercase SLA name → `(offset, size)`.
    name_index: SlaRegisterMap,
    /// Lowercase name → GPR family index (same base offset + slot within define register).
    family_by_name: HashMap<String, usize>,
    /// Family index → representative `(offset, size)` at the widest observed size.
    family_repr: HashMap<usize, (u64, u32)>,
    /// Primary return register offset from the default cspec output (when known).
    return_offset: Option<u64>,
    /// Return target (link register) offset when known from pspec/SLA name.
    return_target_offset: Option<(u64, u32)>,
}

impl RegisterModel {
    pub fn name_for(&self, offset: u64, size: u32) -> Option<&str> {
        self.exact_name_for(offset, size).or_else(|| {
            // Wider covering register (e.g. RAX covers EAX at same offset).
            self.by_offset
                .iter()
                .find(|((off, sz), _)| *off == offset && *sz >= size)
                .map(|(_, name)| name.as_str())
        })
    }

    pub fn exact_name_for(&self, offset: u64, size: u32) -> Option<&str> {
        self.by_offset.get(&(offset, size)).map(String::as_str)
    }

    pub fn family_index(&self, name: &str) -> Option<usize> {
        self.family_by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub fn family_repr(&self, family: usize) -> Option<(u64, u32)> {
        self.family_repr.get(&family).copied()
    }

    pub fn return_offset(&self) -> Option<u64> {
        self.return_offset
    }

    pub fn return_target_offset(&self) -> Option<(u64, u32)> {
        self.return_target_offset
    }

    pub fn to_sla_register_map(&self) -> SlaRegisterMap {
        self.name_index.clone()
    }

    pub fn to_offset_map(&self) -> HashMap<(u64, u32), String> {
        self.by_offset.clone()
    }

    pub fn lookup_name(&self, name: &str) -> Option<(u64, u32)> {
        self.name_index
            .get(name)
            .or_else(|| self.name_index.get(&name.to_ascii_uppercase()))
            .or_else(|| self.name_index.get(&name.to_ascii_lowercase()))
            .copied()
    }

    pub fn build_from_parsed(parsed: &[ParsedRegister]) -> Self {
        let mut by_offset = HashMap::new();
        let mut name_index = SlaRegisterMap::new();
        let mut family_keys: HashMap<u64, usize> = HashMap::new();
        let mut family_by_name = HashMap::new();
        let mut family_repr: HashMap<usize, (u64, u32)> = HashMap::new();
        let mut next_family = 0usize;

        for reg in parsed {
            let lower = reg.name.clone();
            let upper = reg.name.to_ascii_uppercase();
            by_offset.insert((reg.offset, reg.size), lower.clone());
            name_index.insert(upper, (reg.offset, reg.size));

            let family_key = register_family_key(reg);
            let family_id = *family_keys.entry(family_key).or_insert_with(|| {
                let id = next_family;
                next_family += 1;
                id
            });
            family_by_name.insert(lower.clone(), family_id);
            family_repr
                .entry(family_id)
                .and_modify(|repr| {
                    if reg.size > repr.1 {
                        *repr = (reg.offset, reg.size);
                    }
                })
                .or_insert((reg.offset, reg.size));
        }

        let return_target_offset = detect_return_target(&by_offset);
        add_register_aliases(&mut name_index, &by_offset);

        Self {
            by_offset,
            name_index,
            family_by_name,
            family_repr,
            return_offset: None,
            return_target_offset,
        }
    }

    pub fn with_return_offset(mut self, offset: Option<u64>) -> Self {
        self.return_offset = offset;
        self
    }
}

fn register_family_key(reg: &ParsedRegister) -> u64 {
    // AArch64 GPR bank: X/W aliases share one 8-byte slot; W entries are
    // interleaved with `_` placeholders and BE places W in the high half.
    if (0x4000..0x4100).contains(&reg.base_offset) {
        let slot_stride = if reg.size == 4 { 2u64 } else { 1u64 };
        let gpr_index = reg.slot_index as u64 / slot_stride;
        return reg.base_offset.saturating_add(gpr_index.saturating_mul(8));
    }
    reg.offset
}

fn detect_return_target(by_offset: &HashMap<(u64, u32), String>) -> Option<(u64, u32)> {
    by_offset
        .iter()
        .find(|(_, name)| matches!(name.as_str(), "lr" | "x30" | "ra" | "r31" | "blink"))
        .map(|((off, sz), _)| (*off, *sz))
}

/// Ghidra/cspec register synonyms that share the same `(offset, size)`.
fn add_register_aliases(name_index: &mut SlaRegisterMap, by_offset: &HashMap<(u64, u32), String>) {
    const SYNONYMS: &[(&str, &str)] = &[("fp", "s8"), ("s8", "fp"), ("zero", "r0"), ("r0", "zero")];
    for ((off, sz), canonical) in by_offset {
        let upper = canonical.to_ascii_uppercase();
        name_index.entry(upper).or_insert((*off, *sz));
        for (left, right) in SYNONYMS {
            if canonical.eq_ignore_ascii_case(left) {
                name_index.insert(right.to_ascii_uppercase(), (*off, *sz));
            }
        }
    }
}

/// Load (or return cached) [`RegisterModel`] for `language_id`.
pub fn register_model_for_language(language_id: &str) -> Option<Arc<RegisterModel>> {
    if let Some(cached) = model_cache().read().ok()?.get(language_id).cloned() {
        return Some(cached);
    }
    let model = build_register_model_for_language(language_id)?;
    if let Ok(mut cache) = model_cache().write() {
        cache.insert(language_id.to_string(), model.clone());
    }
    Some(model)
}

pub fn build_register_model_for_language(language_id: &str) -> Option<Arc<RegisterModel>> {
    let languages_root = sleigh_languages_root();
    let slaspec = global_language_slaspec_index(&languages_root)
        .get(language_id)?
        .clone();
    let parsed = parse_registers_from_slaspec(&slaspec).ok()?;
    Some(Arc::new(RegisterModel::build_from_parsed(&parsed)))
}

/// Cached [`RegisterModel`] for a [`CallingConvention`] preview/default language pair.
pub fn register_model_for_abi(abi: CallingConvention) -> Option<Arc<RegisterModel>> {
    let options = minimal_options_for_abi(abi);
    super::apply::default_cspec_pair(&options)
        .and_then(|(lang, _)| register_model_for_language(&lang))
}

fn minimal_options_for_abi(abi: CallingConvention) -> crate::midend::NirRenderOptions {
    let (is_64bit, pointer_size) = match abi {
        CallingConvention::WindowsX64
        | CallingConvention::SystemVAmd64
        | CallingConvention::AArch64
        | CallingConvention::PowerPc64
        | CallingConvention::Mips64
        | CallingConvention::LoongArch64 => (true, 8),
        _ => (false, 4),
    };
    crate::midend::NirRenderOptions {
        calling_convention: abi,
        is_64bit,
        pointer_size,
        format: "PE".to_string(),
        ..Default::default()
    }
}

/// Build a [`RegisterNamer`] from ABI defaults (no cspec param offsets unless supplied).
pub fn register_namer_for_abi(abi: CallingConvention) -> RegisterNamer {
    let model = register_model_for_abi(abi);
    RegisterNamer {
        abi,
        sla_map: model.as_ref().map(|m| m.to_offset_map()),
        int_param_offsets: Vec::new(),
        return_offset: model.as_ref().and_then(|m| m.return_offset()),
        return_target: model.as_ref().and_then(|m| m.return_target_offset()),
        model,
        pointer_size: minimal_options_for_abi(abi).pointer_size,
    }
}

pub fn register_namer_from_options(options: &crate::midend::NirRenderOptions) -> RegisterNamer {
    RegisterNamer::from_options(options)
}

/// Populate `options.sla_register_map` from the SLA register model for `language_id`.
pub fn apply_register_model_for_language(
    options: &mut crate::midend::NirRenderOptions,
    language_id: &str,
) -> bool {
    let Some(model) = register_model_for_language(language_id) else {
        return false;
    };
    options.sla_register_map = Some(model.to_offset_map());
    true
}

/// Populate `options.sla_register_map` using the default preview/test language pair.
pub fn apply_register_model_for_options(options: &mut crate::midend::NirRenderOptions) -> bool {
    let Some((language_id, _)) = super::apply::default_cspec_pair(options) else {
        return false;
    };
    apply_register_model_for_language(options, &language_id)
}

/// Space-aware SLA-first register naming helper.
#[derive(Debug, Clone)]
pub struct RegisterNamer {
    pub abi: CallingConvention,
    pub sla_map: Option<HashMap<(u64, u32), String>>,
    pub int_param_offsets: Vec<u64>,
    pub return_offset: Option<u64>,
    pub return_target: Option<(u64, u32)>,
    pub model: Option<Arc<RegisterModel>>,
    pub pointer_size: u32,
}

impl RegisterNamer {
    pub fn from_options(options: &crate::midend::NirRenderOptions) -> Self {
        let model = super::apply::default_cspec_pair(options)
            .and_then(|(lang, _)| register_model_for_language(&lang));
        Self {
            abi: options.calling_convention,
            sla_map: options.sla_register_map.clone(),
            int_param_offsets: super::apply::int_param_offsets(options).to_vec(),
            return_offset: options.cspec_return_offset,
            return_target: model
                .as_ref()
                .and_then(|m| m.return_target_offset())
                .or(options.cspec_return_target),
            model,
            pointer_size: options.pointer_size,
        }
    }

    pub fn hw_name(&self, vn: &Varnode) -> Option<String> {
        if vn.space_id == UNIQUE_SPACE_ID {
            if let Some(name) = unique_x86_register_name(vn.offset, vn.size) {
                return Some(name.to_string());
            }
            if vn.offset >= X86_REG_BASE {
                let native = vn.offset - X86_REG_BASE;
                return self.hw_name_at(native, vn.size).or_else(|| {
                    self.model
                        .as_ref()
                        .and_then(|m| m.name_for(native, vn.size).map(str::to_string))
                });
            }
            return None;
        }
        if !is_register_space_id(vn.space_id) {
            return None;
        }
        self.hw_name_at(vn.offset, vn.size)
    }

    pub fn hw_name_at(&self, offset: u64, size: u32) -> Option<String> {
        if self.abi == CallingConvention::AArch64 && offset == 0x00 {
            return Some(if size == 4 {
                "w0".to_string()
            } else {
                "x0".to_string()
            });
        }
        if matches!(
            self.abi,
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
        ) && size == 1
            && is_x64_canonical_gpr_slot(offset)
        {
            return self.hw_name_at(offset, 8);
        }
        // SLA EFLAGS bits at 0x200.. (CF/ZF/SF/OF/…). Must be distinct names —
        // never collapse to the shared `"reg"` fallback (destroys Jcc recovery).
        if matches!(
            self.abi,
            CallingConvention::WindowsX64
                | CallingConvention::SystemVAmd64
                | CallingConvention::X86_32
        ) && size == 1
            && (0x200..0x280).contains(&offset)
        {
            if let Some(name) = x86_register_space_flag_name(offset, size) {
                return Some(name.to_string());
            }
            // Reserved flag slots (F1/F3/…) stay unnamed rather than sharing `"reg"`.
            return None;
        }
        if self.abi == CallingConvention::PowerPc64 && size == 4 {
            let slot_base = offset & !0x7;
            if offset
                .checked_add(u64::from(size))
                .is_some_and(|end| end <= slot_base.saturating_add(8))
            {
                return self.hw_name_at(slot_base, 8);
            }
        }
        if matches!(
            self.abi,
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
        ) && self.pointer_size == 8
            && size == 4
            && is_x64_canonical_gpr_slot(offset)
            && let Some(name) = self.x64_canonical_hw_name(offset, 8)
        {
            return Some(name);
        }
        let prefer_size = self.preferred_lookup_size(offset, size);
        if let Some(map) = self.sla_map.as_ref() {
            if let Some(name) = map.get(&(offset, prefer_size)) {
                return Some(name.to_ascii_lowercase());
            }
            if prefer_size != size {
                if let Some(name) = map.get(&(offset, size)) {
                    return Some(name.to_ascii_lowercase());
                }
            }
            if let Some((_, name)) = map
                .iter()
                .find(|((off, sz), _)| *off == offset && *sz >= prefer_size)
            {
                return Some(name.to_ascii_lowercase());
            }
        }
        self.model
            .as_ref()
            .and_then(|m| m.exact_name_for(offset, size).map(str::to_string))
            .or_else(|| {
                if matches!(
                    self.abi,
                    CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                ) && size == 4
                    && self.pointer_size == 8
                    && is_x64_canonical_gpr_slot(offset)
                {
                    if let Some(map) = self.sla_map.as_ref() {
                        if let Some(name) = map.get(&(offset, 8)) {
                            return Some(name.to_ascii_lowercase());
                        }
                    }
                    return self
                        .model
                        .as_ref()
                        .and_then(|m| m.name_for(offset, 8).map(str::to_string));
                }
                None
            })
            .or_else(|| {
                self.model.as_ref().and_then(|m| {
                    m.name_for(offset, prefer_size)
                        .or_else(|| m.name_for(offset, size))
                        .map(str::to_string)
                })
            })
            .or_else(|| x86_ia32_low_gpr_name(self.abi, offset, size))
    }

    fn x64_canonical_hw_name(&self, offset: u64, size: u32) -> Option<String> {
        if let Some(map) = self.sla_map.as_ref() {
            if let Some(name) = map.get(&(offset, size)) {
                return Some(name.to_ascii_lowercase());
            }
        }
        self.model
            .as_ref()
            .and_then(|m| m.name_for(offset, size).map(str::to_string))
    }

    fn preferred_lookup_size(&self, offset: u64, size: u32) -> u32 {
        match self.abi {
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                if size == 4 {
                    return 4;
                }
                if self
                    .sla_map
                    .as_ref()
                    .is_some_and(|map| map.contains_key(&(offset, 8)))
                    || self
                        .model
                        .as_ref()
                        .is_some_and(|m| m.exact_name_for(offset, 8).is_some())
                {
                    8
                } else {
                    size
                }
            }
            _ => size,
        }
    }

    pub fn register_name_with_param_owned(
        &self,
        offset: u64,
        size: u32,
    ) -> Option<(String, Option<usize>)> {
        let hw_name = self.hw_name_at(offset, size)?;
        let param_idx = match self.abi {
            CallingConvention::WindowsX64
            | CallingConvention::SystemVAmd64
            | CallingConvention::X86_32 => self
                .int_param_offsets
                .iter()
                .position(|&param_offset| param_offset == offset),
            _ => {
                let name_family = self.model.as_ref().and_then(|m| m.family_index(&hw_name))?;
                self.int_param_offsets.iter().position(|&param_offset| {
                    self.model.as_ref().and_then(|m| {
                        m.name_for(param_offset, self.param_slot_size())
                            .and_then(|n| m.family_index(n))
                    }) == Some(name_family)
                })
            }
        };
        match param_idx {
            Some(idx) => Some((format!("param_{}", idx + 1), Some(idx))),
            None => Some((hw_name, None)),
        }
    }

    pub(crate) fn param_slot_size(&self) -> u32 {
        match self.abi {
            CallingConvention::AArch64
            | CallingConvention::PowerPc64
            | CallingConvention::Mips64
            | CallingConvention::LoongArch64 => 8,
            CallingConvention::Arm32
            | CallingConvention::PowerPc32
            | CallingConvention::Mips32
            | CallingConvention::LoongArch32 => 4,
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => 8,
            CallingConvention::X86_32 => 4,
        }
    }

    pub fn is_primary_return_register(&self, vn: &Varnode) -> bool {
        if vn.space_id == UNIQUE_SPACE_ID {
            return unique_x86_register_name(vn.offset, vn.size)
                .is_some_and(|n| n == "rax" || n == "eax");
        }
        if !is_register_space_id(vn.space_id) {
            return false;
        }
        if let Some(ret_off) = self.return_offset {
            if vn.offset == ret_off {
                return true;
            }
            if self.abi != CallingConvention::AArch64 {
                return false;
            }
        }
        match self.abi {
            CallingConvention::AArch64 => {
                if vn.offset == 0 {
                    return false;
                }
                self.model
                    .as_ref()
                    .and_then(|m| m.family_index("x0"))
                    .is_some_and(|fam| {
                        self.hw_name_at(vn.offset, vn.size)
                            .and_then(|n| self.model.as_ref().and_then(|m| m.family_index(&n)))
                            == Some(fam)
                    })
            }
            CallingConvention::Arm32 => vn.offset == 0x20,
            CallingConvention::PowerPc32 => vn.offset == 0x0c,
            CallingConvention::PowerPc64 => vn.offset == 0x18,
            CallingConvention::LoongArch32 => vn.offset == 0x110,
            CallingConvention::LoongArch64 => vn.offset == 0x120,
            CallingConvention::Mips32 => vn.offset == 0x08,
            CallingConvention::Mips64 => vn.offset == 0x10,
            CallingConvention::WindowsX64
            | CallingConvention::SystemVAmd64
            | CallingConvention::X86_32 => vn.offset == 0x00,
        }
    }

    pub fn is_return_target_register(&self, vn: &Varnode) -> bool {
        if !is_register_space_id(vn.space_id) {
            return false;
        }
        if let Some((off, sz)) = self.return_target {
            return vn.offset == off && vn.size == sz;
        }
        match self.abi {
            CallingConvention::AArch64 => self
                .model
                .as_ref()
                .and_then(|m| m.family_index("x30"))
                .is_some_and(|fam| {
                    self.hw_name_at(vn.offset, vn.size)
                        .and_then(|n| self.model.as_ref().and_then(|m| m.family_index(&n)))
                        == Some(fam)
                }),
            CallingConvention::Arm32 => vn.offset == 0x58,
            CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
                self.hw_name_at(vn.offset, vn.size).as_deref() == Some("lr")
            }
            CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
                self.hw_name_at(vn.offset, vn.size).as_deref() == Some("ra")
            }
            CallingConvention::Mips32 | CallingConvention::Mips64 => {
                self.hw_name_at(vn.offset, vn.size).as_deref() == Some("ra")
            }
            CallingConvention::WindowsX64
            | CallingConvention::SystemVAmd64
            | CallingConvention::X86_32 => false,
        }
    }

    pub fn primary_return_registers(&self) -> Vec<Varnode> {
        let pointer_size = self.pointer_size;
        let offset = self
            .return_offset
            .unwrap_or_else(|| default_return_offset(self.abi));
        let mut out = vec![Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size: pointer_size,
            is_constant: false,
            constant_val: 0,
        }];
        if matches!(
            self.abi,
            CallingConvention::WindowsX64
                | CallingConvention::SystemVAmd64
                | CallingConvention::X86_32
        ) {
            out.push(Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: X86_REG_BASE,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            });
        } else {
            out.push(Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            });
        }
        if matches!(
            self.abi,
            CallingConvention::LoongArch32 | CallingConvention::LoongArch64
        ) {
            out.push(Varnode {
                space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
                offset,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            });
        }
        out
    }

    pub fn gpr_family_index_at(&self, offset: u64, size: u32) -> Option<usize> {
        let hw_name = self.hw_name_at(offset, size)?;
        self.model
            .as_ref()
            .and_then(|m| m.family_index(&hw_name))
            .or_else(|| crate::arch::x86::x86_gpr_family_index(hw_name.as_str()))
    }

    pub fn gpr_family_index_for_name(&self, name: &str) -> Option<usize> {
        self.model
            .as_ref()
            .and_then(|m| m.family_index(name))
            .or_else(|| crate::arch::x86::x86_gpr_family_index(name))
    }

    pub fn is_known_gpr(&self, offset: u64, size: u32) -> bool {
        self.gpr_family_index_at(offset, size).is_some()
    }
}

fn is_x64_canonical_gpr_slot(offset: u64) -> bool {
    matches!(
        offset,
        0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38
    ) || ((0x80..=0xb8).contains(&offset) && offset % 8 == 0)
}

fn x86_ia32_low_gpr_name(abi: CallingConvention, offset: u64, size: u32) -> Option<String> {
    if size != 4 {
        return None;
    }
    if !matches!(
        abi,
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 | CallingConvention::X86_32
    ) {
        return None;
    }
    Some(
        match offset {
            0x00 => "eax",
            0x04 => "ecx",
            0x08 => "edx",
            0x0c => "ebx",
            0x10 => "esp",
            0x14 => "ebp",
            0x18 => "esi",
            0x1c => "edi",
            _ => return None,
        }
        .to_string(),
    )
}

fn default_return_offset(abi: CallingConvention) -> u64 {
    match abi {
        CallingConvention::AArch64 => 0x4000,
        CallingConvention::Arm32 => 0x20,
        CallingConvention::PowerPc32 => 0x0c,
        CallingConvention::PowerPc64 => 0x18,
        CallingConvention::LoongArch32 => 0x110,
        CallingConvention::LoongArch64 => 0x120,
        CallingConvention::Mips32 => 0x08,
        CallingConvention::Mips64 => 0x10,
        CallingConvention::WindowsX64
        | CallingConvention::SystemVAmd64
        | CallingConvention::X86_32 => 0x00,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::cspec::{CspecPentry, CspecPrototype};

    fn model_for(lang: &str) -> RegisterModel {
        register_model_for_language(lang)
            .map(|m| (*m).clone())
            .unwrap_or_else(|| panic!("missing register model for {lang}"))
    }

    #[test]
    fn x86_64_register_model_parity() {
        let model = model_for("x86:LE:64:default");
        for (off, expected) in [
            (0x00, "rax"),
            (0x08, "rcx"),
            (0x10, "rdx"),
            (0x18, "rbx"),
            (0x20, "rsp"),
            (0x28, "rbp"),
            (0x30, "rsi"),
            (0x38, "rdi"),
            (0x80, "r8"),
            (0x88, "r9"),
        ] {
            assert_eq!(
                model.name_for(off, 8).map(str::to_ascii_lowercase),
                Some(expected.to_string()),
                "offset 0x{off:x}"
            );
        }
    }

    #[test]
    fn aarch64_register_model_parity() {
        let model = model_for("AARCH64:LE:64:v8A");
        for i in 0..32_u64 {
            let off = 0x4000 + i * 8;
            let x = if i == 31 {
                "xzr".to_string()
            } else {
                format!("x{i}")
            };
            let w = if i == 31 {
                "wzr".to_string()
            } else {
                format!("w{i}")
            };
            assert_eq!(model.name_for(off, 8), Some(x.as_str()), "x register {i}");
            assert_eq!(model.name_for(off, 4), Some(w.as_str()), "w register {i}");
        }
        assert_eq!(model.name_for(0x08, 8), Some("sp"));
    }

    #[test]
    fn aarch64_be_w0_shares_family_with_x0() {
        let model = model_for("AARCH64:BE:64:v8A");
        let w0_family = model.family_index("w0").expect("w0 family");
        let x0_family = model.family_index("x0").expect("x0 family");
        assert_eq!(w0_family, x0_family);
        assert_eq!(model.name_for(0x4004, 4), Some("w0"));
        assert_eq!(model.name_for(0x4000, 8), Some("x0"));
    }

    #[test]
    fn arm32_register_model_parity() {
        let model = model_for("ARM:LE:32:v7");
        for i in 0..13_u64 {
            let off = 0x20 + i * 4;
            let name = format!("r{i}");
            assert_eq!(model.name_for(off, 4), Some(name.as_str()));
        }
        assert_eq!(model.name_for(0x54, 4), Some("sp"));
        assert_eq!(model.name_for(0x58, 4), Some("lr"));
        assert_eq!(model.name_for(0x5c, 4), Some("pc"));
    }

    #[test]
    fn powerpc32_register_model_parity() {
        let model = model_for("PowerPC:BE:32:default");
        for i in 0..32_u64 {
            let off = i * 4;
            let name = format!("r{i}");
            assert_eq!(model.name_for(off, 4), Some(name.as_str()));
        }
        assert_eq!(model.name_for(0x1020, 4), Some("lr"));
    }

    #[test]
    fn mips32_register_model_parity() {
        let model = model_for("MIPS:LE:32:default");
        const GPRS: [&str; 32] = [
            "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5",
            "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1",
            "gp", "sp", "s8", "ra",
        ];
        for i in 0..32_u64 {
            let off = i * 4;
            let expected = GPRS[i as usize];
            assert_eq!(model.name_for(off, 4), Some(expected), "r{i}");
        }
    }

    #[test]
    fn loongarch64_register_model_parity() {
        let model = model_for("Loongarch:LE:64:lp64d");
        for i in 0..32_u64 {
            let off = 0x100 + i * 8;
            assert!(
                model.name_for(off, 8).is_some(),
                "missing register at idx {i} offset 0x{off:x}"
            );
        }
    }

    #[test]
    fn x64_namer_names_register_space_eflags_bits() {
        use crate::midend::cspec::RegisterNamer;
        use crate::midend::{CallingConvention, NirRenderOptions};

        let options = NirRenderOptions {
            calling_convention: CallingConvention::WindowsX64,
            format: "PE".to_string(),
            pe_x64_only: true,
            is_64bit: true,
            pointer_size: 8,
            ..Default::default()
        };
        let namer = RegisterNamer::from_options(&options);
        assert_eq!(namer.hw_name_at(0x200, 1).as_deref(), Some("cf"));
        assert_eq!(namer.hw_name_at(0x206, 1).as_deref(), Some("zf"));
        assert_eq!(namer.hw_name_at(0x207, 1).as_deref(), Some("sf"));
        assert_eq!(namer.hw_name_at(0x20b, 1).as_deref(), Some("of"));
        // Distinct names — never a shared fallback for known flag bits.
        let names: Vec<_> = [0x200u64, 0x206, 0x207, 0x20b]
            .iter()
            .map(|o| namer.hw_name_at(*o, 1).expect("flag name"))
            .collect();
        let unique: std::collections::BTreeSet<_> = names.iter().cloned().collect();
        assert_eq!(unique.len(), 4, "flags must not share a name: {names:?}");
    }

    #[test]
    fn aarch64_be_namer_resolves_w0_at_high_half() {
        use crate::midend::cspec::{RegisterNamer, test_maps::sync_preview_cspec};
        use crate::midend::{CallingConvention, NirRenderOptions};

        let mut options = NirRenderOptions {
            calling_convention: CallingConvention::AArch64,
            format: "ELF64".to_string(),
            pe_x64_only: false,
            is_64bit: true,
            pointer_size: 8,
            is_big_endian: true,
            ..Default::default()
        };
        sync_preview_cspec(&mut options);
        let namer = RegisterNamer::from_options(&options);
        assert_eq!(namer.hw_name_at(0x4004, 4).as_deref(), Some("w0"));
        assert_eq!(
            namer.register_name_with_param_owned(0x4004, 4),
            Some(("param_1".to_string(), Some(0)))
        );
    }

    #[test]
    fn cspec_prototype_registers_resolve_in_register_model() {
        use crate::midend::cspec::apply::{default_cspec_pair, sleigh_languages_root};
        use crate::midend::cspec::loader::{
            cspec_path_for_pair, load_cspec_for_pair, load_cspec_path,
        };
        use crate::midend::cspec::{CspecPentry, CspecPrototype};
        use crate::midend::{CallingConvention, NirRenderOptions};

        for abi in [
            CallingConvention::WindowsX64,
            CallingConvention::SystemVAmd64,
            CallingConvention::X86_32,
            CallingConvention::AArch64,
            CallingConvention::Arm32,
            CallingConvention::PowerPc32,
            CallingConvention::PowerPc64,
            CallingConvention::LoongArch32,
            CallingConvention::LoongArch64,
            CallingConvention::Mips32,
            CallingConvention::Mips64,
        ] {
            let options = NirRenderOptions {
                calling_convention: abi,
                ..Default::default()
            };
            let Some((lang, comp)) = default_cspec_pair(&options) else {
                continue;
            };
            let model = register_model_for_language(&lang)
                .unwrap_or_else(|| panic!("missing model for {lang}"));
            let model_map = model.to_sla_register_map();
            let root = sleigh_languages_root();
            let resolved = load_cspec_for_pair(&root, &lang, &comp, &model_map)
                .unwrap_or_else(|| panic!("cspec resolution failed for {lang}/{comp}"));
            let proto = resolved.default_proto.as_ref().expect("default proto");
            assert!(
                !proto.int_param_offsets.is_empty() || abi == CallingConvention::X86_32,
                "expected int params for {abi:?} ({lang}/{comp})"
            );
            if matches!(
                abi,
                CallingConvention::WindowsX64
                    | CallingConvention::SystemVAmd64
                    | CallingConvention::AArch64
                    | CallingConvention::Arm32
                    | CallingConvention::Mips32
                    | CallingConvention::Mips64
                    | CallingConvention::LoongArch64
            ) {
                assert!(
                    proto.return_offset.is_some(),
                    "expected return offset for {abi:?} ({lang}/{comp})"
                );
            }

            let path = cspec_path_for_pair(&root, &lang, &comp).expect("cspec path");
            let doc = load_cspec_path(&path).expect("cspec doc");
            let raw = doc.default_proto.as_ref().expect("raw default proto");
            let mut missing = Vec::new();
            if let Some(sp) = doc.stackpointer.as_ref() {
                if model.lookup_name(sp).is_none() {
                    missing.push(sp.clone());
                }
            }
            for pentry in raw.input.iter().chain(raw.output.iter()) {
                if let CspecPentry::Register {
                    name,
                    metatype,
                    storage,
                } = pentry
                {
                    if metatype.as_deref() == Some("float")
                        || storage.as_deref() == Some("hiddenret")
                    {
                        continue;
                    }
                    if model.lookup_name(name).is_none() {
                        missing.push(name.clone());
                    }
                }
            }
            assert!(
                missing.is_empty(),
                "unresolved int-proto register names for {abi:?} ({lang}/{comp}): {missing:?}"
            );
        }
    }
}
