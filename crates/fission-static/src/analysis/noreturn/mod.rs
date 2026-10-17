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

mod scoped_decode;

use std::collections::BTreeSet;
use std::collections::hash_map::HashMap as StdHashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use fission_core::core::ghidra_no_return::{
    binary_format_to_ghidra_format, ghidra_no_return_index,
};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{PcodeFunction, PcodeOpcode};
use fission_sleigh::runtime::{
    DecodeContract, DecodeMemoryContext, RawDecodeSeed, RuntimeSleighFrontend,
};
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
pub(super) fn no_return_scan_max_bytes(
    binary: &LoadedBinary,
    entry_address: u64,
    fallback: usize,
) -> usize {
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
    no_return_functions_with_seeds(binary, None)
}

pub(crate) type DecodeSeeds = StdHashMap<u64, RawDecodeSeed>;

pub(crate) fn no_return_functions_with_seeds(
    binary: &LoadedBinary,
    seeds: Option<&mut DecodeSeeds>,
) -> BTreeSet<u64> {
    let hash = binary.hash.clone();
    if let Ok(mut cache) = noreturn_cache().lock() {
        if let Some(set) = cache.get(&hash) {
            return set.clone();
        }
    }

    let set = compute_no_return_functions(binary, seeds);

    if let Ok(mut cache) = noreturn_cache().lock() {
        cache.put(hash, set.clone());
    }
    set
}

