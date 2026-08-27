//! Transitive no-return function propagation (Ghidra's `FindNoReturnFunctionsAnalyzer`
//! / `NoReturnFunctionAnalyzer` counterpart).
//!
//! `control_flow_facts` only *consumes* a static by-name no-return list
//! (`fission_core::core::ghidra_no_return`) for call sites whose target is a
//! well-known external (`abort`, `ExitProcess`, ...). It never computes that
//! a *local* function is transitively no-return -- e.g. `static void
//! die(void) { log(...); abort(); }` is obviously no-return itself, but
//! nothing propagates that, so calls to `die()` elsewhere still get a
//! spurious fall-through edge.
//!
//! This module closes that gap with a whole-binary fixpoint: seed the
//! by-name-known set, then repeatedly check each remaining function's own
//! p-code CFG for whether every path is terminated by either an infinite
//! loop or a call/tail-branch to an *already-known* no-return function --
//! newly-found no-return functions can make their own callers provably
//! no-return in the next round, so this iterates rather than running once
//! (same shape kuna's own `noreturn_propagate` fixpoint uses).

use std::collections::BTreeSet;
use std::collections::hash_map::HashMap as StdHashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use fission_core::core::ghidra_no_return::{
    binary_format_to_ghidra_format, ghidra_no_return_index,
};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{PcodeFunction, PcodeOpcode};
use fission_sleigh::runtime::{DecodeContract, DecodeMemoryContext, RuntimeSleighFrontend};
use lru::LruCache;
use rayon::prelude::*;

use super::control_flow_facts::ghidra_no_return_compiler_key;

/// Deliberately loader-only, *not* `control_flow_facts::function_max_bytes`:
/// that helper (like `decode_memory_context_for`) calls back into
/// `control_flow_facts_for`, which is what calls into this module in the
/// first place (see the wire-up in `control_flow_facts::assemble`) -- using
/// either here would recurse `assemble` -> this module -> `assemble` -> ...
/// for every function, forever. A plain next-function-in-`binary.functions`
/// bound is slightly less precise (no `control_flow_facts`-refined
/// `function_extents`) but is all the structural no-return check needs.
fn no_return_scan_max_bytes(binary: &LoadedBinary, entry_address: u64, fallback: usize) -> usize {
    let inner = binary.inner();
    if let Some(&idx) = inner.function_addr_index.get(&entry_address) {
        if let Some(info) = inner.functions.get(idx) {
            if info.size > 0 {
                return info.size as usize;
            }
        }
    }
    let mut next = entry_address.saturating_add(fallback as u64);
    for info in &inner.functions {
        if info.address > entry_address && info.address < next {
            next = info.address;
        }
    }
    next.saturating_sub(entry_address).max(1) as usize
}

/// Generous per-function instruction budget for the structural CFG check --
/// same rationale as `decomp::facts::FID_INSTRUCTION_LIMIT`, not a tuning knob.
const NORETURN_INSTRUCTION_LIMIT: usize = 4000;

const CACHE_CAPACITY: usize = 8;

static NORETURN_CACHE: OnceLock<Mutex<LruCache<String, BTreeSet<u64>>>> = OnceLock::new();

fn noreturn_cache() -> &'static Mutex<LruCache<String, BTreeSet<u64>>> {
    NORETURN_CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).expect("non-zero cache capacity"),
        ))
    })
}

/// Return the set of function entry addresses known (by name) or computed
/// (structurally, transitively) to never return, for `binary`.
///
/// Cached per `binary.hash`, mirroring `control_flow_facts_for`'s cache.
pub fn no_return_functions_for(binary: &LoadedBinary) -> BTreeSet<u64> {
    let hash = binary.hash.clone();
    if let Ok(mut cache) = noreturn_cache().lock() {
        if let Some(set) = cache.get(&hash) {
            return set.clone();
        }
    }

    let set = compute_no_return_functions(binary);

    if let Ok(mut cache) = noreturn_cache().lock() {
        cache.put(hash, set.clone());
    }
    set
}

