//! Where FID identifications are lost, counted one stage at a time.
//!
//! `identify` reports a single number -- how many functions matched -- and
//! that number has been near zero on every binary tried so far. It cannot
//! say why, because every distinct failure arrives as the same `None`: the
//! function never hashed, the hash is in no database, the database holds it
//! but marks it auto-fail, or it holds it and the size threshold rejects it.
//! Those call for four different fixes, so this walks the funnel and prints
//! the count surviving each stage.
//!
//! Usage: `cargo run --release -p fission-decompiler --example fid_funnel --
//! <binary> [extra.fidbf ...]`. Each extra path is opened lazily and reported
//! on its own, which is how to ask whether a particular database in the
//! bundle holds anything for this binary at all.

use fission_decompiler::fid::{FidIdentifier, load_fid_databases};
use fission_signatures::fidbf::FID_ACCEPT_THRESHOLD;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: fid_funnel <binary>");
            std::process::exit(2);
        }
    };

    let binary =
        fission_loader::loader::LoadedBinary::from_file(&path).expect("load the binary under test");

    let databases = load_fid_databases(&binary);
    let total_entries: usize = databases.iter().map(|d| d.functions.len()).sum();
    println!(
        "databases loaded: {} ({} signature entries)",
        databases.len(),
        total_entries
    );
    for db in &databases {
        println!("  {:<52} {:>8} entries", db.source_path, db.functions.len());
    }

    let identifier = FidIdentifier::new(&binary, &databases).expect("FID is available here");

    let mut considered = 0usize;
    let mut hashed = 0usize;
    let mut hash_found = 0usize;
    let mut only_auto_fail = 0usize;
    let mut only_force_relation = 0usize;
    let mut only_force_specific = 0usize;
    let mut below_threshold = 0usize;
    let mut accepted = 0usize;
    // Of the ones the threshold rejects, how big were they? The threshold is
    // a code-unit count, so this says whether raising it is even the lever.
    let mut rejected_sizes: Vec<u32> = Vec::new();
    // The threshold is waived by 10 points when the *specific* hash also
    // matches, so whether that bonus ever lands decides how much of the
    // threshold drop is real.
    let mut candidates_seen = 0usize;
    let mut candidates_with_specific = 0usize;

    for func in &binary.functions {
        if func.is_import {
            continue;
        }
        considered += 1;
        let Some((_units, full_hash, specific_hash)) = identifier.hashes(func.address) else {
            continue;
        };
        hashed += 1;

        let candidates: Vec<_> = databases
            .iter()
            .flat_map(|db| db.find_by_full_hash(full_hash))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        hash_found += 1;

        let mut best_reason = None;
        let mut passed = false;
        for c in &candidates {
            candidates_seen += 1;
            if c.specific_hash == specific_hash {
                candidates_with_specific += 1;
            }
            if c.auto_fail {
                best_reason.get_or_insert("auto_fail");
                continue;
            }
            if c.force_relation {
                best_reason.get_or_insert("force_relation");
                continue;
            }
            if c.force_specific && c.specific_hash != specific_hash {
                best_reason.get_or_insert("force_specific");
                continue;
            }
            let bonus = if c.specific_hash == specific_hash {
                10.0
            } else {
                0.0
            };
            if c.auto_pass || (c.code_unit_size as f32 + bonus) >= FID_ACCEPT_THRESHOLD {
                passed = true;
                break;
            }
            best_reason.get_or_insert("below_threshold");
            rejected_sizes.push(c.code_unit_size);
        }

        if passed {
            accepted += 1;
        } else {
            match best_reason {
                Some("auto_fail") => only_auto_fail += 1,
                Some("force_relation") => only_force_relation += 1,
                Some("force_specific") => only_force_specific += 1,
                _ => below_threshold += 1,
            }
        }
    }

    println!();
    println!("functions considered      {considered}");
    println!("  hashed                  {hashed}");
    println!("  full hash in a database {hash_found}");
    println!("    accepted              {accepted}");
    println!("    dropped: auto_fail    {only_auto_fail}");
    println!("    dropped: force_rel    {only_force_relation}");
    println!("    dropped: force_spec   {only_force_specific}");
    println!("    dropped: threshold    {below_threshold}");

    for extra in std::env::args().skip(2) {
        let path = std::path::PathBuf::from(&extra);
        let Some(lazy) = fission_signatures::fidbf::fpk_store::LazyFidDatabase::open(&path) else {
            println!("\n{extra}: could not open");
            continue;
        };
        let language = binary
            .load_spec()
            .map(|spec| spec.pair.language_id.0.clone())
            .unwrap_or_default();
        let mut accepted = 0usize;
        for func in &binary.functions {
            if func.is_import {
                continue;
            }
            let Some((_units, full_hash, specific_hash)) = identifier.hashes(func.address) else {
                continue;
            };
            if !lazy.identify_by_hashes(full_hash, specific_hash).is_empty() {
                accepted += 1;
            }
        }
        println!(
            "\n{extra}: has_language({language})={} accepted={accepted}",
            lazy.has_language(&language)
        );
    }

    println!(
        "  candidates inspected {candidates_seen}, specific hash also matched {candidates_with_specific}"
    );

    if !rejected_sizes.is_empty() {
        rejected_sizes.sort_unstable();
        let median = rejected_sizes[rejected_sizes.len() / 2];
        println!(
            "  threshold-rejected code_unit_size: min={} median={} max={}",
            rejected_sizes[0],
            median,
            rejected_sizes[rejected_sizes.len() - 1]
        );
    }
}
