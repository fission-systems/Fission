use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NirRenderOptions {
    pub pe_x64_only: bool,
    pub is_64bit: bool,
    #[serde(default)]
    pub is_big_endian: bool,
    pub pointer_size: u32,
    pub format: String,
    pub image_base: u64,
    pub sections: Vec<(u64, u64)>,
    pub region_linearize_structuring: bool,
    pub force_linear_structuring: bool,
    #[serde(default)]
    pub structuring_engine: StructuringEngineKind,
    #[serde(default)]
    pub conservative_irreducible_fallback: bool,
    #[serde(default)]
    pub is_data_ref_origin: bool,
    /// Address → symbol name for IAT slots and global data symbols.
    /// Used to replace `DAT_<addr>` with the actual symbol name in decompiled output.
    #[serde(default)]
    pub global_names: HashMap<u64, String>,
    /// Address → global data object byte size when loader metadata provides it.
    #[serde(default)]
    pub global_sizes: HashMap<u64, u64>,
    /// Relocation use-site address → referenced symbol name.
    #[serde(default)]
    pub relocation_names: HashMap<u64, String>,
    /// Calling convention used to identify parameter registers.
    /// Auto-detected from binary format in `from_loaded_binary`; can be overridden.
    #[serde(default)]
    pub calling_convention: CallingConvention,
    /// User-defined p-code operations (<userop_head> index -> name)
    #[serde(default)]
    pub userops: HashMap<u32, String>,
    /// Ghidra-style .cspec-resolved integer parameter register offsets (REGISTER-space).
    ///
    /// Set by `fission-decompiler` (or tests via `cspec::apply`) after resolving `.cspec`
    /// prototype register names against the SLA `ELEM_VARNODE_SYM` register map.
    /// Order matches the prototype `<input>` pentry order (float slots excluded).
    #[serde(default)]
    pub cspec_param_offsets: Option<Vec<u64>>,
    /// Stack base offset where stack arguments begin (from .cspec `<addr space="stack" offset=...>`).
    #[serde(default)]
    pub cspec_stack_arg_base: Option<i64>,
    /// Return-address stack size from .cspec prototype (`extrapop` / `stackshift`).
    ///
    /// Converts pre-call RSP-relative displacements into Ghidra stack-space offsets:
    /// `ghidra_offset = rsp_displacement + cspec_extrapop`.
    #[serde(default)]
    pub cspec_extrapop: Option<i64>,
    /// Ghidra-style SLA register map: REGISTER-space `(offset, size)` → hardware register name.
    ///
    /// Inverted from the `ELEM_VARNODE_SYM` table in the compiled `.sla` file.
    /// When populated, used by [`RegisterNamer`] as the primary offset→name table.
    /// primary offset→name lookup — replacing hardcoded architecture-specific offset tables.
    /// Covers all architectures uniformly (x86, AARCH64, ARM, MIPS, PowerPC, RISC-V, etc.).
    #[serde(default, skip)]
    pub sla_register_map: Option<HashMap<(u64, u32), String>>,

    /// Ghidra `.cspec` primary return register offset (REGISTER-space).
    #[serde(default, skip)]
    pub cspec_return_offset: Option<u64>,

    /// Ghidra `.cspec` float/double return register offset (REGISTER-space),
    /// e.g. x86's `ST0`. Distinct from `cspec_return_offset` -- a function
    /// returns through one or the other depending on its return type.
    #[serde(default, skip)]
    pub cspec_float_return_offset: Option<u64>,

    /// Ghidra `.cspec`/`.pspec` return-target (link register) offset when resolved.
    #[serde(default, skip)]
    pub cspec_return_target: Option<(u64, u32)>,

    // ── .pspec-derived fields ─────────────────────────────────────────────────
    /// Authoritative program counter register name from `.pspec` `<programcounter register="..."/>`.
    ///
    /// Examples: `"RIP"` (x86-64), `"pc"` (ARM/MIPS/AArch64), `"PC"` (PowerPC).
    /// Used to recognize and rewrite PC-relative addressing patterns in NIR output.
    #[serde(default, skip)]
    pub pspec_programcounter: Option<String>,

    /// Tracked register constants from `.pspec` `<context_data><tracked_set>`.
    ///
    /// Ghidra propagates these as constants throughout all functions.  The canonical
    /// case is `("DF", 0)` on x86/x86-64, which collapses direction-flag-dependent
    /// string operations (e.g. `MOVSD`/`MOVSB` in REP form) to their single-direction
    /// form.  Set by `fission-decompiler` alongside the `.cspec` overrides.
    ///
    /// Format: `(register_name, constant_value)` pairs.
    #[serde(default, skip)]
    pub pspec_tracked_context: Vec<(String, u64)>,

    /// Hidden register names from `.pspec` `<register_data><register hidden="true"/>`.
    ///
    /// These are SLEIGH-internal context variables (e.g. `bit64`, `segover`,
    /// `repneprefx`, `rexWprefix`, `rexBprefix`, `xmmTmp1`) that should never
    /// appear in decompiled output.  The NIR builder uses this set to filter
    /// register-space varnodes that map to hidden registers.
    #[serde(default, skip)]
    pub pspec_hidden_registers: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FormatFamily {
    Pe,
    Elf,
    MachO,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdmissionClass {
    PreviewUnsupported,
    PeX86PreviewOnly,
    PeX64Auto,
    GenericPreviewOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StructuringBudgetClass {
    None,
    PeX86Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum StructuringEngineKind {
    LegacyScored,
    #[default]
    GraphCollapseV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirAdmissionFacts {
    pub block_count: usize,
    pub op_count: usize,
    pub max_multiequal_fanin: usize,
}

impl NirAdmissionFacts {
    /// Build admission facts from precomputed p-code shape metrics.
    ///
    /// Callers that have a `PcodeFunction` should use the pcode-side adapter
    /// (`fission_pcode::midend::nir_admission_facts_from_pcode`) so this crate
    /// stays free of a reverse dependency on the lifter IR.
    pub fn from_counts(
        block_count: usize,
        op_count: usize,
        max_multiequal_fanin: usize,
    ) -> Self {
        Self {
            block_count,
            op_count,
            max_multiequal_fanin,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TargetProfile {
    pub format_family: FormatFamily,
    pub pointer_width: u32,
    pub admission_class: AdmissionClass,
    pub structuring_budget_class: StructuringBudgetClass,
    pub worker_eligible: bool,
    pub preview_eligible: bool,
}

impl TargetProfile {
    pub fn from_binary(binary: &LoadedBinary, pe_format_gate_enabled: bool) -> Self {
        Self::from_format(
            &binary.format,
            if binary.is_64bit { 64 } else { 32 },
            pe_format_gate_enabled,
        )
    }

    pub fn from_options(options: &NirRenderOptions) -> Self {
        Self::from_format(
            &options.format,
            options.pointer_size.saturating_mul(8),
            options.pe_x64_only,
        )
    }

    pub fn from_format(format: &str, pointer_width: u32, pe_format_gate_enabled: bool) -> Self {
        let format_upper = format.to_ascii_uppercase();
        let format_family = if format_upper.starts_with("PE") {
            FormatFamily::Pe
        } else if format_upper.starts_with("ELF") {
            FormatFamily::Elf
        } else if format_upper.starts_with("MACHO") || format_upper.starts_with("MACH-O") {
            FormatFamily::MachO
        } else {
            FormatFamily::Other
        };

        let preview_eligible = !pe_format_gate_enabled || format_family == FormatFamily::Pe;
        let worker_eligible =
            preview_eligible && format_family == FormatFamily::Pe && pointer_width == 64;
        let structuring_budget_class =
            if preview_eligible && format_family == FormatFamily::Pe && pointer_width == 32 {
                StructuringBudgetClass::PeX86Conditional
            } else {
                StructuringBudgetClass::None
            };
        let admission_class = match (preview_eligible, format_family, pointer_width) {
            (false, _, _) => AdmissionClass::PreviewUnsupported,
            (true, FormatFamily::Pe, 64) => AdmissionClass::PeX64Auto,
            (true, FormatFamily::Pe, 32) => AdmissionClass::PeX86PreviewOnly,
            (true, _, _) => AdmissionClass::GenericPreviewOnly,
        };

        Self {
            format_family,
            pointer_width,
            admission_class,
            structuring_budget_class,
            worker_eligible,
            preview_eligible,
        }
    }

    pub fn auto_admission_eligible(self, facts: NirAdmissionFacts) -> bool {
        self.worker_eligible
            && facts.block_count <= 12
            && facts.op_count <= 600
            && facts.max_multiequal_fanin <= 4
    }

    pub fn if_lowering_budget_enabled(self) -> bool {
        matches!(
            self.structuring_budget_class,
            StructuringBudgetClass::PeX86Conditional
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirTypeContext {
    pub call_targets: HashMap<u64, String>,
    pub call_target_refs: HashMap<u64, CallTargetRef>,
    #[serde(default)]
    pub iat_target_refs: HashMap<u64, CallTargetRef>,
    #[serde(default)]
    pub ambiguous_call_targets: HashSet<u64>,
    pub call_effect_summaries: HashMap<String, NirCallEffectSummary>,
    #[serde(default)]
    pub call_prototype_summaries: HashMap<String, NirCallPrototypeSummary>,
    pub call_param_rules: Vec<NirCallParamRule>,
    pub function_hints: Option<NirFunctionHints>,
    /// Struct/union/class layouts known from debug info (DWARF/PDB),
    /// keyed by type name. Used to name-overlay heuristically-recovered
    /// aggregate fields; see `NirStructTypeHint`.
    #[serde(default)]
    pub struct_types: HashMap<String, NirStructTypeHint>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirHintStats {
    pub explicit_param_name_hits: usize,
    pub explicit_local_name_hits: usize,
    pub explicit_param_type_hits: usize,
    pub explicit_local_type_hits: usize,
    pub explicit_return_type_hit: usize,
    pub pointer_alias_hits: usize,
    pub local_surface_hits: usize,
    pub derived_origin_type_hits: usize,
    /// Aggregate fields whose synthetic `field_{offset:x}` name was
    /// overlaid with the real name from a debug-info struct/union layout.
    pub debug_struct_field_hits: usize,
    /// Bindings promoted straight to `NirType::Ptr(Aggregate)` from a
    /// debug-info struct/union layout, bypassing `aggregate_fields.rs`'s
    /// own heuristic (which only promotes from `Ptr(Unknown | Int{8|16})`,
    /// not wider integer pointer types -- the common case for a struct
    /// whose first field is >= 32 bits).
    pub debug_struct_promotions: usize,
    /// Register-resident DWARF locals (`register_local_names`) renamed onto
    /// their matching Fission binding.
    pub explicit_register_local_name_hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirCallParamRule {
    pub callee_address: Option<u64>,
    pub callee_name: String,
    pub arg_index: usize,
    pub pointer_alias: String,
    pub pointee_alias: String,
    pub pointer_size: u32,
    pub pointee_sizes: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirFunctionHints {
    pub param_names: Vec<String>,
    pub param_type_names: HashMap<usize, String>,
    pub stack_local_names: HashMap<i64, String>,
    pub stack_local_type_names: HashMap<i64, String>,
    pub return_type_name: Option<String>,
    /// DWARF locals whose `DW_AT_location` is a single register for their
    /// whole visible scope (`DW_OP_reg*`, or a location list where every
    /// range agrees on the same register), keyed by the underlying SLEIGH
    /// register *offset* -- not by name, since most register-resident values
    /// get a generic `uVarN`/`iVarN` display name rather than their raw
    /// hardware register name, so name-matching alone misses most real cases
    /// (only caught the narrow call-result-register path). Keyed by offset
    /// only, not `(offset, size)`: DWARF's `DW_OP_reg*` doesn't carry a width
    /// (the register holds the value; the *variable's own type* says how wide
    /// it is), so a binding's access width can differ slightly from the
    /// declared type without meaning it's an unrelated value -- the separate
    /// "written at most once" safety gate is what actually guards against
    /// genuine reuse. The builder's materialization side channel
    /// (`register_origins` in `apply_preview_type_hints`) supplies each
    /// binding's actual originating register offset for the match. Unlike
    /// `stack_local_names`, a register has no stable per-function identity --
    /// it gets reused for unrelated values constantly -- so this is only safe
    /// to apply when the matching binding is written at most once in the
    /// function body (see `apply_function_name_hints`'s use of this field).
    pub register_local_names: HashMap<u64, String>,
}

/// One field of a struct/union/class type known from debug info (DWARF or
/// PDB), keyed by byte offset from the aggregate's base.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirStructFieldHint {
    pub name: String,
    pub type_name: String,
    pub offset: u32,
    pub size: u32,
}

/// A struct/union/class layout known from debug info, used to give
/// heuristically-recovered `NirType::Aggregate` fields (see
/// `fission-midend-normalize::memory::aggregate_fields`) their real
/// declared names instead of synthetic `field_{offset:x}` ones.
///
/// Deliberately holds only `name`/`type_name` strings, not a resolved
/// `NirType`: the aggregate-field-recovery pass already derives each
/// field's `NirType` from the actual observed load/store access width,
/// which is grounded in real pcode and safer to trust for cast/size
/// correctness than a naively re-parsed debug-info type string.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirStructTypeHint {
    pub name: String,
    pub size: u32,
    pub fields: Vec<NirStructFieldHint>,
}

impl NirRenderOptions {
    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let inner = binary.inner();
        let sections = inner
            .sections
            .iter()
            .filter(|section| section.is_readable || section.is_executable)
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size as u64,
                )
            })
            .collect();

        let mut global_names = inner.iat_symbols.clone();
        for (addr, name) in &inner.global_symbols {
            global_names.entry(*addr).or_insert_with(|| name.clone());
        }
        for (addr, value) in &inner.string_map {
            global_names
                .entry(*addr)
                .or_insert_with(|| format!("\"{}\"", value.escape_default()));
        }

        // Detect calling convention from the selected SLEIGH language first, then format.
        let fmt_upper = binary.format.to_ascii_uppercase();
        let lang_upper = binary
            .sleigh_language_id()
            .unwrap_or(&binary.arch_spec)
            .to_ascii_uppercase();
        let calling_convention = if lang_upper.starts_with("AARCH64:") {
            CallingConvention::AArch64
        } else if lang_upper.starts_with("ARM:") {
            CallingConvention::Arm32
        } else if lang_upper.starts_with("POWERPC:") {
            if binary.is_64bit {
                CallingConvention::PowerPc64
            } else {
                CallingConvention::PowerPc32
            }
        } else if lang_upper.starts_with("LOONGARCH:") {
            if binary.is_64bit {
                CallingConvention::LoongArch64
            } else {
                CallingConvention::LoongArch32
            }
        } else if lang_upper.starts_with("MIPS:") {
            if binary.is_64bit {
                CallingConvention::Mips64
            } else {
                CallingConvention::Mips32
            }
        } else if lang_upper.starts_with("X86:") {
            if binary.is_64bit {
                if fmt_upper.starts_with("ELF") || fmt_upper.starts_with("MACHO") {
                    CallingConvention::SystemVAmd64
                } else {
                    CallingConvention::WindowsX64
                }
            } else {
                CallingConvention::X86_32
            }
        } else if fmt_upper.starts_with("ELF") || fmt_upper.starts_with("MACHO") {
            CallingConvention::SystemVAmd64
        } else {
            CallingConvention::WindowsX64
        };

        let options = Self {
            pe_x64_only: true,
            is_64bit: binary.is_64bit,
            is_data_ref_origin: false,
            is_big_endian: binary
                .sleigh_language_id()
                .unwrap_or(&binary.arch_spec)
                .contains(":BE:"),
            pointer_size: if binary.is_64bit { 8 } else { 4 },
            format: binary.format.clone(),
            image_base: inner.image_base,
            sections,
            region_linearize_structuring: false,
            force_linear_structuring: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            conservative_irreducible_fallback: false,
            global_names,
            global_sizes: inner.global_symbol_sizes.clone(),
            relocation_names: inner.relocation_symbols.clone(),
            calling_convention,
            userops: HashMap::new(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_float_return_offset: None,
            cspec_return_target: None,
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: std::collections::HashSet::new(),
        };
        // SLA register map seeding lives in `fission-pcode` (cspec / register model)
        // so this crate does not depend on SLEIGH language resources. Callers that
        // need a populated map should use `fission_pcode::midend::seed_nir_render_options`.
        options
    }

    pub fn target_profile(&self) -> TargetProfile {
        TargetProfile::from_options(self)
    }

    pub fn effective_structuring_engine(&self) -> StructuringEngineKind {
        match std::env::var("FISSION_STRUCTURING_ENGINE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("graph")
            | Some("graphcollapsev1")
            | Some("graph_collapse_v1")
            | Some("graph-collapse-v1") => StructuringEngineKind::GraphCollapseV1,
            Some("legacy")
            | Some("legacyscored")
            | Some("legacy_scored")
            | Some("legacy-scored") => StructuringEngineKind::GraphCollapseV1,
            _ => self.structuring_engine,
        }
    }

    pub fn is_mapped_global(&self, address: u64) -> bool {
        self.sections
            .iter()
            .any(|(start, end)| address >= *start && address < *end)
    }

    /// Find the base address of the first mapped section that does not contain
    /// `image_base`.  In a relocatable object (.o) the code section (.text)
    /// starts at `image_base` and the read-only data section (.rodata) follows
    /// immediately after.  This heuristic returns the `.rodata` base when the
    /// jump table displacement has not been patched into instruction bytes.
    pub fn first_rodata_section_base(&self) -> Option<u64> {
        self.sections
            .iter()
            .filter(|(start, end)| {
                // Exclude the section that contains image_base (likely .text)
                // and skip zero-sized sections.
                *end > *start && !(self.image_base >= *start && self.image_base < *end)
            })
            .map(|(start, _)| *start)
            .next()
    }
}

pub type PreviewBuildStats = NirBuildStats;
pub type MlilPreviewOptions = NirRenderOptions;
pub type PreviewTypeContext = NirTypeContext;
pub type PreviewHintStats = NirHintStats;
pub type PreviewCallParamRule = NirCallParamRule;
pub type PreviewFunctionHints = NirFunctionHints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuringFailureKind {
    RegionShape,
    PhiJoin,
    IndirectCallRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecoveryMode {
    Structured,
    RegionLinearized,
    ForcedLinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StructuringReasonFamily {
    RegionLegality,
    FollowFailure,
    Irreducible,
    LoopExit,
    SwitchShape,
    Budget,
}

impl StructuringReasonFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            StructuringReasonFamily::RegionLegality => "region_legality",
            StructuringReasonFamily::FollowFailure => "follow_failure",
            StructuringReasonFamily::Irreducible => "irreducible",
            StructuringReasonFamily::LoopExit => "loop_exit",
            StructuringReasonFamily::SwitchShape => "switch_shape",
            StructuringReasonFamily::Budget => "budget",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuringOutcome {
    pub mode: RecoveryMode,
    pub reason_family: StructuringReasonFamily,
    pub retryable: bool,
    pub confidence: u8,
}

pub fn parse_call_target_address(target: &str) -> Option<u64> {
    for prefix in ["sub_", "FUN_0x", "FUN_", "DAT_0x", "DAT_"] {
        if let Some(rest) = target.strip_prefix(prefix) {
            return u64::from_str_radix(rest.trim_start_matches("0x"), 16).ok();
        }
    }
    None
}

pub fn structuring_outcome_for_signature(signature: &str) -> Option<StructuringOutcome> {
    let family = match signature {
        "unsupported_cfg_region_shape" | "unsupported_cfg_phi_join" => {
            StructuringReasonFamily::RegionLegality
        }
        "unsupported_cfg_indirect_call_region" => StructuringReasonFamily::FollowFailure,
        _ => return None,
    };
    Some(StructuringOutcome {
        mode: RecoveryMode::RegionLinearized,
        reason_family: family,
        retryable: true,
        confidence: 224,
    })
}

impl StructuringFailureKind {
    pub const fn preview_block_signature(self) -> &'static str {
        match self {
            StructuringFailureKind::RegionShape => "unsupported_cfg_region_shape",
            StructuringFailureKind::PhiJoin => "unsupported_cfg_phi_join",
            StructuringFailureKind::IndirectCallRegion => "unsupported_cfg_indirect_call_region",
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MlilPreviewError {
    #[error("mlil-preview currently supports PE x64 only")]
    UnsupportedArchitecture,
    #[error("unsupported architecture in mlil-preview")]
    UnsupportedArchitectureDetailed,
    #[error("unsupported control flow in mlil-preview")]
    UnsupportedControlFlow,
    #[error("unsupported branch target in mlil-preview")]
    UnsupportedCfgBranchTarget,
    #[error("unsupported region shape in mlil-preview")]
    UnsupportedCfgRegionShape,
    #[error("unsupported phi join in mlil-preview")]
    UnsupportedCfgPhiJoin,
    #[error("unsupported indirect call region in mlil-preview")]
    UnsupportedCfgIndirectCallRegion,
    #[error("unsupported pcode pattern: {0}")]
    UnsupportedPattern(&'static str),
    #[error("value lowering failed")]
    LoweringFailed,
    #[error("not a function (orphan block detected)")]
    NotAFunctionOrphanBlock,
    #[error("value lowering failed on multiequal")]
    UnsupportedExprMultiequal,
    #[error("value lowering failed on varnode")]
    UnsupportedExprVarnodeLowering,
    #[error("value lowering failed on varnode: unsupported address materialization")]
    UnsupportedExprAddressMaterialization,
    #[error("value lowering failed on varnode: unsupported indirect value source")]
    UnsupportedExprIndirectValueSource,
    #[error("value lowering failed on varnode: unsupported piece/subpiece shape")]
    UnsupportedExprPieceShape,
    #[error("value lowering failed on varnode: unsupported ptr arithmetic shape")]
    UnsupportedExprPtrArithmetic,
    #[error("value lowering failed on varnode: unsupported memory-backed varnode")]
    UnsupportedExprMemoryBackedVarnode,
}

impl MlilPreviewError {
    pub const fn structuring_failure_kind(self) -> Option<StructuringFailureKind> {
        match self {
            MlilPreviewError::UnsupportedCfgRegionShape => {
                Some(StructuringFailureKind::RegionShape)
            }
            MlilPreviewError::UnsupportedCfgPhiJoin => Some(StructuringFailureKind::PhiJoin),
            MlilPreviewError::UnsupportedCfgIndirectCallRegion => {
                Some(StructuringFailureKind::IndirectCallRegion)
            }
            _ => None,
        }
    }
}
