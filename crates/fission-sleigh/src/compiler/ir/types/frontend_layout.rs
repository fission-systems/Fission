#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSubtableDefinition {
    pub name: String,
    #[serde(default)]
    pub sla_subtable_id: u32,
    #[serde(default)]
    pub constructors_by_sla_id: BTreeMap<u32, usize>,
    pub constructors: Vec<CompiledExecutableConstructor>,
    pub decision_tree: CompiledDecisionTree,
    /// Precomputed walker decode policy (see `crate::compiler::decode_metadata`).
    #[serde(default)]
    pub cursor_policy_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFrontend {
    pub arch: String,
    pub default_context: u64,
    pub default_context_known_mask: u64,
    pub entry_spec: String,
    pub entry_id: String,
    pub include_manifest: Vec<String>,
    pub defines: Vec<(String, String)>,
    pub definitions: Vec<CompiledSpecDefinition>,
    pub macros: Vec<CompiledMacro>,
    pub constructors: Vec<CompiledConstructor>,
    pub subtables: BTreeMap<String, CompiledSubtableDefinition>,
    pub language_layout: CompiledLanguageLayout,
    pub construct_templates: Vec<CompiledConstructTpl>,
    pub pcode_ops: Vec<CompiledPcodeOp>,
    pub pattern_nodes: Vec<CompiledPatternNode>,
    /// Address spaces decoded from the `.sla` file (index → space ref).
    #[serde(default)]
    pub sla_spaces: BTreeMap<u64, CompiledSpaceRef>,
    /// Index of the unique (temporary) address space derived from `.sla`.
    /// Replaces the hardcoded `UNIQUE_SPACE_ID = 3` constant.
    #[serde(default)]
    pub sla_unique_space_index: u64,
    /// Index of the register address space derived from `.sla`.
    #[serde(default)]
    pub sla_register_space_index: u64,
    /// Base offset for unique temporary varnode allocation (`uniqbase` from `.sla`).
    #[serde(default)]
    pub sla_uniqbase: u64,
    /// Ghidra unique allocation mask (`ATTR_UNIQMASK`); used with instruction PC for temp bases.
    #[serde(default = "default_sla_uniqmask")]
    pub sla_uniqmask: u64,
    /// Ghidra `SleighLanguage.default_space` index from `.sla` `defaultspace` (`u64::MAX` if unknown).
    #[serde(default = "default_sla_max_index")]
    pub sla_default_space_index: u64,
    /// True when SLA operand token byte spans overlap (shared ModRM-style layout).
    #[serde(default)]
    pub uses_shared_token_layout: bool,
}

fn default_sla_uniqmask() -> u64 {
    u64::MAX
}

fn default_sla_max_index() -> u64 {
    u64::MAX
}