fn compute_no_return_functions(
    binary: &LoadedBinary,
    mut seeds: Option<&mut DecodeSeeds>,
) -> BTreeSet<u64> {
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

    // A global Rayon wait under CF OnceLock can steal another CF caller
    // and block on itself. Only seed-producing FID preparation may use the
    // scoped OS queue: workers decode immutable inputs and cannot call CF;
    // neither caller work nor std::thread joins execute unrelated Rayon jobs.
    // Ordinary no-return callers retain sequential preparation.
    let capture = seeds.is_some();
    let decode = |index: usize| {
        let f = &binary.functions[index];
        if f.is_import {
            return None;
        }
        let address_state = frontend.normalize_low_bit_code_address(f.address);
        let decode_address = address_state.address;
        let context_override = crate::analysis::decode_context_for_address(
            binary,
            &frontend,
            address_state.context_override,
        );
        let max_bytes = no_return_scan_max_bytes(binary, decode_address, 4096);
        let bytes = binary.view_bytes(decode_address, max_bytes)?;
        let memory_context = DecodeMemoryContext::default();
        let contract = DecodeContract::decomp_function(NORETURN_INSTRUCTION_LIMIT);
        let (decoded, seed) = if capture {
            let (decoded, seed) = frontend
                .lift_raw_pcode_function_capturing_seed(
                    bytes,
                    decode_address,
                    contract,
                    &memory_context,
                    context_override,
                )
                .ok()?;
            (decoded, Some(seed))
        } else {
            let decoded = frontend
                .lift_raw_pcode_function_with_context_and_memory_context(
                    bytes,
                    decode_address,
                    contract,
                    &memory_context,
                    context_override,
                )
                .ok()?;
            (decoded, None)
        };
        Some((
            f.address,
            NoReturnFlow::from_function(&decoded.function),
            seed,
        ))
    };
    let rows = if capture {
        scoped_decode::collect_scoped(binary.functions.len(), rayon::current_num_threads(), decode)
    } else {
        (0..binary.functions.len()).filter_map(decode).collect()
    };
    let mut decoded = StdHashMap::<u64, NoReturnFlow>::new();
    // Restore original input order before map insertion, including duplicate
    // address last-writer behavior. Fixed-point iteration remains unchanged.
    for (address, flow, seed) in rows {
        decoded.insert(address, flow);
        if let (Some(seeds), Some(seed)) = (seeds.as_deref_mut(), seed) {
            seeds.insert(address, seed);
        }
    }

    loop {
        let mut changed = false;
        for (&addr, pcode) in &decoded {
            if no_return.contains(&addr) {
                continue;
            }
            if pcode.is_no_return(&no_return) {
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
#[derive(Debug)]
struct NoReturnFlow {
    blocks: Vec<NoReturnBlock>,
}
#[derive(Debug)]
struct NoReturnBlock {
    events: Vec<NoReturnEvent>,
    successors: Vec<u32>,
}
#[derive(Debug)]
enum NoReturnEvent {
    Return,
    Target(u64),
}
impl NoReturnFlow {
    fn from_function(function: &PcodeFunction) -> Self {
        Self {
            blocks: function
                .blocks
                .iter()
                .map(|block| NoReturnBlock {
                    events: block
                        .ops
                        .iter()
                        .filter_map(|op| match op.opcode {
                            PcodeOpcode::Return => Some(NoReturnEvent::Return),
                            PcodeOpcode::Call | PcodeOpcode::Branch => {
                                op.inputs.first().map(|v| NoReturnEvent::Target(v.offset))
                            }
                            _ => None,
                        })
                        .collect(),
                    successors: block.successors.clone(),
                })
                .collect(),
        }
    }
    fn is_no_return(&self, no_return: &BTreeSet<u64>) -> bool {
        if self.blocks.is_empty() {
            return false;
        }
        let mut visited = vec![false; self.blocks.len()];
        let mut stack = vec![0u32];
        while let Some(idx) = stack.pop() {
            let Some(block) = self.blocks.get(idx as usize) else {
                continue;
            };
            if visited[idx as usize] {
                continue;
            }
            visited[idx as usize] = true;
            let mut terminated = false;
            for event in &block.events {
                match event {
                    NoReturnEvent::Return => return false,
                    NoReturnEvent::Target(target) if no_return.contains(target) => {
                        terminated = true;
                        break;
                    }
                    _ => {}
                }
            }
            if terminated {
                continue;
            }
            if block.successors.is_empty() {
                return false;
            }
            stack.extend(block.successors.iter().copied());
        }
        true
    }
}

#[cfg(test)]
fn function_is_structurally_no_return(pcode: &PcodeFunction, no_return: &BTreeSet<u64>) -> bool {
    NoReturnFlow::from_function(pcode).is_no_return(no_return)
}

// Independent pre-change predicate retained only for differential regression checks.
#[cfg(test)]
fn original_no_return_predicate(pcode: &PcodeFunction, no_return: &BTreeSet<u64>) -> bool {
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
    fn scoped_no_return_preparation_matches_serial_facts_and_complete_seeded_decodes() {
        use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo};
        let base = 0x401000u64;
        let abort = base + 0x1000;
        let mut bytes = vec![0x90; 256];
        let mut functions = Vec::new();
        for i in 0..16usize {
            let offset = i * 16;
            let address = base + offset as u64;
            let code = match i % 3 {
                0 => {
                    let mut v = vec![0xe8];
                    v.extend_from_slice(
                        &((abort as i64 - address as i64 - 5) as i32).to_le_bytes(),
                    );
                    v.push(0xc3);
                    v
                }
                1 => vec![0xeb, 0xfe],
                _ => vec![0xc3],
            };
            bytes[offset..offset + code.len()].copy_from_slice(&code);
            functions.push(FunctionInfo {
                name: format!("fixture_{i}"),
                address,
                size: code.len() as u64,
                ..Default::default()
            });
        }
        let mut builder =
            LoadedBinaryBuilder::new("scoped-no-return.bin".into(), DataBuffer::Heap(bytes))
                .format("ELF")
                .arch_spec("x86:LE:64:default")
                .is_64bit(true)
                .image_base(0x400000)
                .add_section(SectionInfo {
                    name: ".text".into(),
                    virtual_address: base,
                    virtual_size: 256,
                    file_offset: 0,
                    file_size: 256,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                });
        for function in functions {
            builder = builder.add_function(function);
        }
        let binary = builder
            .add_function(FunctionInfo {
                name: "abort".into(),
                address: abort,
                is_import: true,
                ..Default::default()
            })
            .build()
            .unwrap();
        let serial = compute_no_return_functions(&binary, None);
        assert!(serial.contains(&abort));
        let mut seeds = DecodeSeeds::new();
        assert_eq!(
            compute_no_return_functions(&binary, Some(&mut seeds)),
            serial
        );
        assert_eq!(seeds.len(), 16);
        let frontend =
            RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().unwrap()).unwrap();
        for function in binary.functions.iter().filter(|f| !f.is_import) {
            let bytes = binary
                .view_bytes(function.address, function.size as usize)
                .unwrap();
            let context = DecodeMemoryContext::default();
            let contract = DecodeContract::decomp_function(NORETURN_INSTRUCTION_LIMIT);
            let expected = frontend
                .lift_raw_pcode_function_with_context_and_memory_context(
                    bytes,
                    function.address,
                    contract,
                    &context,
                    None,
                )
                .unwrap();
            let actual = frontend
                .lift_raw_pcode_function_with_seed(
                    bytes,
                    function.address,
                    contract,
                    &context,
                    None,
                    seeds.remove(&function.address),
                )
                .unwrap();
            assert_eq!(actual, expected);
        }
        assert!(seeds.is_empty());
    }

    #[test]
    fn compact_no_return_flow_matches_original_across_ordered_paths_and_known_subsets() {
        let mut rng = 0x1345_abcdu64;
        for _case in 0..600 {
            let mut next = || {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                rng >> 32
            };
            let count = (next() % 7) as usize;
            let mut blocks = Vec::new();
            for i in 0..count {
                let mut ops = Vec::new();
                for seq in 0..next() % 9 {
                    let opcode = [
                        PcodeOpcode::Return,
                        PcodeOpcode::Call,
                        PcodeOpcode::Branch,
                        PcodeOpcode::CallInd,
                        PcodeOpcode::BranchInd,
                        PcodeOpcode::Copy,
                    ][(next() % 6) as usize];
                    let mut op = op(seq as u32, opcode);
                    if next() % 4 != 0 {
                        let mut target = Varnode::constant((next() % 4) as i64, 8);
                        target.is_constant = next() % 2 == 0;
                        op.inputs.push(target);
                    }
                    ops.push(op);
                }
                let successors = (0..next() % 4).map(|_| (next() % 9) as u32).collect();
                blocks.push(block(i as u32 + 7, 0x1000 + i as u64 * 16, successors, ops));
            }
            let function = PcodeFunction { blocks };
            let compact = NoReturnFlow::from_function(&function);
            for bits in 0..16 {
                let known = (0..4u64).filter(|i| bits & (1 << i) != 0).collect();
                assert_eq!(
                    compact.is_no_return(&known),
                    original_no_return_predicate(&function, &known)
                );
            }
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
