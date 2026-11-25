//! Measure where SLEIGH -> raw P-code cannot carry an instruction's meaning.
//!
//! P-code+ research (draft): before adding semantic contracts on top of P-code
//! we need to know how often raw P-code itself is the ceiling, as opposed to a
//! downstream consumer dropping a distinction P-code already made. This sweep
//! lifts every loader-known function of one binary and classifies each decoded
//! instruction by the gaps P-code has no way to state:
//!
//! - `opaque_userop`: the instruction lowers to a `CALLOTHER`, i.e. its effect
//!   is a named black box (keyed `mnemonic -> userop`).
//! - `zero_pcode`: the instruction produced no P-code at all. Legitimate for
//!   NOPs; anything else (fences, hints) has its effect erased.
//! - `untrapped_divide`: a non-constant integer divide/remainder. Most ISAs
//!   trap on a zero divisor; P-code `INT_DIV` has no trap, so the fault edge is
//!   not representable.
//!
//! It also counts the opcodes whose numeric domain (signed / unsigned / float)
//! is part of their meaning, so a consumer that merges them can be measured
//! against how often the distinction occurs.
//!
//! Output is one deterministic JSON document on stdout (BTreeMap order only).

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::{PcodeOp, PcodeOpcode};
use fission_sleigh::runtime::{
    userop_map_for_load_spec, DecodeContract, DecodedPcodeFunction, RuntimeSleighFrontend,
};
use fission_static::analysis::control_flow_facts::{decode_memory_context_for, function_max_bytes};
use serde::Serialize;

#[derive(Debug, Default, Serialize)]
struct GapReport {
    binary: String,
    language_id: Option<String>,
    functions_lifted: usize,
    functions_failed: usize,
    /// Distinct instruction addresses decoded across all lifted functions.
    instructions: usize,
    /// Instructions that hit at least one gap class.
    instructions_with_gap: usize,
    opaque_userop: BTreeMap<String, usize>,
    zero_pcode: BTreeMap<String, usize>,
    untrapped_divide: BTreeMap<String, usize>,
    domain_opcodes: BTreeMap<String, usize>,
}

fn lift(
    frontend: &RuntimeSleighFrontend,
    binary: &LoadedBinary,
    entry: u64,
    max_bytes: usize,
    instruction_limit: usize,
) -> Result<DecodedPcodeFunction> {
    let address_state = frontend.normalize_low_bit_code_address(entry);
    let decode_entry = address_state.address;
    let len = function_max_bytes(binary, decode_entry, max_bytes);
    let bytes = binary
        .view_bytes(decode_entry, len)
        .ok_or_else(|| anyhow!("unable to read bytes at 0x{decode_entry:x}"))?;
    let memory_context = decode_memory_context_for(binary, decode_entry, len);
    frontend.lift_raw_pcode_function_with_context_and_memory_context(
        bytes,
        decode_entry,
        DecodeContract::decomp_function(instruction_limit),
        &memory_context,
        address_state.context_override,
    )
}

/// Opcodes whose signed / unsigned / float domain is part of the operation.
fn is_domain_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::FloatDiv
    )
}

fn is_integer_divide(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv | PcodeOpcode::IntRem | PcodeOpcode::IntSRem
    )
}

fn userop_name(op: &PcodeOp, userops: &BTreeMap<u32, String>) -> String {
    op.inputs
        .first()
        .filter(|input| input.is_constant)
        .and_then(|input| u32::try_from(input.constant_val).ok())
        .map(|index| {
            userops
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("userop_{index}"))
        })
        .unwrap_or_else(|| "userop_?".to_string())
}

fn bump(map: &mut BTreeMap<String, usize>, key: String) {
    *map.entry(key).or_default() += 1;
}

fn main() -> Result<()> {
    let mut binary_path = None;
    let mut max_bytes = 256 * 1024;
    let mut instruction_limit = 4096;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--binary" => binary_path = Some(PathBuf::from(args.next().context("--binary value")?)),
            "--max-bytes" => max_bytes = args.next().context("--max-bytes value")?.parse()?,
            "--instruction-limit" => {
                instruction_limit = args.next().context("--instruction-limit value")?.parse()?
            }
            "--help" | "-h" => bail!(
                "usage: pcode_semantic_gap_sweep --binary PATH [--max-bytes N] [--instruction-limit N]"
            ),
            other => bail!("unknown argument: {other}"),
        }
    }
    let binary_path = binary_path.context("--binary is required")?;
    let binary = LoadedBinary::from_file(&binary_path)
        .with_context(|| format!("failed to load {}", binary_path.display()))?;
    let load_spec = binary
        .load_spec()
        .ok_or_else(|| anyhow!("loader did not select a load spec"))?;
    let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)?;
    let frontend = frontends
        .first()
        .ok_or_else(|| anyhow!("no executable SLEIGH frontend candidates"))?;
    let userops = userop_map_for_load_spec(load_spec).unwrap_or_default();

    let mut report = GapReport {
        binary: binary_path.display().to_string(),
        language_id: Some(frontend.language().to_string()),
        ..GapReport::default()
    };

    let mut entries: Vec<_> = binary.inner().functions.iter().collect();
    entries.sort_by_key(|func| func.address);
    // Functions overlap (shared tails, nested entries): classify each
    // instruction address once.
    let mut seen = BTreeSet::new();
    for func in entries {
        if func.is_import || func.is_thunk_like {
            continue;
        }
        let lifted = match lift(
            frontend,
            &binary,
            func.address,
            max_bytes,
            instruction_limit,
        ) {
            Ok(lifted) => lifted,
            Err(_) => {
                report.functions_failed += 1;
                continue;
            }
        };
        report.functions_lifted += 1;
        let mut ops_by_address: BTreeMap<u64, Vec<&PcodeOp>> = BTreeMap::new();
        for op in lifted
            .function
            .blocks
            .iter()
            .flat_map(|block| block.ops.iter())
        {
            ops_by_address.entry(op.address).or_default().push(op);
        }
        for instruction in &lifted.instructions {
            if !seen.insert(instruction.address) {
                continue;
            }
            report.instructions += 1;
            let mnemonic = instruction.mnemonic.to_ascii_lowercase();
            let ops = ops_by_address
                .get(&instruction.address)
                .map(Vec::as_slice)
                .unwrap_or_default();
            // Each gap class counts an instruction once, however many ops
            // (x86 DIV emits both a quotient and a remainder) carry it.
            let mut userop_names = BTreeSet::new();
            let mut untrapped = false;
            for op in ops {
                if op.opcode == PcodeOpcode::CallOther {
                    userop_names.insert(userop_name(op, &userops));
                }
                if is_integer_divide(op.opcode)
                    && op.inputs.get(1).is_some_and(|divisor| !divisor.is_constant)
                {
                    untrapped = true;
                }
                if is_domain_opcode(op.opcode) {
                    bump(&mut report.domain_opcodes, format!("{:?}", op.opcode));
                }
            }
            if ops.is_empty() {
                bump(&mut report.zero_pcode, mnemonic.clone());
            }
            for name in &userop_names {
                bump(&mut report.opaque_userop, format!("{mnemonic} -> {name}"));
            }
            if untrapped {
                bump(&mut report.untrapped_divide, mnemonic.clone());
            }
            if ops.is_empty() || !userop_names.is_empty() || untrapped {
                report.instructions_with_gap += 1;
            }
        }
    }

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
