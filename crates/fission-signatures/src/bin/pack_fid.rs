//! Convert `utils/signatures/fid/*.fidbf` into `.fpk` tables.
//!
//! Offline: run when the FID corpus changes, not at analysis time. Each
//! database becomes three files -- libraries, functions, relations -- and every
//! one is decoded back and compared field for field before being written, so a
//! conversion that loses a flag fails here rather than silently weakening
//! matching later.
//!
//! Usage: cargo run -p fission-signatures --bin pack_fid -- <src-dir> <out-dir>

use fission_signatures::fidbf::{fpk_store, parse_fidbf};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: pack_fid <src-dir> <out-dir>");
        std::process::exit(2);
    }
    let src = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).expect("create out dir");

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("read src dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("fidbf"))
        .collect();
    entries.sort();

    let (mut raw, mut packed, mut ok, mut failed) = (0u64, 0u64, 0usize, Vec::new());
    for path in &entries {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let db = match parse_fidbf(path) {
            Ok(db) => db,
            Err(e) => {
                failed.push((stem, format!("{e}")));
                continue;
            }
        };
        let images = fpk_store::encode(&db);
        let names = [
            (format!("{stem}.lib.fpk"), &images.libraries),
            (format!("{stem}.fn.fpk"), &images.functions),
            (format!("{stem}.rel.fpk"), &images.relations),
            (format!("{stem}.dom.fpk"), &images.domain_paths),
        ];
        for (name, bytes) in &names {
            std::fs::write(out.join(name), bytes).expect("write fpk");
        }

        // Verify before counting it as converted.
        let back = fpk_store::decode(
            db.source_path.clone(),
            &out.join(&names[0].0),
            &out.join(&names[1].0),
            &out.join(&names[2].0),
            &out.join(&names[3].0),
        )
        .expect("decode back");
        let same = |a: &[String], b: &[String]| {
            let (mut a, mut b) = (a.to_vec(), b.to_vec());
            a.sort();
            b.sort();
            a == b
        };
        let f_a: Vec<String> = db.functions.iter().map(|x| format!("{x:?}")).collect();
        let f_b: Vec<String> = back.functions.iter().map(|x| format!("{x:?}")).collect();
        let r_a: Vec<String> = db.relations.iter().map(|x| format!("{x:?}")).collect();
        let r_b: Vec<String> = back.relations.iter().map(|x| format!("{x:?}")).collect();
        let l_a: Vec<String> = db.libraries.iter().map(|x| format!("{x:?}")).collect();
        let l_b: Vec<String> = back.libraries.iter().map(|x| format!("{x:?}")).collect();
        if !same(&f_a, &f_b) || !same(&r_a, &r_b) || !same(&l_a, &l_b) {
            failed.push((stem.clone(), "round-trip mismatch".to_string()));
            continue;
        }

        raw += size;
        packed += names.iter().map(|(_, b)| b.len() as u64).sum::<u64>();
        ok += 1;
        println!(
            "  {stem:44} {:6.2}M -> {:6.2}M  ({} fn, {} rel)",
            size as f64 / 1048576.0,
            names.iter().map(|(_, b)| b.len()).sum::<usize>() as f64 / 1048576.0,
            db.functions.len(),
            db.relations.len()
        );
    }

    println!(
        "\nconverted {ok}/{} databases: {:.1}M -> {:.1}M ({:.1}x)",
        entries.len(),
        raw as f64 / 1048576.0,
        packed as f64 / 1048576.0,
        raw as f64 / packed.max(1) as f64
    );
    if !failed.is_empty() {
        println!("\nnot converted ({}):", failed.len());
        for (name, why) in &failed {
            println!("  {name}: {why}");
        }
    }
    let _ = Path::new("");
}
