//! Time and fingerprint SLEIGH decoding over a binary's executable sections.
//!
//! usage: bind_bench <binary> <out-digests.txt | -> [rounds] [thumb]
//!
//! Sweeps every executable section linearly, once through the lift path
//! (instruction + p-code) and once decode-only, `rounds` times each, and
//! prints the best time per path. Then writes one line per decoded
//! instruction (`path address hash-of-debug-output`) so two builds can be
//! compared for byte-identical results.

use std::env;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::time::Instant;

use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn sections(binary: &LoadedBinary) -> Vec<(u64, &[u8])> {
    binary
        .sections
        .iter()
        .filter(|s| s.is_executable && s.file_size > 0)
        .filter_map(|s| {
            let start = usize::try_from(s.file_offset).ok()?;
            let end = start.checked_add(usize::try_from(s.file_size.min(s.virtual_size)).ok()?)?;
            Some((s.virtual_address, binary.data.as_slice().get(start..end)?))
        })
        .collect()
}

/// Sequential sweep, resyncing one alignment unit past any failure.
fn sweep<T>(
    bytes: &[u8],
    base: u64,
    step: usize,
    mut decode: impl FnMut(&[u8], u64) -> Option<(usize, T)>,
    mut each: impl FnMut(u64, T),
) -> usize {
    let mut offset = 0;
    let mut count = 0;
    while offset < bytes.len() {
        let address = base + offset as u64;
        match decode(&bytes[offset..], address) {
            Some((length, value)) if length > 0 => {
                each(address, value);
                count += 1;
                offset += length;
            }
            _ => offset += step,
        }
    }
    count
}

fn digest(value: &impl std::fmt::Debug) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{value:?}").hash(&mut hasher);
    hasher.finish()
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let binary = LoadedBinary::from_file(&args[1]).context("load")?;
    let out = &args[2];
    let rounds: usize = args.get(3).map(|r| r.parse()).transpose()?.unwrap_or(3);
    let frontend = RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().context("spec")?)?;
    let step = if binary.arch_spec.to_lowercase().contains("arm") {
        2
    } else {
        1
    };
    let context = (args.get(4).map(String::as_str) == Some("thumb"))
        .then(|| frontend.low_bit_code_mode_override())
        .flatten();
    let sections = sections(&binary);

    let lift = |bytes: &[u8], address: u64| {
        frontend
            .decode_instruction_and_lift_with_context_override(bytes, address, context)
            .ok()
            .map(|raw| (raw.0.length, raw))
    };
    let nopc = |bytes: &[u8], address: u64| {
        frontend
            .decode_single_no_pcode(bytes, address, context)
            .ok()
            .map(|inst| (inst.length, inst))
    };

    for (name, path) in [("lift", 0), ("nopc", 1)] {
        let mut best = f64::MAX;
        let mut count = 0;
        for _ in 0..rounds {
            let start = Instant::now();
            count = 0;
            for &(base, bytes) in &sections {
                count += if path == 0 {
                    sweep(bytes, base, step, lift, |_, raw| drop(raw))
                } else {
                    sweep(bytes, base, step, nopc, |_, inst| drop(inst))
                };
            }
            best = best.min(start.elapsed().as_secs_f64());
        }
        println!(
            "{name}: {count} instructions, best {best:.3} s ({:.2} us/instr)",
            best * 1e6 / count as f64
        );
    }

    if out == "-" {
        return Ok(());
    }
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    for &(base, bytes) in &sections {
        sweep(
            bytes,
            base,
            step,
            lift,
            |address, (inst, ops, len, details)| {
                // `details.userops` is the frontend's whole userop table, shared by
                // every lift; leave it out of the per-instruction fingerprint.
                let fingerprint = (
                    &inst,
                    &ops,
                    len,
                    &details.template_source,
                    &details.pending_context_commits,
                );
                writeln!(file, "lift {address:#x} {:016x}", digest(&fingerprint)).unwrap();
            },
        );
        sweep(bytes, base, step, nopc, |address, inst| {
            writeln!(file, "nopc {address:#x} {:016x}", digest(&inst)).unwrap();
        });
    }
    Ok(())
}
