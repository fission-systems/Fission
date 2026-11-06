//! How many functions FID names, and how many of those names are right.
//!
//! `fid_funnel` counts where candidates are lost; it cannot say whether the
//! accepted ones are correct. This runs the production path (`identify`) on a
//! binary that still has its symbol table, so the symbols are the answer key
//! (FID reads only code bytes, never names).
//!
//! Usage: `cargo run --release -p fission-decompiler --example fid_precision --
//! <unstripped binary>`. A name counts as correct if it equals any symbol at
//! that address -- aliases (`memcpy`/`__memcpy`) and byte-identical ifunc
//! resolvers are real ties, not mistakes.

use fission_decompiler::fid::{FidIdentifier, load_fid_databases};
use std::collections::{BTreeMap, HashMap, HashSet};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: fid_precision <binary>");
    let binary = fission_loader::loader::LoadedBinary::from_file(&path).expect("load");
    let databases = load_fid_databases(&binary);
    let identifier = FidIdentifier::new(&binary, &databases).expect("FID is available here");

    let mut names_at: HashMap<u64, HashSet<String>> = HashMap::new();
    for f in &binary.functions {
        if !f.is_import && f.size > 0 && !f.name.is_empty() {
            names_at
                .entry(f.address)
                .or_default()
                .insert(f.name.clone());
        }
    }
    let mut addrs: Vec<u64> = names_at.keys().copied().collect();
    addrs.sort_unstable();

    let (mut hashed, mut identified, mut correct) = (0usize, 0usize, 0usize);
    let mut wrong: BTreeMap<u64, (String, String, f32)> = BTreeMap::new();
    let mut hit: BTreeMap<u64, (String, f32)> = BTreeMap::new();
    for &addr in &addrs {
        if identifier.hashes(addr).is_some() {
            hashed += 1;
        }
        if let Some(id) = identifier.identify(addr) {
            identified += 1;
            let truth = &names_at[&addr];
            if truth.contains(&id.name) {
                correct += 1;
                hit.insert(addr, (id.name, id.score));
            } else {
                let mut t: Vec<_> = truth.iter().cloned().collect();
                t.sort();
                wrong.insert(addr, (id.name, t.join("|"), id.score));
            }
        }
    }
    println!(
        "SUMMARY functions={} hashed={hashed} identified={identified} correct={correct} wrong={}",
        addrs.len(),
        identified - correct
    );
    for (a, (n, s)) in &hit {
        println!("HIT 0x{a:x} {n} {s:.2}");
    }
    for (a, (n, t, s)) in &wrong {
        println!("WRONG 0x{a:x} got={n} truth={t} {s:.2}");
    }
}