fn compute_no_return_functions(binary: &LoadedBinary) -> BTreeSet<u64> {
    let mut no_return: BTreeSet<u64> = BTreeSet::new();

    let Some(ghidra_format) = binary_format_to_ghidra_format(&binary.format) else {
        return no_return;
    };
    let compiler_key = ghidra_no_return_compiler_key(binary);
    let no_return_idx = ghidra_no_return_index();

    // Seed: any function (including imports) whose own resolved name/library
    // is already known no-return.
    for f in &binary.functions {
        if f.name.trim().is_empty() {
            continue;
        }
        if no_return_idx.is_no_return(
            ghidra_format,
            compiler_key,
            f.external_library.as_deref(),
            &f.name,
        ) {
            no_return.insert(f.address);
        }
    }

    let Some(load_spec) = binary.load_spec() else {
        return no_return;
    };
    let Ok(frontend) = RuntimeSleighFrontend::new_for_load_spec(load_spec) else {
        return no_return;
    };

    // Decode every non-import function once, in parallel (same
    // parallel-decode-then-sequential-merge shape as the FID-matching fix
    // earlier this session) -- the fixpoint below only re-walks these
    // already-decoded, in-memory CFGs, never re-decodes.
    let decoded: StdHashMap<u64, PcodeFunction> = binary
        .functions
        .par_iter()
        .filter(|f| !f.is_import)
        .filter_map(|f| {
            let max_bytes = no_return_scan_max_bytes(binary, f.address, 4096);
            let bytes = binary.view_bytes(f.address, max_bytes)?;
            let memory_context = DecodeMemoryContext::default();
            let contract = DecodeContract::decomp_function(NORETURN_INSTRUCTION_LIMIT);
            let decoded = frontend
                .lift_raw_pcode_function_with_context_and_memory_context(
                    bytes,
                    f.address,
                    contract,
                    &memory_context,
                    None,
                )
                .ok()?;
            Some((f.address, decoded.function))
        })
        .collect();

    loop {
        let mut changed = false;
        for (&addr, pcode) in &decoded {
            if no_return.contains(&addr) {
                continue;
            }
            if function_is_structurally_no_return(pcode, &no_return) {
                no_return.insert(addr);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    no_return
}

/// Whether every reachable path through `pcode`'s own CFG (from its entry
/// block) is terminated by either an infinite loop or a call/tail-branch to
/// an address already in `no_return` -- i.e. no real `Return` op is
/// reachable. Deliberately conservative: an indirect call/branch (target not
/// a resolvable constant) or a branch outside this function's own decoded
/// blocks never counts as no-return evidence, so this can under-detect but
/// must never mark an actually-returning function as no-return.
fn function_is_structurally_no_return(pcode: &PcodeFunction, no_return: &BTreeSet<u64>) -> bool {
    if pcode.blocks.is_empty() {
        return false;
    }

    let mut visited = vec![false; pcode.blocks.len()];
    let mut stack: Vec<u32> = vec![0];

    while let Some(idx) = stack.pop() {
        let idx_usize = idx as usize;
        if idx_usize >= pcode.blocks.len() || visited[idx_usize] {
            continue;
        }
        visited[idx_usize] = true;
        let block = &pcode.blocks[idx_usize];

        let mut terminal_no_continue = false;
        for op in &block.ops {
            match op.opcode {
                PcodeOpcode::Return => return false,
                PcodeOpcode::Call | PcodeOpcode::Branch => {
                    if let Some(target) = op.inputs.first() {
                        if no_return.contains(&target.offset) {
                            terminal_no_continue = true;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        if terminal_no_continue {
            continue;
        }
        // A path that leaves without a `Return` and without a proven
        // no-return terminator has left this analysis, not the program: an
        // unresolved indirect branch has no successors to walk, so the loop
        // simply ends and every remaining block looks accounted for. That
        // silently promoted a PLT stub -- `endbr64; jmp qword ptr [GOT]`,
        // which is how every imported call is reached -- into the no-return
        // set, and `__strcat_chk` is not a function that fails to return.
        // Under-detecting is the documented tradeoff; this is the
        // over-detection the same paragraph forbids.
        if block.successors.is_empty() {
            return false;
        }
        stack.extend(block.successors.iter().copied());
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_pcode::{PcodeBasicBlock, PcodeOp, Varnode};

    fn op(seq_num: u32, opcode: PcodeOpcode) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x1000 + seq_num as u64,
            output: None,
            inputs: Vec::new(),
            asm_mnemonic: None,
        }
    }

    fn call_op(seq_num: u32, target: u64) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode: PcodeOpcode::Call,
            address: 0x1000 + seq_num as u64,
            output: None,
            inputs: vec![Varnode::constant(target as i64, 8)],
            asm_mnemonic: None,
        }
    }

    fn block(
        index: u32,
        start_address: u64,
        successors: Vec<u32>,
        ops: Vec<PcodeOp>,
    ) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index,
            start_address,
            successors,
            ops,
        }
    }

    #[test]
    fn function_ending_in_return_is_not_no_return() {
        let pcode = PcodeFunction {
            blocks: vec![block(0, 0x1000, vec![], vec![op(0, PcodeOpcode::Return)])],
        };
        assert!(!function_is_structurally_no_return(
            &pcode,
            &BTreeSet::new()
        ));
    }

    #[test]
    fn function_ending_in_call_to_known_no_return_is_no_return() {
        let no_return = BTreeSet::from([0x9999]);
        let pcode = PcodeFunction {
            blocks: vec![block(0, 0x1000, vec![], vec![call_op(0, 0x9999)])],
        };
        assert!(function_is_structurally_no_return(&pcode, &no_return));
    }

    #[test]
    fn function_calling_unknown_target_then_returning_is_not_no_return() {
        let pcode = PcodeFunction {
            blocks: vec![block(
                0,
                0x1000,
                vec![],
                vec![call_op(0, 0x1234), op(1, PcodeOpcode::Return)],
            )],
        };
        assert!(!function_is_structurally_no_return(
            &pcode,
            &BTreeSet::new()
        ));
    }

    #[test]
    fn diamond_with_one_branch_returning_is_not_no_return() {
        // block 0 -> {1, 2}; block 1 calls known-no-return (terminal);
        // block 2 returns normally -> overall NOT no-return.
        let no_return = BTreeSet::from([0x9999]);
        let pcode = PcodeFunction {
            blocks: vec![
                block(0, 0x1000, vec![1, 2], vec![op(0, PcodeOpcode::CBranch)]),
                block(1, 0x1010, vec![], vec![call_op(1, 0x9999)]),
                block(2, 0x1020, vec![], vec![op(2, PcodeOpcode::Return)]),
            ],
        };
        assert!(!function_is_structurally_no_return(&pcode, &no_return));
    }

    #[test]
    fn infinite_loop_with_no_return_op_is_no_return() {
        // block 0 -> block 0 (self-loop), never reaches a Return.
        let pcode = PcodeFunction {
            blocks: vec![block(0, 0x1000, vec![0], vec![op(0, PcodeOpcode::Branch)])],
        };
        assert!(function_is_structurally_no_return(&pcode, &BTreeSet::new()));
    }

    #[test]
    fn empty_function_is_not_no_return() {
        let pcode = PcodeFunction { blocks: vec![] };
        assert!(!function_is_structurally_no_return(
            &pcode,
            &BTreeSet::new()
        ));
    }

    #[test]
    fn unresolved_indirect_tail_branch_is_not_no_return() {
        // A PLT stub: `endbr64; jmp qword ptr [GOT]`. No `Return` op, and
        // the indirect branch resolves to nothing, so the block has no
        // successors to walk. Reading that as "no path returns" makes every
        // imported call look like it never comes back.
        let pcode = PcodeFunction {
            blocks: vec![block(
                0,
                0x1150,
                vec![],
                vec![op(0, PcodeOpcode::BranchInd)],
            )],
        };
        assert!(!function_is_structurally_no_return(
            &pcode,
            &BTreeSet::new()
        ));
    }

    #[test]
    fn direct_branch_to_unknown_target_is_not_no_return() {
        // Same shape, direct: a tail call whose target this function's own
        // decode never covered. Not evidence either way, so not no-return.
        let pcode = PcodeFunction {
            blocks: vec![block(0, 0x1000, vec![], vec![call_op(0, 0x9999)])],
        };
        assert!(!function_is_structurally_no_return(
            &pcode,
            &BTreeSet::new()
        ));
    }
}
