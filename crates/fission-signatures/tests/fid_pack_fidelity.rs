//! The packed `.fpk` tables must answer exactly what the `.fidbf` they were
//! built from answers.
//!
//! Written while FID was matching nothing on any binary. A packing that
//! dropped or mangled records would produce that symptom no matter how the
//! hashes are computed, so it had to be ruled out before blaming the hashing
//! side -- and this keeps it ruled out.
//!
//! Skips when `utils/source/fid/` is absent: the source databases are not
//! committed (only the packed tables are), so this runs on a developer
//! checkout that has them and is skipped everywhere else.
use fission_signatures::fidbf::fpk_store::LazyFidDatabase;
use fission_signatures::fidbf::parse_fidbf;
use std::path::PathBuf;

fn utils_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../utils")
}

#[test]
fn packed_tables_answer_what_the_source_database_answers() {
    let root = utils_root();
    let mut checked = 0usize;
    for db in [
        "gcc-x86.LE.64.default",
        "libc-x86.LE.64.default",
        "libc-ARM.LE.32.v8",
        "gcc-ARM.LE.32.v8",
    ] {
        let source = root.join(format!("source/fid/{db}.fidbf"));
        if !source.exists() {
            continue;
        }
        let eager = parse_fidbf(&source).expect("source database parses");
        // `LazyFidDatabase::open` derives `<stem>.fn.fpk` itself, so it takes
        // the stem-bearing path the resource provider hands it rather than a
        // table path.
        let lazy = LazyFidDatabase::open(&root.join(format!("signatures/fid/{db}.fidbf")))
            .unwrap_or_else(|| panic!("{db}: packed tables open"));

        // Sampled across the whole table rather than off the front, so a
        // truncation and a corruption do not look alike.
        let step = (eager.functions.len() / 200).max(1);
        for f in eager.functions.iter().step_by(step).take(200) {
            let from_source = eager.identify_by_hashes(f.full_hash, f.specific_hash);
            let from_packed = lazy.identify_by_hashes(f.full_hash, f.specific_hash);
            assert_eq!(
                from_source.is_empty(),
                from_packed.is_empty(),
                "{db}: {} answered by one form and not the other",
                f.name
            );
            if from_source.is_empty() {
                continue;
            }
            let source_names: Vec<&str> = from_source.iter().map(|m| m.name.as_str()).collect();
            let packed_names: Vec<&str> = from_packed.iter().map(|m| m.name.as_str()).collect();
            assert!(
                packed_names.contains(&f.name.as_str()),
                "{db}: packed lost {}; got {packed_names:?} against {source_names:?}",
                f.name
            );
        }
        checked += 1;
    }
    if checked == 0 {
        eprintln!("skipped: utils/source/fid/ has no source databases in this checkout");
    }
}
