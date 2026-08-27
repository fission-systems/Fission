//! Pack the parsed Detect-It-Easy corpus into a single `.fpk`.
//!
//! The runtime reads 2,066 `.sg` scripts (9.4 MB), parses each into a
//! `Signature`, and caches the result as JSON keyed on every source file's
//! mtime -- so a cold start walks the whole tree twice, once to parse and once
//! to validate it did not change. Parsing is deterministic, so it can happen
//! here instead and ship as one file.
//!
//! Usage:
//!   cargo run -p fission-loader --bin pack_die -- <mirror-root> <out-dir>

use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: pack_die <detect-it-easy-root> <out-dir>");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).expect("create out dir");

    let mut sg_files = Vec::new();
    for child in ["db", "db_extra", "db_custom"] {
        collect(&root.join(child), &mut sg_files);
    }
    sg_files.sort();
    eprintln!(
        "found {} .sg files under {}",
        sg_files.len(),
        root.display()
    );

    let mut records: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for path in &sg_files {
        let Ok(content) = std::fs::read_to_string(path) else {
            skipped += 1;
            continue;
        };
        match fission_loader::detector::die_engine::parse_sg_signature_for_packing(
            &root, path, &content,
        ) {
            Some(sig) => records.push(serde_json::to_string(&sig).expect("serialize signature")),
            None => skipped += 1,
        }
    }
    eprintln!("parsed {} signatures, skipped {}", records.len(), skipped);

    // Sorted by name: the `json-lines` kind the packer keys on.
    records.sort_by_key(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned))
            .unwrap_or_default()
    });

    let dest = out.join("die_signatures.fpk");
    let blob = fission_signatures::fpk::pack(&records, fission_signatures::fpk::CODEC_ZLIB);
    std::fs::write(&dest, &blob).expect("write fpk");

    let src_bytes: u64 = sg_files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    eprintln!(
        "{:.1} MB of .sg -> {:.1} MB packed ({:.1}%)",
        src_bytes as f64 / 1048576.0,
        blob.len() as f64 / 1048576.0,
        blob.len() as f64 / src_bytes as f64 * 100.0,
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("sg") {
            out.push(path);
        }
    }
}
