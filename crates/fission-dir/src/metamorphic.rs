// SPDX-License-Identifier: Apache-2.0
//! Metamorphic decompilation testing over semantics-preserving binary and IR
//! variants (Issue #1253).
//!
//! # Comparison Discipline & Shared Dependencies
//!
//! Relations compare **semantic observations** (`return_value` plus initialized
//! memory bytes at explicitly mapped addresses), never pseudocode strings,
//! graph edit distance (GED), or recovered variable names.
//!
//! Shared dependencies are recorded explicitly:
//! - IR-level transformations consume and emit [`fission_pcode_ir::PcodeFunction`].
//! - IR-level ground truth is established independently via
//!   [`eval_pcode_tiny_model`], a small self-contained P-code CFG interpreter
//!   that does **not** invoke Fission's midend builder, normalizer, structurer,
//!   or printer.
//! - Decompiled HIR artifacts are obtained via
//!   [`fission_decompiler::render_nir_with_context_output`] (IR variants) or
//!   [`crate::decompile_one`] (binary variants) and evaluated via
//!   [`crate::eval::run_hir`].
//! - Binary-level ground truth compares real machine execution via
//!   [`crate::emu_driver::EmulatorHarness`] and, when native host compilation is
//!   tested, independent direct execution of host-compiled binaries.
//! - [`RelationOutcome::Unsupported`], [`RelationOutcome::Inconclusive`], and
//!   [`RelationOutcome::RejectedPrecondition`] are distinct verdicts and are
//!   never coerced into [`RelationOutcome::Passed`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::{self, Write as _};

use anyhow::{Context, Result};
use fission_decompiler::{MlilPreviewOptions, render_nir_with_context_output};
use fission_pcode_ir::block_cfg::{
    block_terminator_op, build_address_to_index_map, build_layout_fallthrough_map,
    build_successor_index_map, resolve_branch_target_index,
};
use fission_pcode_ir::space::{
    REGISTER_SPACE_ID, RUST_SLEIGH_ALT_REGISTER_SPACE_ID, RUST_SLEIGH_REGISTER_SPACE_ID,
    RUST_SLEIGH_UNIQUE_SPACE_ID, UNIQUE_SPACE_ID,
};
use fission_pcode_ir::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

use crate::eval::{World, normalize, run_hir};

/// Default deterministic seed for smoke metamorphic test generation.
pub const DEFAULT_METAMORPHIC_SEED: u64 = 0x1253_51A7_C0DE_0001;

/// Fixed PR smoke sample budget per fixture/relation pair.
pub const DEFAULT_SMOKE_BUDGET: usize = 16;

/// Step limit for [`eval_pcode_tiny_model`] to fail explicitly on non-termination.
const TINY_MODEL_MAX_STEPS: usize = 4096;

/// Canonical base virtual address mapped for the 4-byte initialized scratch buffer
/// in IR-level metamorphic checks.
pub const IR_SCRATCH_ADDR: u64 = 0x7000_0000;

/// Space ID conventions across P-code frontends.
const CONSTANT_SPACE_ID: u64 = 0;

/// Categorized metamorphic relations covered by this harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationKind {
    IrBranchInvertSwap,
    IrIdentityOps,
    IrEdgeSplitBlockPermute,
    BinaryOptLevel,
    BinaryLayoutShift,
}

impl RelationKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IrBranchInvertSwap => "ir_branch_invert_swap",
            Self::IrIdentityOps => "ir_identity_ops",
            Self::IrEdgeSplitBlockPermute => "ir_edge_split_block_permute",
            Self::BinaryOptLevel => "binary_opt_level",
            Self::BinaryLayoutShift => "binary_layout_shift",
        }
    }
}

impl fmt::Display for RelationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Verified semantic preconditions attached to a fixture before running a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureProperties {
    pub defined_unsigned_only: bool,
    pub bounded_loops: bool,
    pub address_sensitive: bool,
}

impl FixtureProperties {
    /// Standard pure-unsigned bounded fixture with no address-dependent behavior.
    pub const PURE_DEFINED: Self = Self {
        defined_unsigned_only: true,
        bounded_loops: true,
        address_sensitive: false,
    };

    /// Address-sensitive fixture (ineligible for relocation/layout equivalence).
    pub const ADDRESS_SENSITIVE: Self = Self {
        defined_unsigned_only: true,
        bounded_loops: true,
        address_sensitive: true,
    };
}

/// One deterministic concrete input tuple for fixture execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetamorphicSample {
    pub arg0: u32,
    pub arg1: u32,
    pub initial_buffer: [u8; 4],
}

/// Comparable semantic observation of one execution under explicit input/buffer
/// address mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticObservation {
    pub return_value: u32,
    pub buffer: [u8; 4],
}

/// Recorded divergence witness for a metamorphic pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetamorphicWitness {
    pub sample_index: usize,
    pub sample: MetamorphicSample,
    pub detail: String,
}

/// Verdict of checking one fixture under one metamorphic relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationOutcome {
    Passed { checked: usize },
    Diverged { witnesses: Vec<MetamorphicWitness> },
    RejectedPrecondition { reason: String },
    Unsupported { reason: String },
    Inconclusive { reason: String },
}

impl RelationOutcome {
    #[must_use]
    pub fn category(&self) -> &'static str {
        match self {
            Self::Passed { .. } => "passed",
            Self::Diverged { .. } => "diverged",
            Self::RejectedPrecondition { .. } => "rejected_precondition",
            Self::Unsupported { .. } => "unsupported",
            Self::Inconclusive { .. } => "inconclusive",
        }
    }

    #[must_use]
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }
}

/// Explicit denominator counters tracked per `(relation, isa)` bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelationDenominator {
    pub total: usize,
    pub passed: usize,
    pub diverged: usize,
    pub rejected_precondition: usize,
    pub unsupported: usize,
    pub inconclusive: usize,
}

impl RelationDenominator {
    pub fn record(&mut self, outcome: &RelationOutcome) {
        self.total += 1;
        match outcome {
            RelationOutcome::Passed { .. } => self.passed += 1,
            RelationOutcome::Diverged { .. } => self.diverged += 1,
            RelationOutcome::RejectedPrecondition { .. } => self.rejected_precondition += 1,
            RelationOutcome::Unsupported { .. } => self.unsupported += 1,
            RelationOutcome::Inconclusive { .. } => self.inconclusive += 1,
        }
    }
}

/// Digest of a fixture source or compiled artifact recorded in [`MetamorphicManifest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDigest {
    pub name: String,
    pub isa: String,
    pub rel_path: String,
    pub sha256: String,
}

/// Deterministic manifest capturing seed, artifact hashes, shared evaluator
/// dependencies, per-`(relation, isa)` denominators, and reproduction command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetamorphicManifest {
    pub version: u32,
    pub seed: u64,
    pub smoke_budget: usize,
    pub shared_dependencies: Vec<String>,
    pub artifacts: Vec<ArtifactDigest>,
    pub denominators: BTreeMap<String, RelationDenominator>,
    pub repro_command: String,
}

