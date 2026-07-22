use super::*;
use fission_loader::loader::LoadedBinary;
use std::cell::RefCell;

pub(super) type BuilderCacheMap<K, V> = rustc_hash::FxHashMap<K, V>;
pub(super) type BuilderCacheSet<T> = rustc_hash::FxHashSet<T>;

#[derive(Debug)]
pub(crate) struct PreviewBuilder<'a> {
    pub(crate) pcode: &'a PcodeFunction,
    pub(crate) options: &'a MlilPreviewOptions,
    pub(crate) binary: Option<&'a LoadedBinary>,
    pub(crate) type_context: Option<&'a PreviewTypeContext>,
    pub(crate) current_function_name: Option<String>,
    pub(crate) defs: HashMap<VarnodeKey, DefSite<'a>>,
    pub(crate) def_sites: HashMap<VarnodeKey, Vec<DefSite<'a>>>,
    pub(crate) block_defs: Vec<HashMap<VarnodeKey, Vec<usize>>>,
    pub(crate) lookup_site_cache:
        RefCell<BuilderCacheMap<(Option<LoweringSite>, VarnodeKey), Option<LoweringSite>>>,
    pub(crate) peel_cache: RefCell<BuilderCacheMap<(Option<LoweringSite>, VarnodeKey), Varnode>>,
    pub(crate) terminator_cache: BuilderCacheMap<usize, LoweredTerminator>,
    pub(crate) x86_branch_recovery_attempts: usize,
    pub(crate) address_to_index: HashMap<u64, usize>,
    pub(crate) block_target_keys: Vec<u64>,
    pub(crate) target_key_to_index: HashMap<u64, usize>,
    pub(crate) layout_fallthrough: Vec<Option<usize>>,
    pub(crate) successors: Vec<Vec<usize>>,
    pub(crate) predecessors: Vec<Vec<usize>>,
    pub(crate) reachability_cache: RefCell<BuilderCacheMap<(usize, usize, usize), bool>>,
    pub(crate) cfg_facts: crate::midend::structuring::CfgFactCache,
    pub(crate) dom_tree: crate::midend::structuring::DomTree,
    pub(crate) irreducible_edges: HashSet<(usize, usize)>,
    /// For virtual blocks created by node-splitting: maps virtual_idx → original pcode block idx.
    /// Empty when no splitting has been applied (all blocks are genuine pcode blocks).
    pub(crate) virtual_block_map: Vec<usize>,
    pub(crate) loop_bodies: Vec<crate::midend::structuring::loop_analysis::LoopBody>,
    pub(crate) params: BTreeMap<usize, DirBinding>,
    pub(crate) locals: BTreeMap<i64, StackSlot>,
    pub(crate) locals_next_id: StackSlotId,
    pub(crate) temps: BTreeMap<String, DirBinding>,
    pub(crate) temp_next_id: u32,
    pub(crate) materialized_vns: HashMap<MaterializedVarnodeKey, String>,
    pub(crate) load_address_bindings: HashSet<String>,
    pub(crate) load_value_bindings: HashSet<String>,
    pub(crate) explicit_merge_bindings: HashMap<(usize, VarnodeKey), String>,
    pub(crate) call_result_bindings: HashMap<LoweringSite, String>,
    pub(crate) selector_representatives: BuilderCacheMap<(usize, u64, u64), DirExpr>,
    pub(crate) current_lowering_site: Option<LoweringSite>,
    pub(crate) register_param_aliases: HashMap<u64, usize>,
    pub(crate) entry_arity: usize,
    pub(crate) suppress_entry_register_params: bool,
    pub(crate) stack_frame_size: i64,
    pub(crate) entry_frame_pointer_established: bool,
    /// Constant `K` when the prologue establishes `rbp`/`ebp` via
    /// `lea rbp, [rsp+K]` (`K != 0`) rather than plain `mov rbp, rsp`
    /// (`K == 0`, the default). See `entry_analysis::infer_entry_stack_
    /// layout` and its use in `resolve_stack_address_inner`.
    pub(crate) rbp_frame_bias: i64,
    pub(crate) linear_exit_cache: BuilderCacheMap<usize, Option<LinearExit>>,
    pub(crate) linear_body_cache: BuilderCacheMap<LinearBodyCacheKey, LinearBodyCachedOutcome>,
    pub(crate) active_linear_body_keys: BuilderCacheSet<LinearBodyCacheKey>,
    pub(crate) active_conditional_tail_keys: BuilderCacheSet<ConditionalTailKey>,
    pub(crate) jump_targets_cache: Option<HashSet<u64>>,
    pub(crate) active_trace_id: Option<u64>,
    pub(crate) last_trace_id: Option<u64>,
    pub(crate) next_trace_id: u64,
    pub(crate) lowering_site_depth: usize,
    pub(in crate::midend::builder) materialize_owner_repartition:
        RefCell<super::materialize::MaterializeOwnerRepartition>,
    pub(crate) current_stack_home_ptr: Option<Varnode>,
    pub(crate) active_switch_targets: HashSet<usize>,
    pub(crate) telemetry: super::telemetry::BuilderTelemetry,
    pub(crate) structuring_start: Option<std::time::Instant>,
    /// FAS-computed back-edges to virtualize as gotos when node-splitting is
    /// over budget. These edges are skipped during normal block terminator
    /// lowering and emitted as explicit `DirStmt::Goto` stmts instead.
    pub(crate) fas_virtual_edges: Vec<(usize, usize)>,
    pub(crate) lowered_block_stmts_cache: BuilderCacheMap<usize, Vec<DirStmt>>,
    pub(crate) partial_gpr_live_binding_cache: BuilderCacheMap<usize, bool>,
    pub(crate) follow_blocks: Vec<Option<usize>>,
    pub(crate) failed_loop_subgraphs: HashSet<(usize, usize)>,
    pub(crate) lower_varnode_cache: BuilderCacheMap<(Option<LoweringSite>, VarnodeKey), DirExpr>,
    pub(crate) structured_body: Option<Vec<DirStmt>>,
    /// `options` is fixed for the builder's lifetime, so the `RegisterNamer`
    /// it derives from (including a `sla_map` clone) only needs building
    /// once instead of on every `register_namer()` call — that call is on
    /// the hot varnode-lowering path (per-op, potentially per candidate
    /// region-proof attempt during structuring).
    pub(crate) register_namer_cache: std::cell::OnceCell<crate::midend::cspec::RegisterNamer>,
    /// Deterministic proxy for `SESE_REGION_PROOF_BUDGET_MS`: counts
    /// `sese_region_proof_budget_exceeded()` calls since the last
    /// `reset_sese_region_proof_budget()` instead of measuring wall-clock
    /// elapsed time, so region-proof completion (and thus decompiled
    /// output) no longer depends on machine speed / load. See PROJECT.md.
    pub(crate) sese_region_proof_calls: std::cell::Cell<u64>,
    /// Memoizes `prove_loop_carried_register_update`: the pcode/loop_bodies
    /// this proof walks (a `VecDeque`-based BFS over the whole loop body,
    /// per call) are fixed for the builder's lifetime, but the SESE region
    /// search re-lowers overlapping candidate ranges many times, so without
    /// caching the same (block, op, varnode) triple gets re-proven
    /// repeatedly. Keyed by (block_idx, op_idx, output key).
    pub(in crate::midend::builder) loop_carried_proof_cache: RefCell<
        BuilderCacheMap<
            (usize, usize, VarnodeKey),
            Option<super::materialize::LoopCarriedDefinitionProof>,
        >,
    >,
    /// Deterministic proxy for the old shared-5000ms `IfLoweringBudget`
    /// "total structuring" check and the matching inline wall-clock checks
    /// in `loops.rs` (`try_lower_while`, `try_lower_multiblock_dowhile`,
    /// `lower_loop_body_subgraph`). `Rc` so `IfLoweringBudget` instances
    /// (which don't hold a `&host` reference) can share a live handle. See
    /// PROJECT.md.
    pub(crate) structuring_total_work_units: std::rc::Rc<std::cell::Cell<u64>>,
}
