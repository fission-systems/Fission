//! Block-level DIR coverage: how much of a basic block gets verified, not
//! how many ops sit inside some fragment.
//!
//! `dir_coverage` counts ops covered by *any* region, which overstates what a
//! reader would call verified: a region of two ops proves an expression, not a
//! block. The unit here is one basic block and every location it writes that
//! something later could read -- a block counts as covered only when a single
//! region spanning the whole block reconstructs for *each* of those outputs.
//!
//! `--verify` additionally runs the emulator/solver pipeline and requires an
//! `Equivalent` verdict rather than merely a successful reconstruction. That
//! is the number worth reporting; reconstruction alone only says the region
//! was expressible.
//!
//! Usage:
//!   cargo run --release -p fission-dir --example dir_block_coverage -- [--verify] <binary> [addr ...]

use std::collections::BTreeMap;
use std::path::PathBuf;

const REGISTER_SPACES: [u64; 3] = [1, 4, 5];

use fission_dir::{
    DirPipeline, DirReconstructor, ObservationScope, ObservedLocation, PcodeFoundation,
    PcodeNativeReconstructor, PcodeObservationVerifier, PcodeRegionSelection, ValidationBudget,
    ValidationVerdict,
};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let verify = args.first().map(String::as_str) == Some("--verify");
    if verify {
        args.remove(0);
    }
    let Some(path) = args.first().map(PathBuf::from) else {
        eprintln!("usage: dir_block_coverage [--verify] <binary> [addr ...]");
        std::process::exit(2);
    };
    let wanted: Vec<u64> = args[1..]
        .iter()
        .filter_map(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        .collect();

    let Ok(binary) = fission_loader::loader::LoadedBinary::from_file(&path) else {
        eprintln!("load failed");
        std::process::exit(1);
    };
    let addrs: Vec<u64> = if wanted.is_empty() {
        binary.inner().functions.iter().map(|f| f.address).collect()
    } else {
        wanted
    };

    let (mut blocks_total, mut blocks_covered) = (0u64, 0u64);
    let (mut fns_total, mut fns_covered) = (0u64, 0u64);
    let mut outputs_total = 0u64;
    let mut outputs_covered = 0u64;
    let mut shortfall: BTreeMap<u64, u64> = BTreeMap::new();

    for addr in addrs {
        let Ok(pcode) = fission_decompiler::disasm::raw_pcode_function(&binary, addr) else {
            continue;
        };
        let Ok(foundation) = PcodeFoundation::new(addr, pcode.clone()) else {
            continue;
        };
        fns_total += 1;
        let mut all_blocks_ok = true;
        for block in &pcode.blocks {
            if block.ops.is_empty() {
                continue;
            }
            blocks_total += 1;
            // Only *observable* writes count. A unique is internal to one
            // instruction's lowering -- nothing outside the block can read it,
            // so counting uniques would measure the lowering's verbosity, not
            // what the block leaves behind. Registers are the block's effect.
            let mut outs: Vec<(ObservedLocation, usize)> = Vec::new();
            for (index, op) in block.ops.iter().enumerate() {
                let Some(out) = op.output.as_ref() else {
                    continue;
                };
                if out.is_constant || !REGISTER_SPACES.contains(&out.space_id) {
                    continue;
                }
                let location = ObservedLocation {
                    space_id: out.space_id,
                    offset: out.offset,
                    size: out.size,
                };
                // Keep the *last* write: that is the value the block leaves.
                match outs.iter_mut().find(|(l, _)| *l == location) {
                    Some(entry) => entry.1 = index,
                    None => outs.push((location, index)),
                }
            }

            if outs.is_empty() {
                // No observable effect to verify; not evidence either way.
                blocks_total -= 1;
                continue;
            }
            let mut block_ok = true;
            let mut covered_here = 0u64;
            for (output, last_write) in &outs {
                outputs_total += 1;
                // Ops after the last write of this location cannot affect it.
                let selection = PcodeRegionSelection {
                    block_index: block.index,
                    first_op: 0,
                    op_count: last_write + 1,
                    output: *output,
                };
                let reconstructor = PcodeNativeReconstructor::new(selection, [1u64, 2, 3, 4, 5]);
                let ok = if verify {
                    DirPipeline::new(reconstructor, PcodeObservationVerifier::default())
                        .run(
                            &foundation,
                            &ObservationScope::location_only(*output),
                            ValidationBudget::default(),
                        )
                        .ok()
                        .and_then(|artifacts| artifacts.into_iter().next())
                        .is_some_and(|artifact| {
                            artifact.assurance.verdict() == ValidationVerdict::Equivalent
                        })
                } else {
                    reconstructor.reconstruct(&foundation).is_ok()
                };
                if ok {
                    outputs_covered += 1;
                    covered_here += 1;
                } else {
                    block_ok = false;
                }
            }
            if block_ok {
                blocks_covered += 1;
            } else {
                *shortfall
                    .entry(outs.len() as u64 - covered_here)
                    .or_default() += 1;
                all_blocks_ok = false;
            }
        }
        if all_blocks_ok {
            fns_covered += 1;
        }
    }

    let pct = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "{}  함수 {fns_total}  블록 {blocks_total}  출력 {outputs_total}",
        if verify { "[검증]" } else { "[재구성만]" }
    );
    println!(
        "  출력 단위  {outputs_covered}/{outputs_total} = {:.1}%",
        pct(outputs_covered, outputs_total)
    );
    println!(
        "  블록 단위  {blocks_covered}/{blocks_total} = {:.1}%   (블록의 모든 출력이 덮인 경우)",
        pct(blocks_covered, blocks_total)
    );
    println!(
        "  함수 단위  {fns_covered}/{fns_total} = {:.1}%   (모든 블록이 덮인 경우)",
        pct(fns_covered, fns_total)
    );
    if !shortfall.is_empty() {
        println!("\n덮이지 않은 블록의 미달 출력 수 분포:");
        for (missing, count) in shortfall.iter().take(8) {
            println!("  출력 {missing}개 부족: {count}블록");
        }
    }
}
