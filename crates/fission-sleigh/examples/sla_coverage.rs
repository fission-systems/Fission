//! How much of the SLEIGH frontend does the checked-in `.sla` actually carry?
//!
//! The runtime compiles `.slaspec`/`.sinc` and then overwrites the result with
//! `apply_required_sla_overlay`. If the `.sla` is complete, the compile is
//! scaffolding and the 11 MB of SLEIGH source is a runtime dependency only by
//! accident. This measures that rather than arguing it.

use std::collections::BTreeMap;
use std::path::Path;

use fission_sleigh::compiler::sla::load_construct_templates_from_sla;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../utils/sleigh-specs/compiled")
        .canonicalize()
        .expect("compiled sla root");

    let mut files: Vec<_> = walk(&root);
    files.sort();

    let mut ok = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut subtables_total = 0usize;
    let mut ctors_total = 0usize;
    let mut ctors_unsupported = 0usize;
    let mut no_decision_tree: Vec<String> = Vec::new();
    let mut per_file: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut ctx_named = 0usize;
    let mut low_bit_hits: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut files_without_ctx: Vec<String> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match load_construct_templates_from_sla(path) {
            Ok(lib) => {
                ok += 1;
                let native = &lib.native;
                if native.context_fields.is_empty() {
                    files_without_ctx.push(name.clone());
                }
                ctx_named += native.context_fields.len();
                let hits: Vec<String> = native
                    .context_fields
                    .iter()
                    .map(|f| f.name.clone())
                    .filter(|n| ["TMode", "T", "ISA_MODE", "LowBitCodeMode"].contains(&n.as_str()))
                    .collect();
                if !hits.is_empty() {
                    low_bit_hits.insert(name.clone(), hits);
                }
                let mut subs = 0;
                let mut ctors = 0;
                let mut unsup = 0;
                for (sub_name, sub) in &native.subtables {
                    subs += 1;
                    if sub.decision_tree.is_none() && !sub.constructors.is_empty() {
                        no_decision_tree.push(format!("{name}:{sub_name}"));
                    }
                    for c in &sub.constructors {
                        ctors += 1;
                        if !matches!(
                            c.decode_status,
                            fission_sleigh::compiler::CompiledSlaDecodeStatus::Decoded
                        ) {
                            unsup += 1;
                        }
                    }
                }
                subtables_total += subs;
                ctors_total += ctors;
                ctors_unsupported += unsup;
                per_file.insert(name, (subs, ctors, unsup));
            }
            Err(e) => failed.push((name, format!("{e:#}"))),
        }
    }

    println!("compiled .sla files : {}", files.len());
    println!("  parsed ok         : {ok}");
    println!("  failed            : {}", failed.len());
    for (n, e) in failed.iter().take(10) {
        println!("      {n}: {}", e.lines().next().unwrap_or(""));
    }
    println!("subtables           : {subtables_total}");
    println!("constructors        : {ctors_total}");
    println!(
        "  decode Unsupported: {ctors_unsupported} ({:.2}%)",
        if ctors_total == 0 { 0.0 } else { ctors_unsupported as f64 / ctors_total as f64 * 100.0 }
    );
    println!("subtables with constructors but no decision tree: {}", no_decision_tree.len());
    for n in no_decision_tree.iter().take(10) {
        println!("      {n}");
    }

    println!("named context fields reachable from SlaLanguage: {ctx_named}");
    println!("  files with none: {} (a language with no `define context` has none to carry)",
             files_without_ctx.len());

    println!("\nlow-bit-code context fields the runtime looks up by name, found in .sla:");
    println!("  files carrying at least one: {}", low_bit_hits.len());
    for (f, hits) in low_bit_hits.iter().take(8) {
        println!("    {f:<28} {hits:?}");
    }

    println!("\nworst files by unsupported constructors:");
    let mut worst: Vec<_> = per_file.iter().filter(|(_, v)| v.2 > 0).collect();
    worst.sort_by_key(|(_, v)| std::cmp::Reverse(v.2));
    for (n, (subs, ctors, unsup)) in worst.iter().take(10) {
        println!("  {n:<40} subtables={subs:<5} ctors={ctors:<6} unsupported={unsup}");
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().and_then(|x| x.to_str()) == Some("sla") {
            out.push(p);
        }
    }
    out
}
