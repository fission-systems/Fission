//! What fraction of real p-code the DIR native path can actually verify.
//!
//! The reconstructor works on a *region* -- a contiguous op range inside one
//! block, with one declared output -- and rejects anything it cannot model
//! rather than approximating it. That honesty is the point, but it means the
//! first question about the method is a coverage question: over real
//! functions, how much is reachable, and what specifically blocks the rest?
//!
//! For every block of every requested function this walks each op that writes
//! a register-or-unique location, grows the longest region ending at that op
//! which still reconstructs, and records why the next op refused. The output
//! is two distributions: how much of each block is covered, and which
//! opcode/shape is responsible for the boundary.
//!
//! Usage:
//!   cargo run --release -p fission-dir --example dir_coverage -- <binary> [addr ...]
//! With no addresses, every discovered function is measured.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fission_dir::{
    DirReconstructor, ObservedLocation, PcodeFoundation, PcodeNativeReconstructError,
    PcodeNativeReconstructor, PcodeRegionSelection,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: dir_coverage <binary> [addr ...]");
        std::process::exit(2);
    };
    let wanted: Vec<u64> = args
        .filter_map(|a| {
            let t = a.trim_start_matches("0x");
            u64::from_str_radix(t, 16).ok()
        })
        .collect();

    let binary = match fission_loader::loader::LoadedBinary::from_file(&path) {
        Ok(b) => b,
        Err(err) => {
            eprintln!("load failed: {err}");
            std::process::exit(1);
        }
    };

    let addrs: Vec<u64> = if wanted.is_empty() {
        binary.inner().functions.iter().map(|f| f.address).collect()
    } else {
        wanted
    };

    let mut fn_total = 0usize;
    let mut fn_with_pcode = 0usize;
    let mut ops_total = 0u64;
    let mut ops_covered = 0u64;
    let mut blocked: BTreeMap<String, u64> = BTreeMap::new();
    let mut region_sizes: Vec<usize> = Vec::new();

    for addr in addrs {
        fn_total += 1;
        let Some(pcode) = lift(&binary, addr) else {
            continue;
        };
        fn_with_pcode += 1;
        let Ok(foundation) = PcodeFoundation::new(addr, pcode.clone()) else {
            continue;
        };
        for block in &pcode.blocks {
            ops_total += block.ops.len() as u64;
            let mut covered_here = vec![false; block.ops.len()];
            for (idx, op) in block.ops.iter().enumerate() {
                let Some(out) = op.output.as_ref() else {
                    continue;
                };
                let output = ObservedLocation {
                    space_id: out.space_id,
                    offset: out.offset,
                    size: out.size,
                };
                // Longest region ending at `idx` that still reconstructs.
                let mut best = 0usize;
                let mut why: Option<String> = None;
                for len in 1..=(idx + 1) {
                    let sel = PcodeRegionSelection {
                        block_index: block.index,
                        first_op: idx + 1 - len,
                        op_count: len,
                        output,
                    };
                    let r = PcodeNativeReconstructor::new(sel, [1u64, 2, 3, 4, 5]);
                    match r.reconstruct(&foundation) {
                        Ok(_) => best = len,
                        Err(e) => {
                            if why.is_none() {
                                why = Some(reason(&e));
                            }
                            break;
                        }
                    }
                }
                if best > 0 {
                    region_sizes.push(best);
                    for k in (idx + 1 - best)..=idx {
                        covered_here[k] = true;
                    }
                } else if let Some(w) = why {
                    *blocked.entry(w).or_default() += 1;
                }
            }
            ops_covered += covered_here.iter().filter(|c| **c).count() as u64;
        }
    }

    println!("함수 {fn_total}개 중 p-code 확보 {fn_with_pcode}개");
    println!(
        "op {ops_total}개 중 검증 가능 영역에 포함 {ops_covered}개 = {:.1}%",
        100.0 * ops_covered as f64 / ops_total.max(1) as f64
    );
    if !region_sizes.is_empty() {
        region_sizes.sort_unstable();
        let p = |q: f64| region_sizes[((region_sizes.len() - 1) as f64 * q) as usize];
        println!(
            "재구성된 영역 {}개  크기 p50 {} p90 {} max {}",
            region_sizes.len(),
            p(0.5),
            p(0.9),
            region_sizes[region_sizes.len() - 1]
        );
    }
    println!("\n영역을 막은 사유 (출력 op 기준):");
    let mut rows: Vec<_> = blocked.into_iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (why, n) in rows.iter().take(15) {
        println!("  {n:6}  {why}");
    }
}

fn reason(e: &PcodeNativeReconstructError) -> String {
    match e {
        PcodeNativeReconstructError::UnsupportedOpcode { opcode, .. } => {
            format!("UnsupportedOpcode {opcode:?}")
        }
        PcodeNativeReconstructError::UnsupportedWidth { size, .. } => {
            format!("UnsupportedWidth {size}")
        }
        other => format!("{other:?}")
            .split(&[' ', '{'][..])
            .next()
            .unwrap_or("?")
            .to_string(),
    }
}

fn lift(
    binary: &fission_loader::loader::LoadedBinary,
    addr: u64,
) -> Option<fission_pcode::PcodeFunction> {
    fission_decompiler::disasm::raw_pcode_function(binary, addr).ok()
}