impl MetamorphicManifest {
    #[must_use]
    pub fn new(seed: u64, smoke_budget: usize, repro_command: impl Into<String>) -> Self {
        Self {
            version: 1,
            seed,
            smoke_budget,
            shared_dependencies: vec![
                "fission-pcode-ir::PcodeFunction IR representation".to_string(),
                "fission-dir::metamorphic::eval_pcode_tiny_model (independent tiny P-code CFG interpreter)".to_string(),
                "fission-decompiler::render_nir_with_context_output + fission-dir::decompile_one".to_string(),
                "fission-dir::eval::run_hir (HIR tree interpreter)".to_string(),
                "fission-dir::emu_driver::EmulatorHarness (emulator ground truth)".to_string(),
            ],
            artifacts: Vec::new(),
            denominators: BTreeMap::new(),
            repro_command: repro_command.into(),
        }
    }

    pub fn record(&mut self, relation: RelationKind, isa: &str, outcome: &RelationOutcome) {
        let key = format!("{}::{}", relation.as_str(), isa);
        self.denominators.entry(key).or_default().record(outcome);
    }

    /// Deterministic canonical JSON representation for reproducible artifact manifests.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        let _ = writeln!(out, "  \"version\": {},", self.version);
        let _ = writeln!(out, "  \"seed\": \"0x{:016x}\",", self.seed);
        let _ = writeln!(out, "  \"smoke_budget\": {},", self.smoke_budget);
        let _ = writeln!(
            out,
            "  \"repro_command\": \"{}\",",
            json_escape(&self.repro_command)
        );
        out.push_str("  \"shared_dependencies\": [\n");
        for (i, dep) in self.shared_dependencies.iter().enumerate() {
            let comma = if i + 1 < self.shared_dependencies.len() {
                ","
            } else {
                ""
            };
            let _ = writeln!(out, "    \"{}\"{comma}", json_escape(dep));
        }
        out.push_str("  ],\n");
        out.push_str("  \"artifacts\": [\n");
        for (i, art) in self.artifacts.iter().enumerate() {
            let comma = if i + 1 < self.artifacts.len() {
                ","
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    {{\"name\": \"{}\", \"isa\": \"{}\", \"rel_path\": \"{}\", \"sha256\": \"{}\"}}{comma}",
                json_escape(&art.name),
                json_escape(&art.isa),
                json_escape(&art.rel_path),
                json_escape(&art.sha256),
            );
        }
        out.push_str("  ],\n");
        out.push_str("  \"denominators\": {\n");
        let total_keys = self.denominators.len();
        for (idx, (key, den)) in self.denominators.iter().enumerate() {
            let comma = if idx + 1 < total_keys { "," } else { "" };
            let _ = writeln!(
                out,
                "    \"{}\": {{\"total\": {}, \"passed\": {}, \"diverged\": {}, \"rejected_precondition\": {}, \"unsupported\": {}, \"inconclusive\": {}}}{comma}",
                json_escape(key),
                den.total,
                den.passed,
                den.diverged,
                den.rejected_precondition,
                den.unsupported,
                den.inconclusive
            );
        }
        out.push_str("  }\n");
        out.push('}');
        out
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Deterministic SplitMix64 PRNG for seeded metamorphic sampling and permutation.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Generates a deterministic list of unsigned concrete samples combining
/// boundary cases with seeded pseudorandom draws.
#[must_use]
pub fn generate_smoke_samples(seed: u64, budget: usize) -> Vec<MetamorphicSample> {
    if budget == 0 {
        return Vec::new();
    }
    const BOUNDARIES: &[(u32, u32, [u8; 4])] = &[
        (0, 0, [0, 0, 0, 0]),
        (1, 2, [19, 83, 227, 41]),
        (3, 0, [0xff, 0x01, 0x80, 0x7f]),
        (0x7fff_ffff, 0x8000_0000, [10, 20, 30, 40]),
        (0xffff_ffff, 1, [0xaa, 0x55, 0x00, 0xff]),
        (0x1234_5678, 0x9abc_def1, [0x11, 0x22, 0x33, 0x44]),
    ];

    let mut samples = Vec::with_capacity(budget);
    for &(a0, a1, buf) in BOUNDARIES.iter().take(budget) {
        samples.push(MetamorphicSample {
            arg0: a0,
            arg1: a1,
            initial_buffer: buf,
        });
    }
    let mut rng = SplitMix64(seed);
    while samples.len() < budget {
        let x = rng.next_u64();
        let y = rng.next_u64();
        samples.push(MetamorphicSample {
            arg0: x as u32,
            arg1: (x >> 32) as u32,
            initial_buffer: [
                (y & 0xff) as u8,
                ((y >> 8) & 0xff) as u8,
                ((y >> 16) & 0xff) as u8,
                ((y >> 24) & 0xff) as u8,
            ],
        });
    }
    samples
}

/// Checks verified transformation preconditions before accepting a binary-level
/// metamorphic relation.
pub fn check_binary_relation_precondition(
    relation: RelationKind,
    props: &FixtureProperties,
) -> std::result::Result<(), String> {
    if !props.defined_unsigned_only {
        return Err("fixture is not restricted to defined unsigned arithmetic".to_string());
    }
    if !props.bounded_loops {
        return Err("fixture does not carry a bounded-loop guarantee".to_string());
    }
    if matches!(relation, RelationKind::BinaryLayoutShift) && props.address_sensitive {
        return Err(
            "layout-shift relation is inapplicable to address-sensitive functions".to_string(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// IR-level P-code Metamorphic Transformations & Negative Controls
// ---------------------------------------------------------------------------

fn max_unique_end(pcode: &PcodeFunction) -> u64 {
    let mut max_end = 0x1000u64;
    for block in &pcode.blocks {
        for op in &block.ops {
            for vn in op.output.iter().chain(op.inputs.iter()) {
                if vn.space_id == UNIQUE_SPACE_ID || vn.space_id == RUST_SLEIGH_UNIQUE_SPACE_ID {
                    max_end = max_end.max(vn.offset.saturating_add(u64::from(vn.size)));
                }
            }
        }
    }
    max_end
}

fn dominant_unique_space(pcode: &PcodeFunction) -> u64 {
    for block in &pcode.blocks {
        for op in &block.ops {
            for vn in op.output.iter().chain(op.inputs.iter()) {
                if vn.space_id == UNIQUE_SPACE_ID || vn.space_id == RUST_SLEIGH_UNIQUE_SPACE_ID {
                    return vn.space_id;
                }
            }
        }
    }
    UNIQUE_SPACE_ID
}

fn max_block_address_aligned(pcode: &PcodeFunction) -> u64 {
    let max_addr = pcode
        .blocks
        .iter()
        .map(|b| {
            let last_op_addr = b.ops.iter().map(|o| o.address).max().unwrap_or(0);
            b.start_address.max(last_op_addr)
        })
        .max()
        .unwrap_or(0x1000);
    (max_addr + 0x100) & !0xfu64
}

fn allocate_fresh_block_address(pcode: &PcodeFunction, hint: u64) -> u64 {
    let used: BTreeSet<u64> = pcode
        .blocks
        .iter()
        .flat_map(|b| std::iter::once(b.start_address).chain(b.ops.iter().map(|op| op.address)))
        .collect();
    let mut candidate = hint;
    while used.contains(&candidate) {
        candidate = candidate.saturating_add(0x10);
    }
    candidate
}

fn unique_var(space_id: u64, offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn renumber_block_seqs(block: &mut PcodeBasicBlock) {
    let mut current_addr = None;
    let mut next_seq = 0u32;
    for op in &mut block.ops {
        if current_addr != Some(op.address) {
            current_addr = Some(op.address);
            next_seq = 0;
        }
        op.seq_num = next_seq;
        next_seq = next_seq.saturating_add(1);
    }
}

fn recompute_block_indices_and_successors(pcode: &mut PcodeFunction) {
    for (idx, block) in pcode.blocks.iter_mut().enumerate() {
        block.index = idx as u32;
    }
    let addr_to_idx = build_address_to_index_map(pcode);
    let fallthrough = build_layout_fallthrough_map(pcode);
    let succs = build_successor_index_map(pcode, &addr_to_idx, &fallthrough);
    for (block, s) in pcode.blocks.iter_mut().zip(succs) {
        block.successors = s.into_iter().map(|i| i as u32).collect();
    }
}

/// **Relation 1 (IR): Branch Inversion with Swapped Successors.**
///
/// Precondition: the function must contain at least one conditional branch
/// (`PcodeOpcode::CBranch`) with a resolvable distinct taken target and a layout
/// fallthrough block.
///
/// Transformation: replaces `CBranch(taken_addr, cond)` with `inv = !cond;
/// CBranch(fallthrough_addr, inv)` and inserts a forwarding trampoline block
/// immediately after the `CBranch` block that branches unconditionally to
/// `taken_addr` so Fission's layout fallthrough semantics remain exact.
pub fn transform_branch_invert_swap(
    pcode: &PcodeFunction,
) -> std::result::Result<PcodeFunction, String> {
    if pcode.blocks.is_empty() {
        return Err("empty PcodeFunction has no conditional branches".to_string());
    }
    let addr_to_idx = build_address_to_index_map(pcode);
    let Some((cbranch_block_idx, old_taken_addr, old_fallthrough_addr)) =
        pcode.blocks.iter().enumerate().find_map(|(idx, block)| {
            let term = block_terminator_op(block)?;
            if term.opcode != PcodeOpcode::CBranch || term.inputs.len() < 2 {
                return None;
            }
            let target_idx =
                resolve_branch_target_index(pcode, &addr_to_idx, idx, term, &term.inputs[0])?;
            let fallthrough_block = pcode.blocks.get(idx + 1)?;
            let taken_addr = pcode.blocks[target_idx].start_address;
            let fallthrough_addr = fallthrough_block.start_address;
            if taken_addr == fallthrough_addr {
                return None;
            }
            Some((idx, taken_addr, fallthrough_addr))
        })
    else {
        return Err(
            "precondition failed: no conditional CBranch with distinct taken/fallthrough successors"
                .to_string(),
        );
    };

    let mut out = pcode.clone();
    let u_space = dominant_unique_space(&out);
    let inv_var = unique_var(u_space, max_unique_end(&out).saturating_add(0x100), 1);
    let trampoline_addr = allocate_fresh_block_address(&out, max_block_address_aligned(&out));

    let block = &mut out.blocks[cbranch_block_idx];
    let term_pos = block
        .ops
        .iter()
        .rposition(|op| op.opcode == PcodeOpcode::CBranch)
        .ok_or_else(|| "internal error locating CBranch terminator".to_string())?;
    let cbranch_addr = block.ops[term_pos].address;
    let orig_cond = block.ops[term_pos].inputs[1].clone();

    if orig_cond.size != 1 {
        return Err(format!(
            "precondition failed: CBranch condition size {} is not 1-byte boolean",
            orig_cond.size
        ));
    }

    let neg_op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolNegate,
        address: cbranch_addr,
        output: Some(inv_var.clone()),
        inputs: vec![orig_cond],
        asm_mnemonic: None,
    };
    block.ops[term_pos].inputs = vec![Varnode::constant(old_fallthrough_addr as i64, 8), inv_var];
    block.ops.insert(term_pos, neg_op);
    renumber_block_seqs(block);

    let trampoline = PcodeBasicBlock {
        index: 0,
        start_address: trampoline_addr,
        successors: Vec::new(),
        ops: vec![PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Branch,
            address: trampoline_addr,
            output: None,
            inputs: vec![Varnode::constant(old_taken_addr as i64, 8)],
            asm_mnemonic: None,
        }],
    };
    out.blocks.insert(cbranch_block_idx + 1, trampoline);
    recompute_block_indices_and_successors(&mut out);
    Ok(out)
}

/// **Negative Control for Branch Inversion:** negates a `CBranch` condition
/// **without** swapping the taken and fallthrough successors. Must be detected
/// as `Diverged`.
pub fn negative_flip_branch_without_swap(
    pcode: &PcodeFunction,
) -> std::result::Result<PcodeFunction, String> {
    let mut out = pcode.clone();
    let u_space = dominant_unique_space(&out);
    let inv_var = unique_var(u_space, max_unique_end(&out).saturating_add(0x100), 1);

    for block in &mut out.blocks {
        if let Some(term_pos) = block
            .ops
            .iter()
            .rposition(|op| op.opcode == PcodeOpcode::CBranch && op.inputs.len() >= 2)
        {
            let addr = block.ops[term_pos].address;
            let orig_cond = block.ops[term_pos].inputs[1].clone();
            if orig_cond.size != 1 {
                continue;
            }
            let neg_op = PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::BoolNegate,
                address: addr,
                output: Some(inv_var.clone()),
                inputs: vec![orig_cond],
                asm_mnemonic: None,
            };
            block.ops[term_pos].inputs[1] = inv_var;
            block.ops.insert(term_pos, neg_op);
            renumber_block_seqs(block);
            recompute_block_indices_and_successors(&mut out);
            return Ok(out);
        }
    }
    Err("no eligible CBranch for negative control".to_string())
}

fn is_eligible_scalar_alu_op(op: &PcodeOp, min_size: u32) -> bool {
    let Some(out) = &op.output else {
        return false;
    };
    if !matches!(out.size, 1 | 2 | 4 | 8) || out.size < min_size {
        return false;
    }
    // Avoid rewriting hardware flag outputs or zero-sized outputs directly if trivial.
    matches!(
        op.opcode,
        PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntMult
    )
}

/// **Relation 2 (IR): Identity Operations (`+ 0`, `^ 0`, `| 0`) over word-width
/// (4/8-byte) ALU outputs.**
///
/// Sub-word 1-byte ALU outputs fed into `IntZExt` currently trigger Issue #1426
/// (spurious `char` sign extension) and are tested separately via
/// [`transform_identity_ops_including_subword`].
pub fn transform_identity_ops(pcode: &PcodeFunction) -> std::result::Result<PcodeFunction, String> {
    transform_identity_ops_with_min_size(pcode, 4)
}

/// Same identity-op transformation as [`transform_identity_ops`], including
/// 1/2-byte sub-word integer ALU outputs (used to pin Issue #1426).
pub fn transform_identity_ops_including_subword(
    pcode: &PcodeFunction,
) -> std::result::Result<PcodeFunction, String> {
    transform_identity_ops_with_min_size(pcode, 1)
}

fn transform_identity_ops_with_min_size(
    pcode: &PcodeFunction,
    min_size: u32,
) -> std::result::Result<PcodeFunction, String> {
    let mut out = pcode.clone();
    let u_space = dominant_unique_space(&out);
    let mut next_unique = max_unique_end(&out).saturating_add(0x100);
    let mut transformed = 0usize;

    for block in &mut out.blocks {
        let mut new_ops = Vec::with_capacity(block.ops.len() + 3);
        for mut op in block.ops.drain(..) {
            if transformed < 3 && is_eligible_scalar_alu_op(&op, min_size) {
                let orig_out = op.output.clone().expect("checked");
                let tmp = unique_var(u_space, next_unique, orig_out.size);
                next_unique = next_unique.saturating_add(16);
                let addr = op.address;
                op.output = Some(tmp.clone());
                new_ops.push(op);

                let id_opcode = match transformed % 3 {
                    0 => PcodeOpcode::IntXor,
                    1 => PcodeOpcode::IntAdd,
                    _ => PcodeOpcode::IntOr,
                };
                new_ops.push(PcodeOp {
                    seq_num: 0,
                    opcode: id_opcode,
                    address: addr,
                    output: Some(orig_out.clone()),
                    inputs: vec![tmp, Varnode::constant(0, orig_out.size)],
                    asm_mnemonic: None,
                });
                transformed += 1;
            } else {
                new_ops.push(op);
            }
        }
        block.ops = new_ops;
        renumber_block_seqs(block);
    }

    if transformed == 0 {
        return Err(
            "precondition failed: no eligible integer ALU op for identity insertion".to_string(),
        );
    }
    recompute_block_indices_and_successors(&mut out);
    Ok(out)
}

/// **Negative Control for Identity Operations:** flips bit 0 (`x ^ 1`) instead
/// of inserting an identity (`x ^ 0`). Must be detected as `Diverged`.
pub fn negative_perturb_arith_result(
    pcode: &PcodeFunction,
) -> std::result::Result<PcodeFunction, String> {
    let mut out = pcode.clone();
    let u_space = dominant_unique_space(&out);
    let tmp_off = max_unique_end(&out).saturating_add(0x100);

    for block in &mut out.blocks {
        for pos in 0..block.ops.len() {
            if is_eligible_scalar_alu_op(&block.ops[pos], 1) {
                let orig_out = block.ops[pos].output.clone().expect("checked");
                let tmp = unique_var(u_space, tmp_off, orig_out.size);
                let addr = block.ops[pos].address;
                block.ops[pos].output = Some(tmp.clone());
                let perturb = PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntXor,
                    address: addr,
                    output: Some(orig_out.clone()),
                    inputs: vec![tmp, Varnode::constant(1, orig_out.size)],
                    asm_mnemonic: None,
                };
                block.ops.insert(pos + 1, perturb);
                renumber_block_seqs(block);
                recompute_block_indices_and_successors(&mut out);
                return Ok(out);
            }
        }
    }
    Err("no eligible integer ALU op for negative perturbation".to_string())
}

/// **Relation 3 (IR): Safe Edge Splitting & Block-ID / Layout Permutation.**
///
/// Precondition: the CFG must contain at least 2 basic blocks and no unresolved
/// `BranchInd` terminators; any block whose edge is split must have a valid
/// resolved successor target.
///
/// Transformation:
/// 1. Converts implicit non-`CBranch` layout fallthroughs into explicit `Branch`
///    terminators so block order can be permuted without altering control flow.
/// 2. Materializes explicit forwarding trampolines after any `CBranch` blocks so
///    each `(CBranch block, fallthrough trampoline)` stays coupled as an atomic
///    layout unit during permutation.
/// 3. Inserts a fresh forwarding edge-split block along one resolved branch edge.
/// 4. Deterministically permutes all non-entry block units under `seed`, keeping
///    entry block `blocks[0]` first, and renumbers `block.index` & `successors`.
pub fn transform_edge_split_and_permute(
    pcode: &PcodeFunction,
    seed: u64,
) -> std::result::Result<PcodeFunction, String> {
    if pcode.blocks.len() < 2 {
        return Err(
            "precondition failed: edge split and permutation requires at least 2 basic blocks"
                .to_string(),
        );
    }
    if pcode.blocks.iter().any(|b| {
        matches!(
            block_terminator_op(b).map(|op| op.opcode),
            Some(PcodeOpcode::BranchInd)
        )
    }) {
        return Err(
            "precondition failed: indirect BranchInd CFG is outside safe permutation scope"
                .to_string(),
        );
    }

    let mut work = pcode.clone();
    let orig_len = work.blocks.len();

    // Step 1: make all non-terminator fallthroughs explicit `Branch(next_addr)`.
    for idx in 0..orig_len {
        let needs_explicit_branch = match block_terminator_op(&work.blocks[idx]).map(|op| op.opcode)
        {
            Some(PcodeOpcode::Return | PcodeOpcode::Branch | PcodeOpcode::CBranch) => false,
            _ => true,
        };
        if needs_explicit_branch {
            let next_addr = work
                .blocks
                .get(idx + 1)
                .map(|b| b.start_address)
                .ok_or_else(|| {
                    format!(
                        "precondition failed: block {idx} falls through past end of function without Return"
                    )
                })?;
            let addr = work.blocks[idx]
                .ops
                .last()
                .map(|o| o.address)
                .unwrap_or(work.blocks[idx].start_address);
            work.blocks[idx].ops.push(PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Branch,
                address: addr,
                output: None,
                inputs: vec![Varnode::constant(next_addr as i64, 8)],
                asm_mnemonic: None,
            });
            renumber_block_seqs(&mut work.blocks[idx]);
        }
    }

    // Step 2: split one unconditional Branch target with a fresh forwarding block,
    // and wrap any CBranch fallthroughs with immediate forwarding blocks so each
    // (CBranch, fallthrough_trampoline) pair can move freely as a layout unit.
    let mut next_addr_hint = max_block_address_aligned(&work);
    let mut used_addrs: BTreeSet<u64> = work
        .blocks
        .iter()
        .flat_map(|b| std::iter::once(b.start_address).chain(b.ops.iter().map(|o| o.address)))
        .collect();
    let mut alloc_addr = || {
        while used_addrs.contains(&next_addr_hint) {
            next_addr_hint = next_addr_hint.saturating_add(0x10);
        }
        let a = next_addr_hint;
        used_addrs.insert(a);
        next_addr_hint = next_addr_hint.saturating_add(0x10);
        a
    };

    // Split the first Branch or CBranch taken target we find.
    let mut edge_split_block: Option<PcodeBasicBlock> = None;
    let addr_to_idx = build_address_to_index_map(&work);
    for idx in 0..work.blocks.len() {
        if let Some(term) = block_terminator_op(&work.blocks[idx]).cloned() {
            if matches!(term.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
                && !term.inputs.is_empty()
                && let Some(target_idx) =
                    resolve_branch_target_index(&work, &addr_to_idx, idx, &term, &term.inputs[0])
            {
                let orig_target = work.blocks[target_idx].start_address;
                let split_addr = alloc_addr();
                // Rewrite the branch target in this block to `split_addr`.
                if let Some(term_mut) = work.blocks[idx].ops.iter_mut().rev().find(|op| {
                    matches!(op.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
                        && op.address == term.address
                        && op.seq_num == term.seq_num
                }) {
                    term_mut.inputs[0] = Varnode::constant(split_addr as i64, 8);
                }
                edge_split_block = Some(PcodeBasicBlock {
                    index: 0,
                    start_address: split_addr,
                    successors: Vec::new(),
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: split_addr,
                        output: None,
                        inputs: vec![Varnode::constant(orig_target as i64, 8)],
                        asm_mnemonic: None,
                    }],
                });
                break;
            }
        }
    }
    let split_block = edge_split_block.ok_or_else(|| {
        "precondition failed: no resolvable Branch/CBranch edge available to split".to_string()
    })?;

    // Build layout units: entry stays at index 0; if entry ends in CBranch, keep
    // a dedicated immediate fallthrough trampoline pinned at index 1 so the rest
    // of the blocks can permute freely.
    let mut units: Vec<Vec<PcodeBasicBlock>> = Vec::new();
    let old_blocks = std::mem::take(&mut work.blocks);
    for (idx, block) in old_blocks.iter().enumerate() {
        let ends_cbranch =
            block_terminator_op(block).is_some_and(|op| op.opcode == PcodeOpcode::CBranch);
        if ends_cbranch {
            let fallthrough_addr = old_blocks
                .get(idx + 1)
                .map(|b| b.start_address)
                .ok_or_else(|| format!("block {idx} CBranch lacks fallthrough block"))?;
            let tramp_addr = alloc_addr();
            let tramp = PcodeBasicBlock {
                index: 0,
                start_address: tramp_addr,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: tramp_addr,
                    output: None,
                    inputs: vec![Varnode::constant(fallthrough_addr as i64, 8)],
                    asm_mnemonic: None,
                }],
            };
            units.push(vec![block.clone(), tramp]);
        } else {
            units.push(vec![block.clone()]);
        }
    }
    units.push(vec![split_block]);

    // Entry unit stays at index 0; permute units[1..] using SplitMix64(seed).
    let mut rng = SplitMix64(seed ^ 0x517c_c1b7_2722_0a95);
    if units.len() > 2 {
        let tail = &mut units[1..];
        for i in (1..tail.len()).rev() {
            let j = (rng.next_u64() as usize) % (i + 1);
            tail.swap(i, j);
        }
        // Guarantee a genuine non-identity block permutation when >= 2 movable units exist.
        if tail.len() >= 2 && tail[0][0].start_address == old_blocks[1].start_address {
            tail.swap(0, 1);
        }
    }

    work.blocks = units.into_iter().flatten().collect();
    recompute_block_indices_and_successors(&mut work);
    Ok(work)
}

// ---------------------------------------------------------------------------
// Independent Tiny P-code CFG Evaluator
// ---------------------------------------------------------------------------

/// Failures reported explicitly by [`eval_pcode_tiny_model`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcodeEvalError {
    UnsupportedOpcode(PcodeOpcode),
    BudgetExceeded(usize),
    UnresolvedBranch(u64),
    InvalidOperandCount(PcodeOpcode),
    UnboundedDivideByZero,
    OutOfMemoryRead(u64),
    EmptyFunction,
    FellThroughWithoutReturn,
}

