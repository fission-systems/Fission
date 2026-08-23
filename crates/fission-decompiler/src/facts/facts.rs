use crate::decode_rust_sleigh_pcode;
use crate::pipeline::rust_sleigh::apply_spec_overrides;
use crate::{
    CallEdgeKind, CallEffectSummarySource, CallTargetProvenance, CallTargetRef,
    NirCallEffectSummary, NirCallParamRule, NirCallPointerPointee, NirCallPrototypeSummary,
    NirFunctionHints, NirStructFieldHint, NirStructTypeHint, NirType, NirTypeContext,
    PcodeFunction, PcodeOpcode, RegisterNamer, infer_entry_register_param_arity,
};
use fission_analysis_db::SymbolKind;
use fission_core::PATHS;
use fission_core::core::ghidra_no_return::{
    binary_format_to_ghidra_format, ghidra_no_return_index,
};
use fission_core::{normalize_named_type_identity, sanitize_symbol_name};
use fission_loader::loader::LoadedBinary;
use fission_loader::loader::types::DwarfLocation;
use fission_signatures::golang_typeinfo::GoTypeinfoDatabase;
use fission_signatures::win_types::WindowsStructures;
use fission_signatures::{
    SIGNATURE_RESOURCES, pointer_surface_type_name_is_specific, symbol_for_win_api_database_lookup,
};
use fission_static::analysis::decomp::facts::FactProvenance;
use fission_static::analysis::decomp::facts::FactStore;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

fn get_well_known_function_hints(name: &str) -> Option<NirFunctionHints> {
    let lower = name.to_ascii_lowercase();
    if lower == "main" || lower == "wmain" || lower == "winmain" || lower == "wwinmain" {
        return None;
    }
    // The database is a map; scanning all 115,900 signatures to find one by
    // name was doing a lookup the long way, once per function.
    let matched_sig = SIGNATURE_RESOURCES.api_signature(name)?;

    let mut param_names = Vec::new();
    let mut param_type_names = HashMap::new();
    for (index, param) in matched_sig.params.iter().enumerate() {
        param_names.push(param.name.clone());
        param_type_names.insert(index, param.type_name.clone());
    }

    Some(NirFunctionHints {
        param_names,
        param_type_names,
        stack_local_names: HashMap::new(),
        stack_local_type_names: HashMap::new(),
        return_type_name: Some(matched_sig.return_type.clone()),
        register_local_names: HashMap::new(),
        register_local_type_names: HashMap::new(),
    })
}

fn get_go_function_hints(name: &str, binary: &LoadedBinary) -> Option<NirFunctionHints> {
    let go_ver = binary.go_version.as_deref()?;
    let typeinfo_dir = PATHS.get_golang_typeinfo_dir()?;
    let goos = GoTypeinfoDatabase::goos_from_format(&binary.format);
    let goarch = GoTypeinfoDatabase::goarch_from_spec(binary.is_64bit, &binary.arch_spec);
    let db = GoTypeinfoDatabase::get_cached(go_ver, goos, goarch, &typeinfo_dir)?;
    let sig = db.get_func(name)?;

    let mut param_names = Vec::new();
    let mut param_type_names = HashMap::new();
    for (index, (pname, ptype)) in sig.params.iter().enumerate() {
        param_names.push(pname.clone());
        param_type_names.insert(index, ptype.clone());
    }
    let return_type_name = sig.results.first().map(|(_, t)| t.clone());

    Some(NirFunctionHints {
        param_names,
        param_type_names,
        stack_local_names: HashMap::new(),
        stack_local_type_names: HashMap::new(),
        return_type_name,
        register_local_names: HashMap::new(),
        register_local_type_names: HashMap::new(),
    })
}

/// Per-binary memo for the parts of `NirTypeContext` that do not depend on the
/// function being decompiled.
///
/// `build_nir_type_context` takes an address, but `all_target_refs` is derived
/// from the whole program, so `build_nir_call_param_rules` produced identical
/// output for every function in a binary and was recomputed for each one. It
/// walks all 151,408 signatures and every one of their 835,245 parameters to
/// arrive at ~716 rules -- 4.5ms per function, on a corpus run once per row.
///
/// Keyed by the binary's Blake3 hash, so two different binaries in one process
/// do not share an entry and the same binary re-decompiled reuses one.
static BINARY_SCOPED_RULES: LazyLock<Mutex<HashMap<String, Arc<Vec<NirCallParamRule>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn call_param_rules_for_binary(
    binary: &LoadedBinary,
    all_target_refs: &HashMap<u64, CallTargetRef>,
) -> Arc<Vec<NirCallParamRule>> {
    let key = binary.hash.clone();
    // The lock is held across the build on purpose. Releasing it to compute and
    // re-taking it to store lets every thread that arrives first miss: on an
    // 79-function `--all` run, 14 workers each built the same 716 rules,
    // 45ms apiece, and the cache saved nothing. Holding it means one build and
    // the rest wait for it.
    let mut cache = match BINARY_SCOPED_RULES.lock() {
        Ok(cache) => cache,
        // A poisoned lock means another thread panicked mid-build. Fall back to
        // computing rather than propagating the panic.
        Err(_) => return Arc::new(build_nir_call_param_rules(all_target_refs)),
    };
    if let Some(hit) = cache.get(&key) {
        return Arc::clone(hit);
    }
    let rules = Arc::new(build_nir_call_param_rules(all_target_refs));
    cache.insert(key, Arc::clone(&rules));
    rules
}

pub(crate) fn build_nir_type_context(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> NirTypeContext {
    let mut index = CallTargetIndex::default();
    let mut iat_index = CallTargetIndex::default();
    let program = fact_store.program();

    for function in program
        .functions
        .iter()
        .filter(|function| function.is_import)
    {
        if function.entry == 0 || function.name.is_empty() {
            continue;
        }
        iat_index.add(function.entry, &function.name, CandidateClass::Import);
    }

    for symbol in program
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Import)
    {
        if symbol.address == 0 || symbol.name.is_empty() {
            continue;
        }
        iat_index.add(symbol.address, &symbol.name, CandidateClass::Import);
    }

    for (resolved_address, fact) in fact_store.iter_resolved_name_facts() {
        if matches!(fact.provenance, FactProvenance::ImportExport) {
            continue;
        }
        if fact.name.is_empty() {
            continue;
        }
        index.add(resolved_address, &fact.name, CandidateClass::Fact);
    }

    for function in &program.functions {
        if function.name.is_empty() {
            continue;
        }
        if function.is_import {
            continue;
        }
        let class = if function.is_export && function.is_thunk {
            CandidateClass::ExportThunk
        } else if function.is_export {
            CandidateClass::Export
        } else {
            CandidateClass::Direct
        };
        index.add(function.entry, &function.name, class);
        if function.is_export
            && function.is_thunk
            && let Some(thunk_target) = function.thunk_target
            && thunk_target != 0
        {
            index.add(
                thunk_target,
                &function.name,
                CandidateClass::ExportThunkTarget,
            );
        }
    }

    for symbol in program
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Data)
    {
        if symbol.name.is_empty() {
            continue;
        }
        index.add(symbol.address, &symbol.name, CandidateClass::Global);
    }
    let resolved_index = index.finish();
    let resolved_iat_index = iat_index.finish();
    let call_target_refs = resolved_index.call_target_refs;
    let iat_target_refs = resolved_iat_index.call_target_refs;
    let call_targets = call_target_refs
        .iter()
        .chain(iat_target_refs.iter())
        .map(|(address, target_ref)| (*address, target_ref.symbol.clone()))
        .collect::<HashMap<_, _>>();
    let all_target_refs = call_target_refs
        .iter()
        .chain(iat_target_refs.iter())
        .map(|(address, target_ref)| (*address, target_ref.clone()))
        .collect::<HashMap<_, _>>();

    let mut function_hints = build_nir_function_hints(binary, fact_store, address);
    if function_hints.is_none() {
        let name = all_target_refs
            .get(&address)
            .map(|tr| tr.symbol.as_str())
            .unwrap_or("");
        if let Some(well_known) = get_well_known_function_hints(name) {
            function_hints = Some(well_known);
        } else if let Some(go_hints) = get_go_function_hints(name, binary) {
            function_hints = Some(go_hints);
        }
    }

    NirTypeContext {
        call_targets,
        call_target_refs: call_target_refs.clone(),
        iat_target_refs: iat_target_refs.clone(),
        ambiguous_call_targets: resolved_index.ambiguous_call_targets,
        call_effect_summaries: build_nir_call_effect_summaries(&all_target_refs, binary),
        call_prototype_summaries: HashMap::new(),
        call_result_is_source_value: build_nir_call_result_facts(&all_target_refs),
        call_param_rules: call_param_rules_for_binary(binary, &all_target_refs)
            .as_ref()
            .clone(),
        function_hints,
        struct_types: build_nir_struct_type_hints(binary),
    }
}

