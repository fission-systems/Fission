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
    pub(crate) cfg_facts: crate::nir::structuring::CfgFactCache,
    pub(crate) dom_tree: crate::nir::structuring::DomTree,
    pub(crate) irreducible_edges: HashSet<(usize, usize)>,
    /// For virtual blocks created by node-splitting: maps virtual_idx → original pcode block idx.
    /// Empty when no splitting has been applied (all blocks are genuine pcode blocks).
    pub(crate) virtual_block_map: Vec<usize>,
    pub(crate) loop_bodies: Vec<crate::nir::structuring::loop_analysis::LoopBody>,
    pub(crate) params: BTreeMap<usize, NirBinding>,
    pub(crate) locals: BTreeMap<i64, StackSlot>,
    pub(crate) locals_next_id: StackSlotId,
    pub(crate) temps: BTreeMap<String, NirBinding>,
    pub(crate) temp_next_id: u32,
    pub(crate) materialized_vns: HashMap<MaterializedVarnodeKey, String>,
    pub(crate) load_address_bindings: HashSet<String>,
    pub(crate) load_value_bindings: HashSet<String>,
    pub(crate) explicit_merge_bindings: HashMap<(usize, VarnodeKey), String>,
    pub(crate) call_result_bindings: HashMap<LoweringSite, String>,
    pub(crate) selector_representatives: BuilderCacheMap<(usize, u64, u64), HirExpr>,
    pub(crate) current_lowering_site: Option<LoweringSite>,
    pub(crate) register_param_aliases: HashMap<u64, usize>,
    pub(crate) entry_arity: usize,
    pub(crate) suppress_entry_register_params: bool,
    pub(crate) stack_frame_size: i64,
    pub(crate) linear_exit_cache: BuilderCacheMap<usize, Option<LinearExit>>,
    pub(crate) linear_body_cache: BuilderCacheMap<LinearBodyCacheKey, LinearBodyCachedOutcome>,
    pub(crate) active_linear_body_keys: BuilderCacheSet<LinearBodyCacheKey>,
    pub(crate) active_conditional_tail_keys: BuilderCacheSet<ConditionalTailKey>,
    pub(crate) jump_targets_cache: Option<HashSet<u64>>,
    pub(crate) active_trace_id: Option<u64>,
    pub(crate) last_trace_id: Option<u64>,
    pub(crate) next_trace_id: u64,
    pub(crate) lowering_site_depth: usize,
    pub(in crate::nir::builder) materialize_owner_repartition:
        RefCell<super::materialize::MaterializeOwnerRepartition>,
    pub(crate) current_stack_home_ptr: Option<Varnode>,
    pub(crate) active_switch_targets: HashSet<usize>,
    pub(crate) telemetry: super::telemetry::BuilderTelemetry,
    pub(crate) structuring_start: Option<std::time::Instant>,
    /// FAS-computed back-edges to virtualize as gotos when node-splitting is
    /// over budget. These edges are skipped during normal block terminator
    /// lowering and emitted as explicit `HirStmt::Goto` stmts instead.
    pub(crate) fas_virtual_edges: Vec<(usize, usize)>,
    pub(crate) lowered_block_stmts_cache: BuilderCacheMap<usize, Vec<HirStmt>>,
    pub(crate) partial_gpr_live_binding_cache: BuilderCacheMap<usize, bool>,
    pub(crate) follow_blocks: Vec<Option<usize>>,
    pub(crate) failed_loop_subgraphs: HashSet<(usize, usize)>,
    pub(crate) lower_varnode_cache: BuilderCacheMap<(Option<LoweringSite>, VarnodeKey), HirExpr>,
    pub(crate) structured_body: Option<Vec<HirStmt>>,
}