impl fmt::Display for PcodeEvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOpcode(op) => {
                write!(f, "unsupported P-code opcode in tiny model: {op:?}")
            }
            Self::BudgetExceeded(steps) => write!(f, "tiny P-code model exceeded {steps} steps"),
            Self::UnresolvedBranch(addr) => write!(f, "unresolved branch target {addr:#x}"),
            Self::InvalidOperandCount(op) => write!(f, "invalid operand count for {op:?}"),
            Self::UnboundedDivideByZero => f.write_str("unsigned/signed division by zero"),
            Self::OutOfMemoryRead(addr) => write!(f, "uninitialized RAM read at {addr:#x}"),
            Self::EmptyFunction => f.write_str("empty PcodeFunction"),
            Self::FellThroughWithoutReturn => f.write_str("execution fell through past last block"),
        }
    }
}

#[derive(Default)]
struct ByteStorage {
    bytes: HashMap<u64, u8>,
}

impl ByteStorage {
    fn read_u64(&self, offset: u64, size: u32) -> u64 {
        let n = size.clamp(1, 8);
        let mut val = 0u64;
        for i in 0..u64::from(n) {
            let b = self.bytes.get(&(offset + i)).copied().unwrap_or(0);
            val |= u64::from(b) << (i * 8);
        }
        truncate_u64(val, n)
    }