impl CompiledFrontend {
    /// Ghidra `ConstTpl.J_CURSPACE` / `ParserWalker.getCurSpace()`: the default
    /// non-const address space for pcode emission (typically `ram`).
    ///
    /// Algorithm (Ghidra-aligned): use decoded `sla_default_space_index` when present;
    /// else a sole `sleigh_is_ram_class` space; else name `ram`; else first non-const/unique/register.
    pub fn sla_default_cur_space_index(&self) -> anyhow::Result<u64> {
        if self.sla_default_space_index != u64::MAX {
            if self.sla_spaces.contains_key(&self.sla_default_space_index) {
                return Ok(self.sla_default_space_index);
            }
        }
        let ram_only: Vec<u64> = self
            .sla_spaces
            .iter()
            .filter(|(_, s)| s.sleigh_is_ram_class)
            .map(|(i, _)| *i)
            .collect();
        if ram_only.len() == 1 {
            return Ok(ram_only[0]);
        }
        if let Some((idx, _)) = self.sla_spaces.iter().find(|(_, s)| s.name == "ram") {
            return Ok(*idx);
        }
        self.sla_spaces
            .iter()
            .find(|(_, s)| {
                s.name != "const"
                    && !s.sleigh_is_unique_space
                    && s.name != "register"
                    && s.index != 0
            })
            .map(|(idx, _)| *idx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "SLA space table has no default address space for CurSpace (defaultspace / RAM class)"
                )
            })
    }

    /// Pointer size in bytes for [`Self::sla_default_cur_space_index`]'s space.
    pub fn sla_default_cur_space_pointer_size(&self) -> anyhow::Result<u32> {
        let idx = self.sla_default_cur_space_index()?;
        let space = self
            .sla_spaces
            .get(&idx)
            .ok_or_else(|| anyhow::anyhow!("CurSpace index {idx} missing from sla_spaces"))?;
        if space.addr_size == 0 {
            anyhow::bail!("SLA space {} has addr_size=0 (cannot resolve CurSpaceSize)", space.name);
        }
        Ok(space.addr_size)
    }

    /// Returns the pointer/address size in bytes for the RAM (default data) space.
    /// This is ATTRIB_SIZE in Ghidra (e.g. 4 for 32-bit, 8 for 64-bit).
    /// Falls back to 8 (64-bit) when the SLA did not encode an address size.
    pub fn sla_ram_address_size(&self) -> u32 {
        let pick = || {
            if self.sla_default_space_index != u64::MAX {
                self.sla_spaces.get(&self.sla_default_space_index)
            } else {
                None
            }
            .or_else(|| {
                self.sla_spaces
                    .values()
                    .find(|s| s.sleigh_is_ram_class)
            })
            .or_else(|| self.sla_spaces.values().find(|s| s.name == "ram"))
            .or_else(|| {
                self.sla_spaces.values().find(|s| {
                    s.index != 0 && !s.sleigh_is_unique_space && s.name != "const" && s.name != "register"
                })
            })
        };
        pick()
            .map(|s| s.addr_size)
            .filter(|&sz| sz > 0)
            .unwrap_or(8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledLanguageLayout {
    pub address_spaces: Vec<CompiledAddressSpace>,
    pub registers: Vec<CompiledRegister>,
    pub token_fields: Vec<CompiledTokenField>,
    pub context_fields: Vec<CompiledContextField>,
    pub subtables: Vec<CompiledSubtable>,
    pub display_templates: Vec<CompiledDisplayTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledAddressSpace {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledRegister {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledTokenField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSubtable {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDisplayTemplate {
    pub constructor_hash: u64,
    #[serde(default)]
    pub pieces: Vec<CompiledDisplayPiece>,
    #[serde(default)]
    pub first_whitespace: Option<usize>,
    #[serde(default)]
    pub flowthru_operand_index: Option<usize>,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledDisplayPiece {
    Literal(String),
    OperandRef(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSpecDefinition {
    pub kind: String,
    pub source: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledMacro {
    pub name: String,
    pub source: String,
    pub body_line_count: usize,
}

/// Deferred global context change (Ghidra `ContextCommit` / `globalset` statement).
///
/// When a constructor fires, its `context_commits` are queued. After the instruction
/// is decoded, `apply_context_commits()` resolves each commit's target address from the
/// fixed handle of the referenced symbol and writes the context bits to the context
/// cache for future instructions at that address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextCommit {
    /// Symbol table ID of the target operand (raw SLA `ATTR_ID`). Used for tracing.
    pub symbol_id: u32,
    /// Resolved operand handle index within the constructor's handle list.
    /// `u32::MAX` means the symbol is a built-in (e.g. `inst_next`): the target
    /// address is computed at runtime as `instruction_start + instruction_length`.
    pub hand_index: u32,
    /// Word index within the context register (Ghidra `ATTR_NUMBER`).
    pub word_index: u32,
    /// Bit mask of the context bits to commit (Ghidra `ATTR_MASK`).
    pub mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextOp {
    pub bit_offset: u32,
    pub bit_width: u32,
    pub value: u64,
    pub word_index: u32,
    pub mask: u64,
    pub shift: i32,
    pub expr: Option<CompiledPatternExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledConstructor {
    pub mnemonic: String,
    pub display: String,
    pub source: String,
    pub control_flow: ControlFlowClass,
    pub pattern_signature: String,
    pub semantic_template: CompiledSemanticTemplate,
    pub with_stack: Vec<String>,
    pub semantic_ops: Vec<String>,
    pub signature_hash: u64,
    pub context_changes: Vec<CompiledContextOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledExecutableConstructor {
    pub constructor_id: u32,
    #[serde(default)]
    pub sla_identity: Option<CompiledSlaConstructorIdentity>,
    #[serde(default)]
    pub sla_decode_status: CompiledSlaDecodeStatus,
    pub mnemonic: String,
    pub source: String,
    pub display: String,
    #[serde(default = "CompiledDisplayTemplate::empty")]
    pub display_template: CompiledDisplayTemplate,
    pub signature_hash: u64,
    pub minimum_length: u32,
    pub context_changes: Vec<CompiledContextOp>,
    /// Deferred global context commits (Ghidra `globalset` / `ContextCommit`).
    #[serde(default)]
    pub context_commits: Vec<CompiledContextCommit>,
    pub matcher: CompiledPatternMatcher,
    pub mod_constraint: Option<u8>,
    pub operand_reg_values: Vec<u8>,
    pub opsize_variants: Vec<u8>,
    pub operand_specs: Vec<CompiledOperandSpec>,
    #[serde(default)]
    pub display_operands: Vec<CompiledDisplayOperand>,
    pub construct_tpl_kind: CompiledConstructTplKind,
    pub constructor_template: CompiledConstructorTemplate,
    /// Named p-code sections from Ghidra's `namedtempl`.
    /// Index corresponds to the section number (ATTR_SECTION value).
    #[serde(default)]
    pub named_templates: Vec<Option<CompiledConstructTpl>>,
    pub runtime_ready: bool,
    pub unsupported_template_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSlaConstructorIdentity {
    #[serde(default)]
    pub subtable_id: u32,
    pub subtable_name: String,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub source_line: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledSlaDecodeStatus {
    Decoded,
    Unsupported,
}

