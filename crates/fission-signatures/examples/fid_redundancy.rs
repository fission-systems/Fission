//! How much do the 57 FID databases share?
//!
//! Each `.fpk` is compressed independently, so a byte sequence common to
//! several databases is paid for once per database. A shared dictionary only
//! pays if that overlap is real, so measure it before designing for it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use fission_signatures::fpk::FpkReader;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../utils/signatures/fid")
        .canonicalize()
        .expect("fid root");

    let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read fid dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".fn.fpk"))
        .collect();
    files.sort();
    println!("fn tables: {}", files.len());

    let mut key_dbs: HashMap<String, u32> = HashMap::new();
    let mut rec_dbs: HashMap<String, u32> = HashMap::new();
    let mut total_records = 0usize;
    let mut total_bytes = 0usize;
    let mut read = 0usize;

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(reader) = FpkReader::open(path) else {
            eprintln!("  skip (open) {name}");
            continue;
        };
        let Ok(lines) = reader.read_all() else {
            eprintln!("  skip (read) {name}");
            continue;
        };
        read += 1;
        total_records += lines.len();

        let mut keys_here = HashSet::new();
        let mut recs_here = HashSet::new();
        for line in &lines {
            total_bytes += line.len() + 1;
            keys_here.insert(line.split('|').next().unwrap_or("").to_string());
            recs_here.insert(line.clone());
        }
        for k in keys_here {
            *key_dbs.entry(k).or_default() += 1;
        }
        for r in recs_here {
            *rec_dbs.entry(r).or_default() += 1;
        }
    }

    let on_disk: u64 = files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();

    println!("databases read     : {read}");
    println!("total records      : {total_records}");
    println!("uncompressed bytes : {:.1} MB", total_bytes as f64 / 1048576.0);
    println!("on disk (.fn.fpk)  : {:.1} MB", on_disk as f64 / 1048576.0);
    println!();

    let dup_keys = key_dbs.values().filter(|&&v| v > 1).count();
    let dup_recs = rec_dbs.values().filter(|&&v| v > 1).count();
    println!("distinct keys      : {}", key_dbs.len());
    println!(
        "  in >1 database   : {dup_keys}  ({:.1}%)",
        dup_keys as f64 / key_dbs.len().max(1) as f64 * 100.0
    );
    println!("distinct records   : {}", rec_dbs.len());
    println!(
        "  in >1 database   : {dup_recs}  ({:.1}%)",
        dup_recs as f64 / rec_dbs.len().max(1) as f64 * 100.0
    );
    println!();

    // A key is a hash, so of course it does not repeat. The question that
    // matters for a dictionary is whether the *text* repeats -- function names
    // like `memcpy` appear in every database that ships a CRT.
    let mut name_dbs: HashMap<String, u32> = HashMap::new();
    for path in &files {
        let Ok(reader) = FpkReader::open(path) else { continue };
        let Ok(lines) = reader.read_all() else { continue };
        let mut here = HashSet::new();
        for line in &lines {
            // records are `hash|...|name|...`; take the longest alphabetic field
            if let Some(best) = line
                .split('|')
                .filter(|f| f.len() > 2 && f.chars().any(|c| c.is_ascii_alphabetic()))
                .max_by_key(|f| f.len())
            {
                here.insert(best.to_string());
            }
        }
        for n in here {
            *name_dbs.entry(n).or_default() += 1;
        }
    }
    let dup_names = name_dbs.values().filter(|&&v| v > 1).count();
    println!(
        "distinct symbol-ish fields: {}  in >1 database: {dup_names} ({:.1}%)",
        name_dbs.len(),
        dup_names as f64 / name_dbs.len().max(1) as f64 * 100.0
    );
    let mut top: Vec<_> = name_dbs.iter().collect();
    top.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
    for (n, v) in top.iter().take(6) {
        println!("   in {v:>3} dbs : {}", &n[..n.len().min(50)]);
    }
    println!();

    let unique_bytes: usize = rec_dbs.keys().map(|r| r.len() + 1).sum();
    println!(
        "if every record were stored once: {:.1} MB of {:.1} MB ({:.1}% is repetition)",
        unique_bytes as f64 / 1048576.0,
        total_bytes as f64 / 1048576.0,
        (1.0 - unique_bytes as f64 / total_bytes.max(1) as f64) * 100.0
    );
}