    fn write_u64(&mut self, offset: u64, size: u32, val: u64) {
        let n = size.clamp(1, 8);
        let masked = truncate_u64(val, n);
        for i in 0..u64::from(n) {
            self.bytes
                .insert(offset + i, ((masked >> (i * 8)) & 0xff) as u8);
        }
    }
}

fn truncate_u64(val: u64, size_bytes: u32) -> u64 {
    let bits = size_bytes.saturating_mul(8).min(64);
    if bits == 0 {
        0
    } else if bits >= 64 {
        val
    } else {
        val & ((1u64 << bits) - 1)
    }
}

fn sign_extend_u64(val: u64, size_bytes: u32) -> i64 {
    let bits = size_bytes.saturating_mul(8).clamp(1, 64);
    let masked = truncate_u64(val, size_bytes);
    let shift = 64 - bits;
    ((masked << shift) as i64) >> shift
}

fn is_reg_space(space_id: u64) -> bool {
    space_id == REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_ALT_REGISTER_SPACE_ID
}

fn is_uniq_space(space_id: u64) -> bool {
    space_id == UNIQUE_SPACE_ID || space_id == RUST_SLEIGH_UNIQUE_SPACE_ID
}

/// Evaluates `pcode` directly with an independent, self-contained P-code CFG
/// interpreter (no Fission midend/structurer/HIR dependencies).
///
/// Parameter values `reg_inits` supply initial register values `(reg_offset,
/// byte_size, value)` (for example SysV64 `RDI=0x38`, `RSI=0x30`). `ram_init`
/// preloads initialized memory bytes. Returns [`SemanticObservation`] on
/// `Return` or an explicit [`PcodeEvalError`] (never coerces unsupported ops
/// into a default success value).
pub fn eval_pcode_tiny_model(
    pcode: &PcodeFunction,
    reg_inits: &[(u64, u32, u64)],
    ram_init: &[(u64, u8)],
    scratch_base: u64,
) -> std::result::Result<SemanticObservation, PcodeEvalError> {
    if pcode.blocks.is_empty() {
        return Err(PcodeEvalError::EmptyFunction);
    }

    let addr_to_idx = build_address_to_index_map(pcode);
    let mut regs = ByteStorage::default();
    let mut uniqs = ByteStorage::default();
    let mut ram: BTreeMap<u64, u8> = ram_init.iter().copied().collect();

    for &(off, size, val) in reg_inits {
        regs.write_u64(off, size, val);
    }

    let read_vn = |vn: &Varnode,
                   regs: &ByteStorage,
                   uniqs: &ByteStorage|
     -> std::result::Result<u64, PcodeEvalError> {
        if vn.is_constant || vn.space_id == CONSTANT_SPACE_ID {
            Ok(truncate_u64(vn.constant_val as u64, vn.size))
        } else if is_reg_space(vn.space_id) {
            Ok(regs.read_u64(vn.offset, vn.size))
        } else if is_uniq_space(vn.space_id) {
            Ok(uniqs.read_u64(vn.offset, vn.size))
        } else {
            // Direct RAM varnode reads not used directly unless Load is used.
            Err(PcodeEvalError::UnsupportedOpcode(PcodeOpcode::Unknown))
        }
    };

    let write_vn = |vn: &Varnode,
                    val: u64,
                    regs: &mut ByteStorage,
                    uniqs: &mut ByteStorage|
     -> std::result::Result<(), PcodeEvalError> {
        if is_reg_space(vn.space_id) {
            regs.write_u64(vn.offset, vn.size, val);
            Ok(())
        } else if is_uniq_space(vn.space_id) {
            uniqs.write_u64(vn.offset, vn.size, val);
            Ok(())
        } else {
            Err(PcodeEvalError::UnsupportedOpcode(PcodeOpcode::Unknown))
        }
    };

    let mut block_idx = 0usize;
    let mut op_idx = 0usize;
    let mut steps = 0usize;

    loop {
        if steps >= TINY_MODEL_MAX_STEPS {
            return Err(PcodeEvalError::BudgetExceeded(steps));
        }
        steps += 1;

        let block = &pcode.blocks[block_idx];
        if op_idx >= block.ops.len() {
            if block_idx + 1 < pcode.blocks.len() {
                block_idx += 1;
                op_idx = 0;
                continue;
            }
            return Err(PcodeEvalError::FellThroughWithoutReturn);
        }

        let op = &block.ops[op_idx];
        match op.opcode {
            PcodeOpcode::Return => {
                let ret = if let Some(val_vn) = op.inputs.get(1) {
                    read_vn(val_vn, &regs, &uniqs)?
                } else {
                    regs.read_u64(0x00, 8)
                };
                let buffer = [
                    ram.get(&scratch_base).copied().unwrap_or(0),
                    ram.get(&(scratch_base + 1)).copied().unwrap_or(0),
                    ram.get(&(scratch_base + 2)).copied().unwrap_or(0),
                    ram.get(&(scratch_base + 3)).copied().unwrap_or(0),
                ];
                return Ok(SemanticObservation {
                    return_value: ret as u32,
                    buffer,
                });
            }
            PcodeOpcode::Branch => {
                let target_vn = op
                    .inputs
                    .first()
                    .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Branch))?;
                if target_vn.is_constant || target_vn.space_id == CONSTANT_SPACE_ID {
                    if let Some(target_idx) =
                        resolve_branch_target_index(pcode, &addr_to_idx, block_idx, op, target_vn)
                    {
                        block_idx = target_idx;
                        op_idx = 0;
                        continue;
                    }
                    return Err(PcodeEvalError::UnresolvedBranch(target_vn.offset));
                }
                return Err(PcodeEvalError::UnresolvedBranch(target_vn.offset));
            }
            PcodeOpcode::CBranch => {
                if op.inputs.len() < 2 {
                    return Err(PcodeEvalError::InvalidOperandCount(PcodeOpcode::CBranch));
                }
                let cond = read_vn(&op.inputs[1], &regs, &uniqs)?;
                if (cond & 1) != 0 {
                    let target_vn = &op.inputs[0];
                    let target_idx =
                        resolve_branch_target_index(pcode, &addr_to_idx, block_idx, op, target_vn)
                            .ok_or(PcodeEvalError::UnresolvedBranch(target_vn.offset))?;
                    block_idx = target_idx;
                    op_idx = 0;
                    continue;
                }
                op_idx += 1;
            }
            PcodeOpcode::Copy => {
                let a = read_vn(
                    op.inputs
                        .first()
                        .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Copy))?,
                    &regs,
                    &uniqs,
                )?;
                if let Some(out) = &op.output {
                    write_vn(out, a, &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::Load => {
                if op.inputs.len() < 2 {
                    return Err(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Load));
                }
                let addr = read_vn(&op.inputs[1], &regs, &uniqs)?;
                let out = op
                    .output
                    .as_ref()
                    .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Load))?;
                let mut loaded = 0u64;
                for i in 0..u64::from(out.size.clamp(1, 8)) {
                    let byte = ram
                        .get(&addr.saturating_add(i))
                        .copied()
                        .ok_or_else(|| PcodeEvalError::OutOfMemoryRead(addr.saturating_add(i)))?;
                    loaded |= u64::from(byte) << (i * 8);
                }
                write_vn(out, loaded, &mut regs, &mut uniqs)?;
                op_idx += 1;
            }
            PcodeOpcode::Store => {
                if op.inputs.len() < 3 {
                    return Err(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Store));
                }
                let addr = read_vn(&op.inputs[1], &regs, &uniqs)?;
                let val_vn = &op.inputs[2];
                let val = read_vn(val_vn, &regs, &uniqs)?;
                let n = val_vn.size.clamp(1, 8);
                for i in 0..u64::from(n) {
                    ram.insert(addr.saturating_add(i), ((val >> (i * 8)) & 0xff) as u8);
                }
                op_idx += 1;
            }
            PcodeOpcode::IntZExt => {
                let in0 = op
                    .inputs
                    .first()
                    .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::IntZExt))?;
                let a = read_vn(in0, &regs, &uniqs)?;
                if let Some(out) = &op.output {
                    write_vn(out, truncate_u64(a, in0.size), &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::IntSExt => {
                let in0 = op
                    .inputs
                    .first()
                    .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::IntSExt))?;
                let a = read_vn(in0, &regs, &uniqs)?;
                let extended = sign_extend_u64(a, in0.size) as u64;
                if let Some(out) = &op.output {
                    write_vn(out, truncate_u64(extended, out.size), &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::SubPiece => {
                if op.inputs.len() < 2 {
                    return Err(PcodeEvalError::InvalidOperandCount(PcodeOpcode::SubPiece));
                }
                let a = read_vn(&op.inputs[0], &regs, &uniqs)?;
                let trunc_bytes = read_vn(&op.inputs[1], &regs, &uniqs)?;
                let shift = trunc_bytes.saturating_mul(8);
                let val = if shift >= 64 { 0 } else { a >> shift };
                if let Some(out) = &op.output {
                    write_vn(out, val, &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::Piece => {
                if op.inputs.len() < 2 {
                    return Err(PcodeEvalError::InvalidOperandCount(PcodeOpcode::Piece));
                }
                let hi = read_vn(&op.inputs[0], &regs, &uniqs)?;
                let lo = read_vn(&op.inputs[1], &regs, &uniqs)?;
                let lo_bits = op.inputs[1].size.saturating_mul(8).min(63);
                let val = (hi << lo_bits) | truncate_u64(lo, op.inputs[1].size);
                if let Some(out) = &op.output {
                    write_vn(out, val, &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::BoolNegate | PcodeOpcode::IntNegate | PcodeOpcode::Int2Comp => {
                let in0 = op
                    .inputs
                    .first()
                    .ok_or(PcodeEvalError::InvalidOperandCount(op.opcode))?;
                let a = read_vn(in0, &regs, &uniqs)?;
                let res = match op.opcode {
                    PcodeOpcode::BoolNegate => u64::from((a & 1) == 0),
                    PcodeOpcode::IntNegate => !a,
                    PcodeOpcode::Int2Comp => (0u64).wrapping_sub(a),
                    _ => unreachable!(),
                };
                if let Some(out) = &op.output {
                    write_vn(out, res, &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow => {
                if op.inputs.len() < 2 {
                    return Err(PcodeEvalError::InvalidOperandCount(op.opcode));
                }
                let in0 = &op.inputs[0];
                let in1 = &op.inputs[1];
                let a = read_vn(in0, &regs, &uniqs)?;
                let b = read_vn(in1, &regs, &uniqs)?;
                let sz = in0.size.max(1);
                let sa = sign_extend_u64(a, in0.size);
                let sb = sign_extend_u64(b, in1.size);
                let res = match op.opcode {
                    PcodeOpcode::IntAdd => a.wrapping_add(b),
                    PcodeOpcode::IntSub => a.wrapping_sub(b),
                    PcodeOpcode::IntMult => a.wrapping_mul(b),
                    PcodeOpcode::IntDiv => {
                        if b == 0 {
                            return Err(PcodeEvalError::UnboundedDivideByZero);
                        }
                        a / b
                    }
                    PcodeOpcode::IntRem => {
                        if b == 0 {
                            return Err(PcodeEvalError::UnboundedDivideByZero);
                        }
                        a % b
                    }
                    PcodeOpcode::IntSDiv => {
                        if sb == 0 {
                            return Err(PcodeEvalError::UnboundedDivideByZero);
                        }
                        sa.wrapping_div(sb) as u64
                    }
                    PcodeOpcode::IntSRem => {
                        if sb == 0 {
                            return Err(PcodeEvalError::UnboundedDivideByZero);
                        }
                        sa.wrapping_rem(sb) as u64
                    }
                    PcodeOpcode::IntAnd => a & b,
                    PcodeOpcode::IntOr => a | b,
                    PcodeOpcode::IntXor => a ^ b,
                    PcodeOpcode::IntLeft => {
                        let bits = sz.saturating_mul(8);
                        if b >= u64::from(bits) {
                            0
                        } else {
                            a << (b as u32)
                        }
                    }
                    PcodeOpcode::IntRight => {
                        let bits = sz.saturating_mul(8);
                        if b >= u64::from(bits) {
                            0
                        } else {
                            a >> (b as u32)
                        }
                    }
                    PcodeOpcode::IntSRight => {
                        let bits = sz.saturating_mul(8);
                        if b >= u64::from(bits) {
                            if sa < 0 { !0u64 } else { 0 }
                        } else {
                            (sa >> (b as u32)) as u64
                        }
                    }
                    PcodeOpcode::IntEqual => u64::from(a == b),
                    PcodeOpcode::IntNotEqual => u64::from(a != b),
                    PcodeOpcode::IntLess => u64::from(a < b),
                    PcodeOpcode::IntLessEqual => u64::from(a <= b),
                    PcodeOpcode::IntSLess => u64::from(sa < sb),
                    PcodeOpcode::IntSLessEqual => u64::from(sa <= sb),
                    PcodeOpcode::BoolAnd => u64::from(((a & 1) != 0) && ((b & 1) != 0)),
                    PcodeOpcode::BoolOr => u64::from(((a & 1) != 0) || ((b & 1) != 0)),
                    PcodeOpcode::BoolXor => u64::from(((a ^ b) & 1) != 0),
                    PcodeOpcode::IntCarry => {
                        let sum = u128::from(a) + u128::from(b);
                        let bits = sz.saturating_mul(8).min(64);
                        u64::from((sum >> bits) != 0)
                    }
                    PcodeOpcode::IntSCarry => {
                        let bits = sz.saturating_mul(8).min(64);
                        let mask = if bits == 64 {
                            u64::MAX
                        } else {
                            (1u64 << bits) - 1
                        };
                        let sign_bit = 1u64 << (bits - 1);
                        let s = a.wrapping_add(b) & mask;
                        u64::from(((a ^ b) & sign_bit) == 0 && ((a ^ s) & sign_bit) != 0)
                    }
                    PcodeOpcode::IntSBorrow => {
                        let bits = sz.saturating_mul(8).min(64);
                        let mask = if bits == 64 {
                            u64::MAX
                        } else {
                            (1u64 << bits) - 1
                        };
                        let sign_bit = 1u64 << (bits - 1);
                        let d = a.wrapping_sub(b) & mask;
                        u64::from(((a ^ b) & sign_bit) != 0 && ((a ^ d) & sign_bit) != 0)
                    }
                    _ => unreachable!(),
                };
                if let Some(out) = &op.output {
                    write_vn(out, res, &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::PopCount => {
                let a = read_vn(
                    op.inputs
                        .first()
                        .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::PopCount))?,
                    &regs,
                    &uniqs,
                )?;
                if let Some(out) = &op.output {
                    write_vn(out, u64::from(a.count_ones()), &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            PcodeOpcode::LzCount => {
                let in0 = op
                    .inputs
                    .first()
                    .ok_or(PcodeEvalError::InvalidOperandCount(PcodeOpcode::LzCount))?;
                let a = read_vn(in0, &regs, &uniqs)?;
                let bits = in0.size.saturating_mul(8).clamp(1, 64);
                let top_aligned = a << (64 - bits);
                let lz = if a == 0 {
                    bits
                } else {
                    top_aligned.leading_zeros()
                };
                if let Some(out) = &op.output {
                    write_vn(out, u64::from(lz), &mut regs, &mut uniqs)?;
                }
                op_idx += 1;
            }
            other => return Err(PcodeEvalError::UnsupportedOpcode(other)),
        }
    }
}

/// Specifies how a [`MetamorphicSample`] binds to registers and `HirFunction`
/// parameters for an IR-level metamorphic test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrInputAbi {
    /// Two 32-bit unsigned scalar parameters `(RDI, RSI)`.
    TwoScalars,
    /// Pointer to initialized 4-byte scratch buffer + 32-bit unsigned integer `(RDI=scratch, RSI=arg0)`.
    BufferAndScalar,
}

impl IrInputAbi {
    fn reg_inits(self, sample: &MetamorphicSample) -> Vec<(u64, u32, u64)> {
        match self {
            // SysV64 RDI = 0x38, RSI = 0x30
            Self::TwoScalars => vec![
                (0x38, 8, u64::from(sample.arg0)),
                (0x30, 8, u64::from(sample.arg1)),
            ],
            Self::BufferAndScalar => vec![
                (0x38, 8, IR_SCRATCH_ADDR),
                (0x30, 8, u64::from(sample.arg0)),
            ],
        }
    }

    fn hir_arg_words(self, sample: &MetamorphicSample) -> [u64; 2] {
        match self {
            Self::TwoScalars => [u64::from(sample.arg0), u64::from(sample.arg1)],
            Self::BufferAndScalar => [IR_SCRATCH_ADDR, u64::from(sample.arg0)],
        }
    }
}

fn eval_decompiled_ir(
    pcode: &PcodeFunction,
    name: &str,
    options: &MlilPreviewOptions,
    abi: IrInputAbi,
    samples: &[MetamorphicSample],
) -> std::result::Result<Vec<SemanticObservation>, RelationOutcome> {
    let entry_addr = pcode
        .blocks
        .first()
        .map(|b| b.start_address)
        .unwrap_or(0x1000);
    let output = render_nir_with_context_output(pcode, name, entry_addr, options, None, None)
        .map_err(|err| RelationOutcome::Unsupported {
            reason: format!("decompilation failed for {name}: {err}"),
        })?;
    let hir = output
        .printed_hir
        .ok_or_else(|| RelationOutcome::Unsupported {
            reason: format!("missing printed HIR snapshot for {name}"),
        })?;

    if hir.params.len() != 2 {
        return Err(RelationOutcome::Unsupported {
            reason: format!(
                "{name}: recovered {} params (expected 2) in IR harness",
                hir.params.len()
            ),
        });
    }

    let mut obs = Vec::with_capacity(samples.len());
    for sample in samples {
        let raw_args = abi.hir_arg_words(sample);
        let hir_args: Vec<i64> = raw_args
            .iter()
            .zip(&hir.params)
            .map(|(&w, p)| normalize(w as i64, &p.ty))
            .collect();
        let image: Vec<(u64, u8)> = sample
            .initial_buffer
            .iter()
            .enumerate()
            .map(|(i, &b)| (IR_SCRATCH_ADDR + i as u64, b))
            .collect();
        let run = run_hir(
            &hir.body,
            &hir.params,
            &hir.locals,
            &hir_args,
            &World {
                image: &image,
                ..World::default()
            },
        )
        .map_err(|err| RelationOutcome::Unsupported {
            reason: format!("HIR evaluation failed for {name}: {err}"),
        })?;
        let ret = run.ret.ok_or_else(|| RelationOutcome::Unsupported {
            reason: format!("HIR body for {name} ended without Return"),
        })?;
        let mut buf = sample.initial_buffer;
        for (i, slot) in buf.iter_mut().enumerate() {
            if let Some(&b) = run.writes.get(&(IR_SCRATCH_ADDR + i as u64)) {
                *slot = b;
            }
        }
        obs.push(SemanticObservation {
            return_value: ret as u32,
            buffer: buf,
        });
    }
    Ok(obs)
}

/// Verifies an IR-level `(orig, variant)` pair by:
/// 1. Independently running [`eval_pcode_tiny_model`] on both `orig` and
///    `variant` over all `samples` (`Unsupported` errors never become passes);
/// 2. Decompiling both via Fission's `render_nir_with_context_output` and
///    evaluating both HIR outputs via [`run_hir`];
/// 3. Asserting semantic observation agreement across all lanes
///    (`tiny_orig == tiny_variant == hir_orig == hir_variant`) or returning
///    [`RelationOutcome::Diverged`] with concrete witnesses.
pub fn verify_ir_metamorphic_pair(
    name: &str,
    orig: &PcodeFunction,
    variant: &PcodeFunction,
    options: &MlilPreviewOptions,
    abi: IrInputAbi,
    samples: &[MetamorphicSample],
    expect_equivalent: bool,
) -> RelationOutcome {
    let mut tiny_orig = Vec::with_capacity(samples.len());
    let mut tiny_variant = Vec::with_capacity(samples.len());

    for s in samples {
        let reg_inits = abi.reg_inits(s);
        let ram_init: Vec<(u64, u8)> = s
            .initial_buffer
            .iter()
            .enumerate()
            .map(|(i, &b)| (IR_SCRATCH_ADDR + i as u64, b))
            .collect();
        match eval_pcode_tiny_model(orig, &reg_inits, &ram_init, IR_SCRATCH_ADDR) {
            Ok(o) => tiny_orig.push(o),
            Err(e) => {
                return RelationOutcome::Unsupported {
                    reason: format!("tiny model failed on orig {name}: {e}"),
                };
            }
        }
        match eval_pcode_tiny_model(variant, &reg_inits, &ram_init, IR_SCRATCH_ADDR) {
            Ok(o) => tiny_variant.push(o),
            Err(e) => {
                return RelationOutcome::Unsupported {
                    reason: format!("tiny model failed on variant {name}: {e}"),
                };
            }
        }
    }

    let hir_orig = match eval_decompiled_ir(orig, name, options, abi, samples) {
        Ok(v) => v,
        Err(outcome) => return outcome,
    };
    let hir_variant = match eval_decompiled_ir(variant, name, options, abi, samples) {
        Ok(v) => v,
        Err(outcome) => return outcome,
    };

    let mut witnesses = Vec::new();
    for (idx, s) in samples.iter().enumerate() {
        let to = &tiny_orig[idx];
        let tv = &tiny_variant[idx];
        let ho = &hir_orig[idx];
        let hv = &hir_variant[idx];

        let mut mismatch = Vec::new();
        if expect_equivalent && to != tv {
            mismatch.push(format!("tiny_orig={to:?} != tiny_variant={tv:?}"));
        }
        if to != ho {
            mismatch.push(format!("tiny_orig={to:?} != hir_orig={ho:?}"));
        }
        if tv != hv {
            mismatch.push(format!("tiny_variant={tv:?} != hir_variant={hv:?}"));
        }
        if ho != hv {
            mismatch.push(format!("hir_orig={ho:?} != hir_variant={hv:?}"));
        }
        if !mismatch.is_empty() {
            witnesses.push(MetamorphicWitness {
                sample_index: idx,
                sample: s.clone(),
                detail: mismatch.join("; "),
            });
        }
    }

    if witnesses.is_empty() {
        RelationOutcome::Passed {
            checked: samples.len(),
        }
    } else {
        RelationOutcome::Diverged { witnesses }
    }
}

/// Computes lowercase hex SHA-256 (FIPS 180-4) using pure safe Rust without extra crates.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(((data.len() + 9) / 64 + 1) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut hex = String::with_capacity(64);
    for word in h {
        let _ = write!(hex, "{word:08x}");
    }
    hex
}

/// Computes lowercase SHA-256 hex digest of a file on disk.
pub fn sha256_file(path: &std::path::Path) -> Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read {} for SHA-256", path.display()))?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_nist_abc_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn precondition_rejects_address_sensitive_layout_shift() {
        assert!(
            check_binary_relation_precondition(
                RelationKind::BinaryOptLevel,
                &FixtureProperties::ADDRESS_SENSITIVE,
            )
            .is_ok()
        );
        assert!(
            check_binary_relation_precondition(
                RelationKind::BinaryLayoutShift,
                &FixtureProperties::ADDRESS_SENSITIVE,
            )
            .is_err()
        );
        assert!(
            check_binary_relation_precondition(
                RelationKind::BinaryOptLevel,
                &FixtureProperties {
                    defined_unsigned_only: false,
                    bounded_loops: true,
                    address_sensitive: false,
                },
            )
            .is_err()
        );
    }
}