/// Transport exact source-result facts from the signature owner to the
/// builder. ABI result registers remain machine clobbers for declared-void
/// calls; this fact only says that the clobber is not a source-language value.
fn build_nir_call_result_facts(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> HashMap<String, bool> {
    let mut facts = HashMap::new();
    for target_ref in call_target_refs.values() {
        let signature = SIGNATURE_RESOURCES
            .api_signature(&target_ref.symbol)
            .or_else(|| {
                symbol_for_win_api_database_lookup(&target_ref.symbol)
                    .and_then(|name| SIGNATURE_RESOURCES.api_signature(name))
            });
        let Some(signature) = signature else {
            continue;
        };
        if !signature.return_type.trim().eq_ignore_ascii_case("void") {
            continue;
        }
        facts.insert(target_ref.symbol.clone(), false);
    }
    facts
}

/// Struct/union/class layouts known from debug info (DWARF `DW_TAG_structure_
/// type`/`DW_TAG_union_type`/`DW_TAG_class_type`), keyed by type name.
///
/// Used only to overlay real field names onto heuristically-recovered
/// `NirType::Aggregate` fields in `type_hints.rs` -- see `NirStructTypeHint`.
fn build_nir_struct_type_hints(binary: &LoadedBinary) -> HashMap<String, NirStructTypeHint> {
    let mut struct_types = HashMap::default();
    for ty in &binary.inferred_types {
        if ty.name.is_empty() || ty.fields.is_empty() {
            continue;
        }
        struct_types
            .entry(ty.name.clone())
            .or_insert_with(|| NirStructTypeHint {
                name: ty.name.clone(),
                size: ty.size,
                fields: ty
                    .fields
                    .iter()
                    .map(|field| NirStructFieldHint {
                        name: field.name.clone(),
                        type_name: field.type_name.clone(),
                        offset: field.offset,
                        size: field.size,
                    })
                    .collect(),
            });
    }
    struct_types
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateClass {
    Import,
    Fact,
    ExportThunk,
    ExportThunkTarget,
    Export,
    Direct,
    Global,
}

impl CandidateClass {
    fn rank(self) -> u8 {
        match self {
            CandidateClass::Import => 7,
            CandidateClass::ExportThunk | CandidateClass::ExportThunkTarget => 6,
            CandidateClass::Export => 5,
            CandidateClass::Fact => 4,
            CandidateClass::Direct => 3,
            CandidateClass::Global => 2,
        }
    }

    fn provenance(self) -> CallTargetProvenance {
        match self {
            CandidateClass::Import => CallTargetProvenance::Import,
            CandidateClass::Fact => CallTargetProvenance::Fact,
            CandidateClass::ExportThunk => CallTargetProvenance::Export,
            CandidateClass::ExportThunkTarget => CallTargetProvenance::ExportThunkTarget,
            CandidateClass::Export => CallTargetProvenance::Export,
            CandidateClass::Direct => CallTargetProvenance::Direct,
            CandidateClass::Global => CallTargetProvenance::Global,
        }
    }

    fn edge_kind(self) -> CallEdgeKind {
        match self {
            CandidateClass::Import => CallEdgeKind::Import,
            CandidateClass::Direct => CallEdgeKind::Direct,
            _ => CallEdgeKind::Reference,
        }
    }

    fn confidence(self) -> u8 {
        match self {
            CandidateClass::Import | CandidateClass::Fact => 255,
            CandidateClass::ExportThunk | CandidateClass::ExportThunkTarget => 240,
            CandidateClass::Export => 232,
            CandidateClass::Direct => 224,
            CandidateClass::Global => 192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallTargetCandidate {
    symbol: String,
    class: CandidateClass,
}

#[derive(Debug, Default)]
struct CallTargetIndex {
    candidates: BTreeMap<u64, Vec<CallTargetCandidate>>,
}

#[derive(Debug, Default)]
struct ResolvedCallTargetIndex {
    call_target_refs: HashMap<u64, CallTargetRef>,
    ambiguous_call_targets: HashSet<u64>,
}

impl CallTargetIndex {
    fn add(&mut self, address: u64, name: &str, class: CandidateClass) {
        let sanitized = sanitize_nir_symbol_name(name);
        if sanitized.is_empty() || is_generic_loader_symbol(&sanitized) {
            return;
        }
        self.candidates
            .entry(address)
            .or_default()
            .push(CallTargetCandidate {
                symbol: sanitized,
                class,
            });
    }

    fn finish(self) -> ResolvedCallTargetIndex {
        let mut resolved = ResolvedCallTargetIndex::default();
        for (address, mut candidates) in self.candidates {
            candidates.sort_by(|left, right| {
                right
                    .class
                    .rank()
                    .cmp(&left.class.rank())
                    .then_with(|| left.symbol.cmp(&right.symbol))
            });
            let Some(best) = candidates.first() else {
                continue;
            };
            let best_rank = best.class.rank();
            let same_rank_symbols = candidates
                .iter()
                .filter(|candidate| candidate.class.rank() == best_rank)
                .map(|candidate| candidate.symbol.as_str())
                .collect::<BTreeSet<_>>();
            if same_rank_symbols.len() > 1 {
                resolved.ambiguous_call_targets.insert(address);
                continue;
            }
            resolved.call_target_refs.insert(
                address,
                CallTargetRef {
                    address: Some(address),
                    symbol: best.symbol.clone(),
                    provenance: best.class.provenance(),
                    edge_kind: best.class.edge_kind(),
                    confidence: best.class.confidence(),
                },
            );
        }
        resolved
    }
}

fn is_generic_loader_symbol(name: &str) -> bool {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    is_generic_symbol_with_prefix(stripped, "sub_")
        || is_generic_symbol_with_prefix(stripped, "FUN_0x")
        || is_generic_symbol_with_prefix(stripped, "FUN_")
        || is_generic_symbol_with_prefix(stripped, "ltmp")
        || is_generic_symbol_with_prefix(stripped, "tmp_")
}

fn is_generic_symbol_with_prefix(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn build_nir_call_effect_summaries(
    call_target_refs: &HashMap<u64, CallTargetRef>,
    binary: &LoadedBinary,
) -> HashMap<String, NirCallEffectSummary> {
    let ghidra_format = binary_format_to_ghidra_format(&binary.format);
    let compiler_key = ghidra_no_return_compiler_key(binary);
    let no_return_idx = ghidra_no_return_index();

    let mut result: HashMap<String, NirCallEffectSummary> = HashMap::new();

    for (&address, target_ref) in call_target_refs {
        let library_name: Option<&str> = binary
            .function_at_exact(address)
            .and_then(|f| f.external_library.as_deref());

        let may_exit = ghidra_format.and_then(|fmt| {
            if no_return_idx.is_no_return(fmt, compiler_key, library_name, &target_ref.symbol) {
                Some(true)
            } else {
                None
            }
        });

        let source = if may_exit.is_some() {
            Some(CallEffectSummarySource::GhidraNoReturnData)
        } else {
            Some(CallEffectSummarySource::CallTargetRef)
        };

        let entry = result
            .entry(target_ref.symbol.clone())
            .or_insert(NirCallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                may_call_unknown: None,
                may_exit,
                source,
            });
        // Upgrade to may_exit=true if a later address for the same symbol name provides evidence.
        if entry.may_exit.is_none() && may_exit.is_some() {
            entry.may_exit = may_exit;
            entry.source = source;
        }
    }

    result
}

fn ghidra_no_return_compiler_key(binary: &LoadedBinary) -> Option<&'static str> {
    let lang = binary
        .identity_report
        .as_ref()?
        .summary
        .likely_language
        .as_deref()?;
    match lang.to_ascii_lowercase().as_str() {
        "go" | "golang" => Some("golang"),
        "rust" => Some("rustc"),
        _ => None,
    }
}

/// Walk a raw (pre-normalize) PreHIR body and record, per callee name, the
/// highest `Call` argument count seen -- mirrors
/// `fission-midend-normalize`'s `apply_interproc_callsite_arity_pass`, but
/// deliberately re-implemented here rather than reused: that pass reads
/// `PreHirFunction::callee_observed_max_arity` *after* normalize's early
/// type-signature fixed point has already run
/// `apply_callsite_type_prop_pass`, which truncates each call's `args` down
/// to the callee's own body-inferred arity (`prune_known_api_call_args_stmts`
/// in `callsite_type_prop.rs`) -- so by the time it runs, a call site can
/// never be observed as wider than what the callee's own preview-inferred
/// signature already implied. Walking the *raw*, pre-normalize body (as
/// captured by `fission_pcode::take_last_raw_hir_snapshot`) reads the
/// builder's real, uncapped argument recovery instead.
fn collect_raw_call_arities(stmts: &[fission_pcode::PreHirStmt], out: &mut HashMap<String, usize>) {
    use fission_pcode::{PreHirExpr, PreHirStmt};

    fn visit_expr(expr: &PreHirExpr, out: &mut HashMap<String, usize>) {
        match expr {
            PreHirExpr::Call { target, args, .. } => {
                for arg in args {
                    visit_expr(arg, out);
                }
                out.entry(target.clone())
                    .and_modify(|max| *max = (*max).max(args.len()))
                    .or_insert(args.len());
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                visit_expr(lhs, out);
                visit_expr(rhs, out);
            }
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => visit_expr(expr, out),
            PreHirExpr::Index { base, index, .. } => {
                visit_expr(base, out);
                visit_expr(index, out);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                visit_expr(cond, out);
                visit_expr(then_expr, out);
                visit_expr(else_expr, out);
            }
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        }
    }

    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                visit_expr(rhs, out);
            }
            PreHirStmt::VaStart { va_list, .. } => visit_expr(va_list, out),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_raw_call_arities(body, out),
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                visit_expr(expr, out);
                for case in cases {
                    collect_raw_call_arities(&case.body, out);
                }
                collect_raw_call_arities(default, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                visit_expr(cond, out);
                collect_raw_call_arities(then_body, out);
                collect_raw_call_arities(else_body, out);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

/// Record real, observed call-site arity as a structuring hint for each
/// resolved callee of the function that was just built.
///
/// The highest argument count any real call site in this function passed
/// to a callee is a sound, structural fact. Feeding it forward into
/// `FactStore` lets a *later* decompile of that callee -- in this same
/// session -- widen its own rendered signature via
/// `ensure_missing_hinted_params` (`fission-pcode`'s `type_hints.rs`) when
/// the callee's own body doesn't clearly reveal all of its parameters (e.g.
/// a trailing register param it never reads). Recorded directly on the
/// `FactStore`, not via `DecompFacts::record_discovered_hints` -- that
/// trait method also flips `hints_changed` and rebuilds `type_context`
/// centered on whatever address is passed in, which is the right behavior
/// for the self-referential "rebuild the function I'm currently building"
/// round trip (see `render.rs`'s 3-round loop), not for recording facts
/// about a *different* function.
pub(crate) fn record_interprocedural_arity_facts(
    fact_store: &mut FactStore,
    type_context: &NirTypeContext,
    raw_hir: &fission_pcode::PreHirFunction,
    self_address: u64,
) {
    let mut observed_arity = HashMap::new();
    collect_raw_call_arities(&raw_hir.body, &mut observed_arity);
    if observed_arity.is_empty() {
        return;
    }
    // Bounded by this one function's own direct call targets, not the
    // whole binary's call_target index.
    let name_to_addr: HashMap<&str, u64> = type_context
        .call_targets
        .iter()
        .map(|(addr, name)| (name.as_str(), *addr))
        .collect();

    for (callee_name, arity) in observed_arity {
        if arity == 0 {
            continue; // uninformative / can't distinguish from "not a real call"
        }
        let Some(&callee_addr) = name_to_addr.get(callee_name.as_str()) else {
            continue; // unresolved symbol (external import, indirect, etc.)
        };
        if callee_addr == self_address {
            continue; // recursive call carries no new arity info
        }
        // `nir_function_hints_are_empty` (this file, below) treats an
        // all-blank `param_names` as "no real hint" and
        // `merge_nir_function_hints` discards it before it ever reaches
        // `ensure_missing_hinted_params` -- so a pure arity signal needs
        // *some* non-empty name per slot to survive that gate. Using the
        // same `param_{index+1}` default `ensure_missing_hinted_params`
        // itself falls back to (`type_hints.rs`) makes this a no-op rename
        // for any slot a function's own body already reveals (matches its
        // own default naming exactly) and only has visible effect on the
        // slots this hint is actually widening arity for.
        let hints = NirFunctionHints {
            param_names: (1..=arity).map(|i| format!("param_{i}")).collect(),
            ..Default::default()
        };
        fact_store.record_structuring_hints(callee_addr, hints);
    }
}

/// Whole-program call-arity pre-analysis: decode every non-import function in
/// the binary once, harvest each one's own real (pre-normalize) call-site
/// argument counts via [`collect_raw_call_arities`], and record every
/// resolved callee's observed arity as a structuring hint on `fact_store` --
/// before any per-request decompile has happened.
///
/// This is what lets a *first-ever* decompile of a callee in a fresh session
/// already show a widened signature, which
/// [`record_interprocedural_arity_facts`] alone cannot: that helper only
/// records what it observes *during* a real decompile, so it requires the
/// caller to have already been rendered earlier in the same session. Meant
/// to run once during background discovery (mirrors the whole-binary,
/// bounded-per-function decode shape already used by
/// `ingest_signature_matches_with_databases` for FID matching, and by Phase
/// A's no-return fixpoint), not per decompile request.
pub fn seed_whole_program_call_arity_facts(binary: &LoadedBinary, fact_store: &mut FactStore) {
    use rayon::prelude::*;

    let type_context = build_nir_type_context(binary, fact_store, 0);
    let mut options = crate::seed_nir_render_options(binary);
    apply_spec_overrides(binary, &mut options);

    let name_to_addr: HashMap<&str, u64> = type_context
        .call_targets
        .iter()
        .map(|(addr, name)| (name.as_str(), *addr))
        .collect();

    let observations: Vec<(u64, HashMap<String, usize>)> = binary
        .functions
        .par_iter()
        .filter(|f| !f.is_import && f.address != 0)
        .filter_map(|f| {
            let max_bytes = direct_callee_max_bytes(binary, f.address)?;
            let instruction_limit = direct_callee_instruction_limit(max_bytes);
            let pcode = decode_rust_sleigh_pcode(
                binary,
                &f.name,
                f.address,
                max_bytes,
                instruction_limit,
                true,
                true,
            )
            .ok()?;
            let raw_hir = fission_pcode::build_raw_hir(
                &pcode,
                &f.name,
                f.address,
                &options,
                Some(binary),
                Some(&type_context),
            )
            .ok()?;
            let mut arities = HashMap::new();
            collect_raw_call_arities(&raw_hir.body, &mut arities);
            (!arities.is_empty()).then_some((f.address, arities))
        })
        .collect();

    let mut merged: HashMap<u64, usize> = HashMap::new();
    for (caller_addr, arities) in &observations {
        for (callee_name, &arity) in arities {
            if arity == 0 {
                continue; // uninformative / can't distinguish from "not a real call"
            }
            let Some(&callee_addr) = name_to_addr.get(callee_name.as_str()) else {
                continue; // unresolved symbol (external import, indirect, etc.)
            };
            if callee_addr == *caller_addr {
                continue; // recursive call carries no new arity info
            }
            merged
                .entry(callee_addr)
                .and_modify(|max| *max = (*max).max(arity))
                .or_insert(arity);
        }
    }

    for (callee_addr, arity) in merged {
        // See `record_interprocedural_arity_facts` above for why `param_{i}`
        // placeholders (not empty names) are required here.
        let hints = NirFunctionHints {
            param_names: (1..=arity).map(|i| format!("param_{i}")).collect(),
            ..Default::default()
        };
        fact_store.record_structuring_hints(callee_addr, hints);
    }
}

pub(crate) fn refine_nir_type_context_with_callee_effect_summaries(
    binary: &LoadedBinary,
    pcode: &PcodeFunction,
    type_context: &mut NirTypeContext,
) {
    let direct_callees = collect_direct_internal_callee_targets(pcode);
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        let resolved_in_refs = direct_callees
            .iter()
            .filter(|a| type_context.call_target_refs.contains_key(a))
            .count();
        eprintln!(
            "[CALLEE-DIAG] fn_entry=0x{:x} blocks={} direct_callees={} resolved_in_refs={} call_target_refs_total={}",
            pcode.blocks.first().map(|b| b.start_address).unwrap_or(0),
            pcode.blocks.len(),
            direct_callees.len(),
            resolved_in_refs,
            type_context.call_target_refs.len(),
        );
    }
    for target_addr in direct_callees {
        if pcode
            .blocks
            .first()
            .is_some_and(|block| block.start_address == target_addr)
        {
            continue;
        }
        let has_resolved_target_identity = type_context.call_target_refs.contains_key(&target_addr);
        let target_ref = type_context
            .call_target_refs
            .get(&target_addr)
            .cloned()
            .unwrap_or_else(|| CallTargetRef {
                address: Some(target_addr),
                symbol: format!("sub_{target_addr:x}"),
                provenance: CallTargetProvenance::Direct,
                edge_kind: CallEdgeKind::Direct,
                confidence: 160,
            });
        if matches!(target_ref.provenance, CallTargetProvenance::Import) {
            continue;
        }
        let Some((effect_summary, prototype_summary)) =
            build_preview_callee_summaries(binary, target_addr, &target_ref.symbol, type_context)
        else {
            continue;
        };
        if has_resolved_target_identity {
            type_context
                .call_effect_summaries
                .insert(target_ref.symbol.clone(), effect_summary);
        }
        if let Some(mut prototype_summary) = prototype_summary {
            // A stripped `sub_<addr>` identity is sufficient to transport an
            // ABI-slot type fact back to the exact direct call, but it is not
            // a declaration-level proof that recovered extra arguments may be
            // deleted. Keep exact-arity pruning on the pre-existing resolved
            // symbol path only.
            if !has_resolved_target_identity {
                prototype_summary.min_arity = 0;
                prototype_summary.locked_exact_arity = None;
            }
            type_context
                .call_prototype_summaries
                .insert(target_ref.symbol.clone(), prototype_summary);
        }
    }
}

fn collect_direct_internal_callee_targets(pcode: &PcodeFunction) -> BTreeSet<u64> {
    let mut callees = BTreeSet::new();
    for block in &pcode.blocks {
        for op in &block.ops {
            if op.opcode != PcodeOpcode::Call {
                continue;
            }
            let Some(target) = op.inputs.first() else {
                continue;
            };
            callees.insert(target.offset);
        }
    }
    callees
}

fn build_preview_callee_summaries(
    binary: &LoadedBinary,
    target_addr: u64,
    target_name: &str,
    type_context: &NirTypeContext,
) -> Option<(NirCallEffectSummary, Option<NirCallPrototypeSummary>)> {
    let function = binary.function_at_exact(target_addr)?;
    if function.is_import {
        return None;
    }
    let max_bytes = direct_callee_max_bytes(binary, target_addr)?;
    let instruction_limit = direct_callee_instruction_limit(max_bytes);
    let next_function = binary.function_after(target_addr).map(|func| func.address);
    let pcode = decode_rust_sleigh_pcode(
        binary,
        target_name,
        target_addr,
        max_bytes,
        instruction_limit,
        true,
        true,
    )
    .ok()?;
    let (summary, detail) = summarize_preview_callee_effects(&pcode);
    trace_preview_callee_effect_detail(
        target_name,
        target_addr,
        function.size,
        max_bytes,
        instruction_limit,
        next_function,
        &pcode,
        &detail,
    );
    let mut options = crate::seed_nir_render_options(binary);
    apply_spec_overrides(binary, &mut options);
    let register_namer = RegisterNamer::from_options(&options);
    // Full raw-HIR construction and normalization is deliberately bounded by
    // the callee's decoded semantic size. Effect summarization above remains
    // linear and useful for larger callees, while typed previews beyond this
    // boundary have a sharply different downstream cost profile (large nested
    // regions, type fixed points, and cleanup), so they are not a safe fact
    // source for an interactive caller decompilation.
    const MAX_TYPED_PREVIEW_PCODE_OPS: usize = 2_000;
    let prototype = (detail.op_count <= MAX_TYPED_PREVIEW_PCODE_OPS)
        .then(|| infer_entry_register_param_arity(&pcode, &register_namer))
        .flatten()
        .map(|arity| {
            let mut callee_context = type_context.clone();
            callee_context.function_hints = Some(NirFunctionHints {
                param_names: (1..=arity).map(|index| format!("param_{index}")).collect(),
                ..Default::default()
            });
            callee_context.call_prototype_summaries.remove(target_name);

            let typed_params = fission_pcode::build_raw_hir(
                &pcode,
                target_name,
                target_addr,
                &options,
                Some(binary),
                Some(&callee_context),
            )
            .ok()
            .map(|mut callee| {
                fission_midend_normalize::normalize_hir_function(&mut callee);
                callee.params
            })
            .unwrap_or_default();

            let mut param_pointer_pointees = vec![None; arity];
            let mut param_surface_type_names = vec![None; arity];
            for (index, param) in typed_params.into_iter().take(arity).enumerate() {
                let surface = param
                    .surface_type_name
                    .filter(|name| pointer_surface_type_name_is_specific(name));
                let pointee = match param.ty {
                    NirType::Ptr(inner) => match *inner {
                        NirType::Int { bits, signed } => {
                            Some(NirCallPointerPointee::Int { bits, signed })
                        }
                        _ if surface.is_some() => Some(NirCallPointerPointee::Unknown),
                        _ => None,
                    },
                    _ if surface.is_some() => Some(NirCallPointerPointee::Unknown),
                    _ => None,
                };
                if pointee.is_some() {
                    param_pointer_pointees[index] = pointee;
                    param_surface_type_names[index] = surface;
                }
            }

            if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
                eprintln!(
                    "[CALLEE-TYPE-DIAG] target={} arity={} pointer_pointees={:?} surface_types={:?}",
                    target_name, arity, param_pointer_pointees, param_surface_type_names
                );
            }

            NirCallPrototypeSummary {
                min_arity: arity,
                max_arity: arity,
                locked_exact_arity: Some(arity),
                param_pointer_pointees,
                param_surface_type_names,
            }
        });
    Some((summary, prototype))
}

fn direct_callee_max_bytes(binary: &LoadedBinary, target_addr: u64) -> Option<usize> {
    let function = binary.function_at_exact(target_addr)?;
    const DEFAULT_BYTES: usize = 0x400;
    const MAX_BYTES_CAP: usize = 0x4000;

    if function.size > 0 {
        return Some((function.size as usize).min(MAX_BYTES_CAP).max(1));
    }

    if let Some(next) = binary.function_after(target_addr)
        && next.address > target_addr
    {
        let distance = (next.address - target_addr) as usize;
        return Some(distance.min(MAX_BYTES_CAP).max(1));
    }

    Some(DEFAULT_BYTES)
}

fn direct_callee_instruction_limit(max_bytes: usize) -> usize {
    let estimated = (max_bytes / 4).clamp(32, 512);
    estimated.max(32)
}

#[derive(Debug, Clone, Default)]
struct PreviewCalleeEffectDetail {
    store_count: usize,
    call_count: usize,
    callind_count: usize,
    callother_count: usize,
    return_count: usize,
    block_count: usize,
    op_count: usize,
    first_store: Option<(u64, PcodeOpcode)>,
    first_call: Option<(u64, Option<u64>, PcodeOpcode)>,
    first_callother: Option<(u64, PcodeOpcode)>,
    first_return: Option<(u64, PcodeOpcode)>,
    last_op_addr: Option<u64>,
    has_fallthrough_past_return: bool,
    is_single_call_return_wrapper: bool,
}

fn summarize_preview_callee_effects(
    pcode: &PcodeFunction,
) -> (NirCallEffectSummary, PreviewCalleeEffectDetail) {
    let mut reads_memory = Some(false);
    let mut writes_memory = Some(false);
    let mut may_call_unknown = Some(false);
    let mut may_exit = None;
    let mut saw_return = false;
    let mut detail = PreviewCalleeEffectDetail {
        block_count: pcode.blocks.len(),
        ..PreviewCalleeEffectDetail::default()
    };

    for block in &pcode.blocks {
        for op in &block.ops {
            detail.op_count += 1;
            detail.last_op_addr = Some(op.address);
            match op.opcode {
                PcodeOpcode::Load => {
                    reads_memory = Some(true);
                }
                PcodeOpcode::Store => {
                    reads_memory = Some(true);
                    writes_memory = Some(true);
                    detail.store_count += 1;
                    detail.first_store.get_or_insert((op.address, op.opcode));
                }
                PcodeOpcode::Call => {
                    may_call_unknown = Some(true);
                    detail.call_count += 1;
                    detail.first_call.get_or_insert((
                        op.address,
                        op.inputs
                            .first()
                            .and_then(|input| input.is_constant.then_some(input.offset)),
                        op.opcode,
                    ));
                }
                PcodeOpcode::CallInd => {
                    may_call_unknown = Some(true);
                    detail.callind_count += 1;
                    detail
                        .first_call
                        .get_or_insert((op.address, None, op.opcode));
                }
                PcodeOpcode::CallOther => {
                    may_call_unknown = Some(true);
                    may_exit = Some(true);
                    detail.callother_count += 1;
                    detail
                        .first_callother
                        .get_or_insert((op.address, op.opcode));
                }
                PcodeOpcode::Return => {
                    saw_return = true;
                    detail.return_count += 1;
                    detail.first_return.get_or_insert((op.address, op.opcode));
                }
                _ => {}
            }
        }
    }

    if let (Some((return_addr, _)), Some(last_op_addr)) = (detail.first_return, detail.last_op_addr)
    {
        detail.has_fallthrough_past_return = last_op_addr > return_addr;
    }
    detail.is_single_call_return_wrapper = detail.store_count == 0
        && detail.callother_count == 0
        && detail.callind_count == 0
        && detail.call_count == 1
        && detail.return_count == 1
        && detail.op_count <= 3;

    if may_exit != Some(true) {
        may_exit = if saw_return && may_call_unknown == Some(false) {
            Some(false)
        } else {
            None
        };
    }

    (
        NirCallEffectSummary {
            reads_memory,
            writes_memory,
            escapes_args: None,
            may_call_unknown,
            may_exit,
            source: Some(CallEffectSummarySource::PreviewCalleeAnalysis),
        },
        detail,
    )
}

fn trace_preview_callee_effect_detail(
    target_name: &str,
    target_addr: u64,
    function_size: u64,
    max_bytes: usize,
    instruction_limit: usize,
    next_function: Option<u64>,
    pcode: &PcodeFunction,
    detail: &PreviewCalleeEffectDetail,
) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_none() {
        return;
    }
    eprintln!(
        "[GT-TRACE] callee-lift-bounds target={} start=0x{:x} max_bytes={} instruction_limit={} function_size={} next_function={:?}",
        target_name,
        target_addr,
        max_bytes,
        instruction_limit,
        function_size,
        next_function.map(|addr| format!("0x{:x}", addr))
    );
    eprintln!(
        "[GT-TRACE] callee-effect-detail target={} target_addr=0x{:x} store_count={} call_count={} callind_count={} callother_count={} return_count={}",
        target_name,
        target_addr,
        detail.store_count,
        detail.call_count,
        detail.callind_count,
        detail.callother_count,
        detail.return_count
    );
    eprintln!(
        "[GT-TRACE] callee-shape target={} block_count={} op_count={} return_count={} has_fallthrough_past_return={} single_call_return_wrapper={}",
        target_name,
        detail.block_count,
        detail.op_count,
        detail.return_count,
        detail.has_fallthrough_past_return,
        detail.is_single_call_return_wrapper
    );
    if let Some((address, opcode)) = detail.first_store {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-store target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind=Store addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
    if let Some((address, call_target, opcode)) = detail.first_call {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-call target={} addr=0x{:x} call_target={:?} op={:?}",
            target_name, address, call_target, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind={:?} addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            opcode,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
    if let Some((address, opcode)) = detail.first_callother {
        let within_function =
            addr_within_function_bounds(address, target_addr, function_size, next_function);
        eprintln!(
            "[GT-TRACE] callee-effect-first-callother target={} addr=0x{:x} op={:?}",
            target_name, address, opcode
        );
        eprintln!(
            "[GT-TRACE] callee-effect-first-op-detail target={} kind=CallOther addr=0x{:x} within_function={} block_count={} op_count={}",
            target_name,
            address,
            within_function,
            pcode.blocks.len(),
            detail.op_count
        );
    }
}

fn addr_within_function_bounds(
    address: u64,
    start_addr: u64,
    function_size: u64,
    next_function: Option<u64>,
) -> bool {
    if function_size > 0 {
        return address >= start_addr && address < start_addr.saturating_add(function_size);
    }
    if let Some(next_addr) = next_function {
        return address >= start_addr && address < next_addr;
    }
    address >= start_addr
}

fn build_nir_function_hints(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> Option<NirFunctionHints> {
    let debug_hints = fact_store
        .preferred_debug_function(address)
        .and_then(|debug| nir_hints_from_debug_function(debug, binary));
    merge_nir_function_hints(debug_hints, fact_store.structuring_hints(address))
}

/// Resolve register-resident DWARF locals (`DW_OP_reg*` for the whole
/// declared scope, or a location list where every range agrees on the same
/// register -- see `DwarfAnalyzer::parse_location_list`) to the underlying
/// SLEIGH register *offset*, e.g. DWARF `reg5` on x86-64 -> Ghidra `RDI` ->
/// offset via the checked-in `.slaspec` register model (the size half of
/// `RegisterModel::lookup_name`'s result is dropped: `DW_OP_reg*` doesn't
/// carry a width, and a binding's access width can legitimately differ
/// slightly from the declared variable's type without meaning it's an
/// unrelated value -- see `NirFunctionHints::register_local_names`).
/// Deliberately keyed by offset rather than by name: most register-resident
/// values get a generic `uVarN`/`iVarN` display name during materialization
/// rather than their raw hardware register name, so matching by name alone
/// would miss most real cases (`type_hints.rs`'s `apply_function_name_hints`
/// matches this against each binding's actual `register_origins` entry --
/// its *real* originating register, independent of whatever name
/// materialization gave it).
///
/// Returns an empty map on any missing piece (no load spec, no `.dwarf` file
/// for this architecture, no register-resident locals) -- best-effort, same
/// posture as the rest of this file's debug-info handling.
fn record_unambiguous_register_type_hint(
    types: &mut HashMap<u64, String>,
    conflicts: &mut HashSet<u64>,
    offset: u64,
    type_name: &str,
) {
    let type_name = type_name.trim();
    if type_name.is_empty() || conflicts.contains(&offset) {
        return;
    }
    match types.get(&offset) {
        None => {
            types.insert(offset, type_name.to_string());
        }
        Some(existing) if existing == type_name => {}
        Some(_) => {
            types.remove(&offset);
            conflicts.insert(offset);
        }
    }
}

fn register_local_hints_from_debug_function(
    debug: &fission_loader::loader::types::DwarfFunctionInfo,
    binary: &LoadedBinary,
) -> (HashMap<u64, String>, HashMap<u64, String>) {
    let mut names = HashMap::new();
    let mut types = HashMap::new();
    let mut type_conflicts = HashSet::new();
    let register_locals = debug
        .local_vars
        .iter()
        .filter(|local| {
            matches!(local.location, DwarfLocation::Register(_))
                && (!local.name.trim().is_empty() || !local.type_name.trim().is_empty())
        })
        .collect::<Vec<_>>();
    if register_locals.is_empty() {
        return (names, types);
    }
    let Some(load_spec) = binary.load_spec() else {
        return (names, types);
    };
    let language_id = load_spec.pair.language_id.as_str();
    let compiler_spec_id = load_spec.pair.compiler_spec_id.as_str();

    let languages_root = fission_sleigh::compiler::sleigh_languages_root();
    let Some(dwarf_map) = fission_pcode::midend::cspec::dwarf_regs::load_dwarf_regs_for_pair(
        &languages_root,
        language_id,
        compiler_spec_id,
    ) else {
        return (names, types);
    };
    let Some(model) = fission_pcode::midend::cspec::register_model_for_language(language_id) else {
        return (names, types);
    };

    for local in register_locals {
        let DwarfLocation::Register(reg_str) = &local.location else {
            continue;
        };
        let Some(dwarf_num) = reg_str
            .strip_prefix("reg")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(ghidra_name) = dwarf_map.ghidra_name_for(dwarf_num) else {
            continue;
        };
        let Some((offset, _size)) = model.lookup_name(ghidra_name) else {
            continue;
        };
        let name = local.name.trim();
        if !name.is_empty() {
            names.entry(offset).or_insert_with(|| name.to_string());
        }

        record_unambiguous_register_type_hint(
            &mut types,
            &mut type_conflicts,
            offset,
            &local.type_name,
        );
    }
    (names, types)
}

fn nir_hints_from_debug_function(
    debug: &fission_loader::loader::types::DwarfFunctionInfo,
    binary: &LoadedBinary,
) -> Option<NirFunctionHints> {
    let param_names = debug
        .params
        .iter()
        .map(|param| param.name.trim().to_string())
        .collect::<Vec<_>>();
    let param_type_names = debug
        .params
        .iter()
        .enumerate()
        .filter_map(|(index, param)| {
            let type_name = param.type_name.trim();
            (!type_name.is_empty()).then(|| (index, type_name.to_string()))
        })
        .collect::<HashMap<_, _>>();
    let stack_local_names = debug
        .local_vars
        .iter()
        .filter_map(|local| match local.location {
            DwarfLocation::StackOffset(offset) if !local.name.trim().is_empty() => {
                Some((offset, local.name.trim().to_string()))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let stack_local_type_names = debug
        .local_vars
        .iter()
        .filter_map(|local| match local.location {
            DwarfLocation::StackOffset(offset) => {
                let type_name = local.type_name.trim();
                (!type_name.is_empty()).then(|| (offset, type_name.to_string()))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let return_type_name = debug
        .return_type
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned);
    let (register_local_names, register_local_type_names) =
        register_local_hints_from_debug_function(debug, binary);

    if param_names.is_empty()
        && param_type_names.is_empty()
        && stack_local_names.is_empty()
        && stack_local_type_names.is_empty()
        && return_type_name.is_none()
        && register_local_names.is_empty()
        && register_local_type_names.is_empty()
    {
        None
    } else {
        Some(NirFunctionHints {
            param_names,
            param_type_names,
            stack_local_names,
            stack_local_type_names,
            return_type_name,
            register_local_names,
            register_local_type_names,
        })
    }
}

fn merge_nir_function_hints(
    debug: Option<NirFunctionHints>,
    structural: Option<&NirFunctionHints>,
) -> Option<NirFunctionHints> {
    let mut merged = debug.unwrap_or_default();
    let Some(structural) = structural else {
        return (!nir_function_hints_are_empty(&merged)).then_some(merged);
    };

    if merged.param_names.len() < structural.param_names.len() {
        merged
            .param_names
            .resize(structural.param_names.len(), String::new());
    }
    for (index, name) in structural.param_names.iter().enumerate() {
        if merged.param_names[index].is_empty() && !name.is_empty() {
            merged.param_names[index] = name.clone();
        }
    }
    for (index, type_name) in &structural.param_type_names {
        merged
            .param_type_names
            .entry(*index)
            .or_insert_with(|| type_name.clone());
    }
    for (offset, name) in &structural.stack_local_names {
        merged
            .stack_local_names
            .entry(*offset)
            .or_insert_with(|| name.clone());
    }
    for (offset, type_name) in &structural.stack_local_type_names {
        merged
            .stack_local_type_names
            .entry(*offset)
            .or_insert_with(|| type_name.clone());
    }
    if merged.return_type_name.is_none() {
        merged
            .return_type_name
            .clone_from(&structural.return_type_name);
    }
    for (reg_name, var_name) in &structural.register_local_names {
        merged
            .register_local_names
            .entry(reg_name.clone())
            .or_insert_with(|| var_name.clone());
    }
    for (reg_name, type_name) in &structural.register_local_type_names {
        merged
            .register_local_type_names
            .entry(*reg_name)
            .or_insert_with(|| type_name.clone());
    }

    (!nir_function_hints_are_empty(&merged)).then_some(merged)
}

fn nir_function_hints_are_empty(hints: &NirFunctionHints) -> bool {
    hints.param_names.iter().all(String::is_empty)
        && hints.param_type_names.is_empty()
        && hints.stack_local_names.is_empty()
        && hints.stack_local_type_names.is_empty()
        && hints.return_type_name.is_none()
        && hints.register_local_names.is_empty()
        && hints.register_local_type_names.is_empty()
}

pub(crate) fn sanitize_nir_symbol_name(name: &str) -> String {
    sanitize_symbol_name(name)
}

/// The rules that depend only on the signature tables and struct layouts.
///
/// Both inputs are static, so this is the same list in every process and for
/// every binary; `callee_address` is the only part that varies, and it is
/// attached in [`build_nir_call_param_rules`] from the binary's own call
/// targets. Exported by `bin/export_call_param_rules` so the runtime can load
/// ~716 rules instead of walking 151,408 signatures to rediscover them.
pub fn name_keyed_call_param_rules() -> Vec<NirCallParamRule> {
    let mut rules = Vec::new();
    let Ok(structures) = WindowsStructures::try_new() else {
        return rules;
    };
    let Ok(signatures) = SIGNATURE_RESOURCES.api_signatures() else {
        return rules;
    };
    for sig in signatures {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_nir_struct_name(&param.type_name, &structures) else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            rules.push(NirCallParamRule {
                callee_address: None,
                callee_name: sig.name.clone(),
                arg_index,
                pointer_alias: param.type_name.clone(),
                pointee_alias: struct_name,
                pointer_size: 8,
                pointee_sizes: vec![struct_def.size_64 as u32],
            });
        }
    }
    rules.sort_by(|a, b| {
        a.callee_name
            .cmp(&b.callee_name)
            .then_with(|| a.arg_index.cmp(&b.arg_index))
    });
    rules
}

/// The precomputed rules, or `None` when the bundle does not carry them.
///
/// Falling back to computing them keeps a checkout without the file working;
/// what the file buys is not having to read 151,408 signatures to rediscover
/// 716 rules.
static PRECOMPUTED_RULES: LazyLock<Option<Vec<NirCallParamRule>>> = LazyLock::new(|| {
    let path = fission_core::resources::ResourceProvider::global()
        .win32_typeinfo_json_path("call_param_rules.json")?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
});

fn build_nir_call_param_rules(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> Vec<NirCallParamRule> {
    if let Some(precomputed) = PRECOMPUTED_RULES.as_ref() {
        return attach_addresses(precomputed, call_target_refs);
    }
    build_nir_call_param_rules_from_signatures(call_target_refs)
}

/// Expand name-keyed rules into per-address ones for this binary.
///
/// A rule for a callee the binary never calls is kept with no address, exactly
/// as the signature walk produced it: `collect_call_hints_from_expr` matches on
/// `callee_name` as well as address, so a call rendered by bare name still
/// finds its rule.
fn attach_addresses(
    rules: &[NirCallParamRule],
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> Vec<NirCallParamRule> {
    let mut addresses_by_name: HashMap<&str, Vec<u64>> = HashMap::new();
    for (address, target_ref) in call_target_refs {
        addresses_by_name
            .entry(target_ref.symbol.as_str())
            .or_default()
            .push(*address);
    }
    let mut out = Vec::with_capacity(rules.len());
    for rule in rules {
        match addresses_by_name.get(rule.callee_name.as_str()) {
            Some(addresses) if !addresses.is_empty() => {
                for address in addresses {
                    let mut with_address = rule.clone();
                    with_address.callee_address = Some(*address);
                    out.push(with_address);
                }
            }
            _ => out.push(rule.clone()),
        }
    }
    out
}

fn build_nir_call_param_rules_from_signatures(
    call_target_refs: &HashMap<u64, CallTargetRef>,
) -> Vec<NirCallParamRule> {
    let mut call_param_rules = Vec::new();
    let Ok(structures) = WindowsStructures::try_new() else {
        return call_param_rules;
    };
    let target_addresses_by_name = call_target_refs.iter().fold(
        HashMap::<String, Vec<u64>>::new(),
        |mut acc, (addr, target_ref)| {
            acc.entry(target_ref.symbol.clone())
                .or_default()
                .push(*addr);
            acc
        },
    );
    let Ok(signatures) = SIGNATURE_RESOURCES.api_signatures() else {
        return call_param_rules;
    };
    for sig in signatures {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_nir_struct_name(&param.type_name, &structures) else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            let addresses = target_addresses_by_name
                .get(&sig.name)
                .cloned()
                .unwrap_or_default();
            if addresses.is_empty() {
                call_param_rules.push(NirCallParamRule {
                    callee_address: None,
                    callee_name: sig.name.clone(),
                    arg_index,
                    pointer_alias: param.type_name.clone(),
                    pointee_alias: struct_name.clone(),
                    pointer_size: 8,
                    pointee_sizes: vec![struct_def.size_64 as u32],
                });
            } else {
                for address in addresses {
                    call_param_rules.push(NirCallParamRule {
                        callee_address: Some(address),
                        callee_name: sig.name.clone(),
                        arg_index,
                        pointer_alias: param.type_name.clone(),
                        pointee_alias: struct_name.clone(),
                        pointer_size: 8,
                        pointee_sizes: vec![struct_def.size_64 as u32],
                    });
                }
            }
        }
    }
    call_param_rules
}

fn resolve_nir_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }
    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        let Some(candidate) = normalize_named_type_identity(candidate) else {
            continue;
        };
        if structures.get(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        build_nir_call_param_rules, build_nir_call_result_facts, merge_nir_function_hints,
        record_interprocedural_arity_facts, record_unambiguous_register_type_hint,
        resolve_nir_struct_name, summarize_preview_callee_effects,
    };
    use crate::{
        CallEdgeKind, CallTargetProvenance, CallTargetRef, NirTypeContext, PcodeBasicBlock,
        PcodeFunction, PcodeOp, PcodeOpcode, Varnode,
    };
    use fission_signatures::win_types::WindowsStructures;
    use fission_static::analysis::decomp::facts::FactStore;
    use std::collections::{HashMap, HashSet};

    fn type_context_with_call_target(addr: u64, name: &str) -> NirTypeContext {
        let mut ctx = NirTypeContext::default();
        ctx.call_targets.insert(addr, name.to_string());
        ctx
    }

    #[test]
    fn exact_void_signature_marks_abi_result_as_non_source() {
        let call_targets = HashMap::from([(
            0x401000,
            CallTargetRef {
                address: Some(0x401000),
                symbol: "free".to_string(),
                provenance: CallTargetProvenance::Import,
                edge_kind: CallEdgeKind::Direct,
                confidence: 100,
            },
        )]);

        let facts = build_nir_call_result_facts(&call_targets);
        assert_eq!(facts.get("free"), Some(&false));
    }

    fn raw_hir_calling(callee_name: &str, arg_count: usize) -> fission_pcode::PreHirFunction {
        use fission_pcode::{NirType, PreHirExpr, PreHirStmt};
        let args = (0..arg_count)
            .map(|i| PreHirExpr::Const(i as i64, NirType::Unknown))
            .collect();
        fission_pcode::PreHirFunction {
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: callee_name.to_string(),
                args,
                ty: NirType::Unknown,
            })],
            ..Default::default()
        }
    }

    #[test]
    fn records_observed_arity_for_resolved_callee() {
        let mut facts = FactStore::default();
        let ctx = type_context_with_call_target(0x401000, "target_fn");
        let raw_hir = raw_hir_calling("target_fn", 3);

        record_interprocedural_arity_facts(&mut facts, &ctx, &raw_hir, 0x400000);

        let hints = facts.structuring_hints(0x401000).expect("hints recorded");
        assert_eq!(hints.param_names.len(), 3);
    }

    #[test]
    fn skips_zero_arity() {
        let mut facts = FactStore::default();
        let ctx = type_context_with_call_target(0x401000, "target_fn");
        let raw_hir = raw_hir_calling("target_fn", 0);

        record_interprocedural_arity_facts(&mut facts, &ctx, &raw_hir, 0x400000);

        assert!(facts.structuring_hints(0x401000).is_none());
    }

    #[test]
    fn skips_unresolved_callee_name() {
        let mut facts = FactStore::default();
        let ctx = type_context_with_call_target(0x401000, "target_fn");
        let raw_hir = raw_hir_calling("some_other_fn", 2);

        record_interprocedural_arity_facts(&mut facts, &ctx, &raw_hir, 0x400000);

        assert!(facts.structuring_hints(0x401000).is_none());
    }

    #[test]
    fn skips_self_recursive_call() {
        let mut facts = FactStore::default();
        // The function being built (0x400000) calls itself.
        let ctx = type_context_with_call_target(0x400000, "self_fn");
        let raw_hir = raw_hir_calling("self_fn", 4);

        record_interprocedural_arity_facts(&mut facts, &ctx, &raw_hir, 0x400000);

        assert!(facts.structuring_hints(0x400000).is_none());
    }

    #[test]
    fn debug_hints_keep_precedence_over_structuring_overlay() {
        let debug = crate::NirFunctionHints {
            param_names: vec!["debug_name".into()],
            param_type_names: HashMap::from([(0, "int".into())]),
            register_local_type_names: HashMap::from([(0x20, "long".into())]),
            return_type_name: Some("int".into()),
            ..Default::default()
        };
        let structural = crate::NirFunctionHints {
            param_names: vec!["structural_name".into(), "second".into()],
            param_type_names: HashMap::from([(0, "uint32_t".into()), (1, "char *".into())]),
            register_local_type_names: HashMap::from([
                (0x20, "unsigned long".into()),
                (0x28, "size_t".into()),
            ]),
            return_type_name: Some("uint32_t".into()),
            ..Default::default()
        };

        let merged = merge_nir_function_hints(Some(debug), Some(&structural)).unwrap();
        assert_eq!(merged.param_names, ["debug_name", "second"]);
        assert_eq!(merged.param_type_names[&0], "int");
        assert_eq!(merged.param_type_names[&1], "char *");
        assert_eq!(merged.register_local_type_names[&0x20], "long");
        assert_eq!(merged.register_local_type_names[&0x28], "size_t");
        assert_eq!(merged.return_type_name.as_deref(), Some("int"));
    }

    #[test]
    fn conflicting_register_local_types_are_rejected_for_the_whole_offset() {
        let mut types = HashMap::new();
        let mut conflicts = HashSet::new();

        record_unambiguous_register_type_hint(&mut types, &mut conflicts, 0x20, "int");
        record_unambiguous_register_type_hint(&mut types, &mut conflicts, 0x20, "int");
        assert_eq!(types.get(&0x20).map(String::as_str), Some("int"));

        record_unambiguous_register_type_hint(&mut types, &mut conflicts, 0x20, "char *");
        record_unambiguous_register_type_hint(&mut types, &mut conflicts, 0x20, "int");
        assert!(!types.contains_key(&0x20));
        assert!(conflicts.contains(&0x20));

        record_unambiguous_register_type_hint(&mut types, &mut conflicts, 0x28, "size_t");
        assert_eq!(types.get(&0x28).map(String::as_str), Some("size_t"));
    }

    // -------------------------------------------------------------------------
    // GDT pattern matching: resolve_nir_struct_name
    // -------------------------------------------------------------------------

    #[test]
    fn gdt_pattern_match_lp_prefix_resolves_to_struct() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PSECURITY_DESCRIPTOR", &ws),
            Some("SECURITY_DESCRIPTOR".to_string()),
            "PSECURITY_DESCRIPTOR -> SECURITY_DESCRIPTOR via P-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_p_prefix_resolves_sid() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PSID", &ws),
            Some("SID".to_string()),
            "PSID -> SID via P-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_lp_prefix_resolves_critical_section() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("LPCRITICAL_SECTION", &ws),
            Some("CRITICAL_SECTION".to_string()),
            "LPCRITICAL_SECTION -> CRITICAL_SECTION via LP-prefix strip"
        );
    }

    #[test]
    fn gdt_pattern_match_pointer_star_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("SECURITY_DESCRIPTOR*", &ws),
            None,
            "pointer-star types must be rejected"
        );
    }

    #[test]
    fn gdt_pattern_match_bare_type_no_prefix_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("UINT", &ws),
            None,
            "bare scalar type without LP/P prefix must not match"
        );
    }

    #[test]
    fn gdt_pattern_match_p_prefix_unknown_struct_returns_none() {
        let ws =
            WindowsStructures::try_new().expect("structures.json must be loadable from workspace");
        assert_eq!(
            resolve_nir_struct_name("PVOID", &ws),
            None,
            "PVOID -> VOID is not a known struct"
        );
    }

    // -------------------------------------------------------------------------
    // GDT pattern matching: build_nir_call_param_rules (end-to-end)
    // -------------------------------------------------------------------------

    fn make_call_target_refs(entries: &[(u64, &str)]) -> HashMap<u64, CallTargetRef> {
        entries
            .iter()
            .map(|&(addr, name)| {
                (
                    addr,
                    CallTargetRef {
                        address: Some(addr),
                        symbol: name.to_string(),
                        provenance: CallTargetProvenance::Import,
                        edge_kind: CallEdgeKind::Import,
                        confidence: 255,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn gdt_call_param_rules_generated_for_known_win32_api() {
        let refs = make_call_target_refs(&[(0x1000, "AccessCheckAndAuditAlarmA")]);
        let rules = build_nir_call_param_rules(&refs);
        assert!(
            !rules.is_empty(),
            "expected at least one NirCallParamRule for AccessCheckAndAuditAlarmA \
             (has PSECURITY_DESCRIPTOR param)"
        );
        let sd_rule = rules.iter().find(|r| {
            r.callee_name == "AccessCheckAndAuditAlarmA" && r.pointee_alias == "SECURITY_DESCRIPTOR"
        });
        assert!(
            sd_rule.is_some(),
            "expected a rule for SECURITY_DESCRIPTOR param of AccessCheckAndAuditAlarmA"
        );
    }

    #[test]
    fn gdt_call_param_rules_empty_for_unknown_function() {
        let refs = make_call_target_refs(&[(0x2000, "NonExistentFunctionXYZ")]);
        let rules = build_nir_call_param_rules(&refs);
        let matched: Vec<_> = rules
            .iter()
            .filter(|r| r.callee_name == "NonExistentFunctionXYZ")
            .collect();
        assert!(
            matched.is_empty(),
            "unknown function must produce no param rules"
        );
    }

    #[test]
    fn gdt_call_param_rules_no_address_when_not_in_refs() {
        let refs = HashMap::new();
        let rules = build_nir_call_param_rules(&refs);
        let no_addr_rules: Vec<_> = rules
            .iter()
            .filter(|r| r.callee_address.is_none())
            .collect();
        assert!(
            !no_addr_rules.is_empty(),
            "rules without a resolved address must still be emitted for signature-only coverage"
        );
    }

    fn op(seq_num: u32, opcode: PcodeOpcode) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x401000 + seq_num as u64,
            output: None,
            inputs: Vec::new(),
            asm_mnemonic: None,
        }
    }

    fn constant_call_op(seq_num: u32, target: u64) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode: PcodeOpcode::Call,
            address: 0x401000 + seq_num as u64,
            output: None,
            inputs: vec![Varnode::constant(target as i64, 8)],
            asm_mnemonic: None,
        }
    }

    fn test_pcode(ops: Vec<PcodeOp>) -> PcodeFunction {
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x401000,
                successors: Vec::new(),
                ops,
            }],
        }
    }

    #[test]
    fn preview_callee_effect_summary_marks_leaf_return_as_non_exiting() {
        let pcode = test_pcode(vec![op(0, PcodeOpcode::Copy), op(1, PcodeOpcode::Return)]);
        let (summary, detail) = summarize_preview_callee_effects(&pcode);
        assert_eq!(summary.reads_memory, Some(false));
        assert_eq!(summary.writes_memory, Some(false));
        assert_eq!(summary.may_call_unknown, Some(false));
        assert_eq!(summary.may_exit, Some(false));
        assert_eq!(detail.return_count, 1);
    }

    #[test]
    fn preview_callee_effect_summary_marks_store_and_nested_call_as_unsafe() {
        let pcode = test_pcode(vec![
            op(0, PcodeOpcode::Load),
            op(1, PcodeOpcode::Store),
            constant_call_op(2, 0x500000),
            op(3, PcodeOpcode::Return),
        ]);
        let (summary, detail) = summarize_preview_callee_effects(&pcode);
        assert_eq!(summary.reads_memory, Some(true));
        assert_eq!(summary.writes_memory, Some(true));
        assert_eq!(summary.may_call_unknown, Some(true));
        assert_eq!(summary.may_exit, None);
        assert_eq!(detail.store_count, 1);
        assert_eq!(detail.call_count, 1);
        assert_eq!(detail.first_store, Some((0x401001, PcodeOpcode::Store)));
        assert_eq!(
            detail.first_call,
            Some((0x401002, Some(0x500000), PcodeOpcode::Call))
        );
    }
}
