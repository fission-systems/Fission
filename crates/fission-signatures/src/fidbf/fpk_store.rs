//! `.fpk` representation of a parsed FID database.
//!
//! `utils/signatures/fid/` ships 254M of Ghidra containers -- B-tree interior
//! nodes, page padding and a record index keyed by database id. None of that
//! survives loading: `collect_records` walks the tree, discards every interior
//! key, and `FidbfDatabase` then builds its own index keyed by `full_hash`,
//! which is what lookups actually use. The on-disk structure exists so Ghidra
//! can read and WRITE a database incrementally; Fission only ever reads one
//! whole.
//!
//! So the same content is written here as three sorted record tables, each an
//! `.fpk`. Measured over four representative databases, functions pack 4.9x and
//! relations 2.5x, taking `fid/` from 254M to roughly 60M.
//!
//! Every field of every table is preserved. Dropping the relation table would
//! be a larger saving and would also quietly discard what `force_relation`
//! needs -- a full-hash match that is only valid when a parent or child also
//! matches -- and that flag is set on real functions by the database's own
//! build process.

use super::types::{
    FID_ACCEPT_THRESHOLD, FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfMatch, FidbfRelation,
    FidbfRelationType,
};
use crate::fpk::{
    BLOCK_TARGET_BULK, CODEC_ZSTD_COLUMNAR, FpkError, FpkReader, HashEntry, append_hash_index,
    pack_with, pack_with_locators,
};
use std::path::Path;

pub const KIND_FID_LIBRARY: u16 = 10;
pub const KIND_FID_FUNCTION: u16 = 11;
pub const KIND_FID_RELATION: u16 = 12;
pub const KIND_FID_DOMAIN_PATH: u16 = 13;

fn esc(text: &str) -> String {
    // `|` separates fields and `\n` separates records; a library or symbol name
    // containing either would otherwise shift every field after it.
    text.replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace('\n', "\\n")
}

fn unesc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p') => out.push('|'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn relation_code(kind: FidbfRelationType) -> String {
    match kind {
        FidbfRelationType::Call => "c".to_string(),
        FidbfRelationType::Jump => "j".to_string(),
        FidbfRelationType::Inferior => "i".to_string(),
        FidbfRelationType::Superior => "s".to_string(),
        // The raw value is kept so an unknown type round-trips as itself rather
        // than collapsing into a single "unknown".
        FidbfRelationType::Unknown(raw) => format!("u{raw}"),
    }
}

fn relation_kind(code: &str) -> FidbfRelationType {
    match code {
        "c" => FidbfRelationType::Call,
        "j" => FidbfRelationType::Jump,
        "i" => FidbfRelationType::Inferior,
        "s" => FidbfRelationType::Superior,
        other => FidbfRelationType::Unknown(
            other
                .strip_prefix('u')
                .and_then(|v| v.parse().ok())
                .unwrap_or(-1),
        ),
    }
}

/// The `.fpk` images for one database.
pub struct FidFpkImages {
    pub libraries: Vec<u8>,
    pub functions: Vec<u8>,
    pub relations: Vec<u8>,
    /// `domain_path` values, interned. In vs2015_x64 that field averages 177
    /// characters across 80,736 functions -- 14.3M of the record text -- but
    /// takes only 4,275 distinct values totalling 0.76M. Storing an index into
    /// this table instead of the string is what keeps the function table from
    /// being mostly build paths.
    pub domain_paths: Vec<u8>,
}

pub fn encode(db: &FidbfDatabase) -> FidFpkImages {
    // Keys lead every record: `pack` sorts on the text before the first `|`,
    // and zero-padded hex keeps that order numeric.
    let libraries: Vec<String> = db
        .libraries
        .iter()
        .map(|l| {
            format!(
                "{:016x}|{}|{}|{}|{}|{}|{}|{}|{}",
                l.key,
                esc(&l.family_name),
                esc(&l.version),
                esc(&l.variant),
                esc(&l.ghidra_version),
                esc(&l.language_id),
                l.language_version,
                l.language_minor_version,
                esc(&l.compiler_spec_id),
            )
        })
        .collect();

    // Interned in first-seen order; the id is what the function record carries.
    let mut domain_ids: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut domain_list: Vec<&str> = Vec::new();
    for f in &db.functions {
        if !domain_ids.contains_key(f.domain_path.as_str()) {
            domain_ids.insert(f.domain_path.as_str(), domain_list.len());
            domain_list.push(f.domain_path.as_str());
        }
    }
    let domain_paths: Vec<String> = domain_list
        .iter()
        .enumerate()
        .map(|(id, path)| format!("{id:08x}|{}", esc(path)))
        .collect();

    let functions: Vec<String> = db
        .functions
        .iter()
        .map(|f| {
            format!(
                "{:016x}|{}|{:x}|{:x}|{:x}|{:x}|{:x}|{}|{}|{}|{}|{}|{}|{}",
                f.key,
                esc(&f.name),
                f.library_id,
                f.full_hash,
                f.specific_hash,
                f.code_unit_size,
                f.entry_point,
                u8::from(f.has_terminator),
                f.specific_hash_additional_size,
                domain_ids.get(f.domain_path.as_str()).copied().unwrap_or(0),
                f.flags,
                u8::from(f.auto_pass),
                u8::from(f.auto_fail),
                u8::from(f.force_specific) * 2 + u8::from(f.force_relation),
            )
        })
        .collect();

    let relations: Vec<String> = db
        .relations
        .iter()
        .map(|r| {
            format!(
                "{:016x}:{:016x}|{}",
                r.function_id,
                r.related_id,
                relation_code(r.relation_type)
            )
        })
        .collect();

    // Sorted by symbol name (column 1), not by database key. FID is always
    // read whole and indexed by hash afterwards, so the block index is not a
    // lookup path here -- and name order is what lets neighbouring records
    // share prefixes, which is worth more than every other choice combined.
    let bulk = |records: &Vec<String>, kind: u16, sort_field: usize| {
        pack_with(
            records,
            kind,
            CODEC_ZSTD_COLUMNAR,
            sort_field,
            BLOCK_TARGET_BULK,
        )
    };

    // The function table carries a `full_hash -> record` index so a match can
    // be answered without decoding the table. `functions[i]` was built from
    // `db.functions[i]`, so the locator and the hash line up by position.
    let (mut functions_image, locators) = pack_with_locators(
        &functions,
        KIND_FID_FUNCTION,
        CODEC_ZSTD_COLUMNAR,
        1,
        BLOCK_TARGET_BULK,
    );
    let entries: Vec<HashEntry> = db
        .functions
        .iter()
        .zip(&locators)
        .map(|(f, l)| HashEntry {
            key: f.full_hash,
            block: l.block,
            row: l.row,
        })
        .collect();
    append_hash_index(&mut functions_image, entries);

    FidFpkImages {
        libraries: bulk(&libraries, KIND_FID_LIBRARY, 1),
        functions: functions_image,
        relations: bulk(&relations, KIND_FID_RELATION, 0),
        domain_paths: bulk(&domain_paths, KIND_FID_DOMAIN_PATH, 1),
    }
}

fn parse_u64_hex(text: &str) -> u64 {
    u64::from_str_radix(text, 16).unwrap_or(0)
}

fn parse_i64_hex(text: &str) -> i64 {
    u64::from_str_radix(text, 16).unwrap_or(0) as i64
}

/// Rebuild a database from the three images written by [`encode`].
pub fn decode(
    source_path: String,
    libraries_fpk: &Path,
    functions_fpk: &Path,
    relations_fpk: &Path,
    domain_paths_fpk: &Path,
) -> Result<FidbfDatabase, FpkError> {
    let mut domains: Vec<String> = Vec::new();
    for line in FpkReader::open(domain_paths_fpk)?.read_all()? {
        let Some((id, path)) = line.split_once('|') else {
            continue;
        };
        let id = usize::from_str_radix(id, 16).unwrap_or(0);
        if domains.len() <= id {
            domains.resize(id + 1, String::new());
        }
        domains[id] = unesc(path);
    }

    let mut libraries = Vec::new();
    for line in FpkReader::open(libraries_fpk)?.read_all()? {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 9 {
            continue;
        }
        libraries.push(FidbfLibrary {
            key: parse_i64_hex(f[0]),
            family_name: unesc(f[1]),
            version: unesc(f[2]),
            variant: unesc(f[3]),
            ghidra_version: unesc(f[4]),
            language_id: unesc(f[5]),
            language_version: f[6].parse().unwrap_or(0),
            language_minor_version: f[7].parse().unwrap_or(0),
            compiler_spec_id: unesc(f[8]),
        });
    }

    let mut functions = Vec::new();
    for line in FpkReader::open(functions_fpk)?.read_all()? {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 14 {
            continue;
        }
        let forced: u8 = f[13].parse().unwrap_or(0);
        functions.push(FidbfFunction {
            key: parse_i64_hex(f[0]),
            name: unesc(f[1]),
            library_id: parse_i64_hex(f[2]),
            full_hash: parse_u64_hex(f[3]),
            specific_hash: parse_u64_hex(f[4]),
            code_unit_size: parse_u64_hex(f[5]) as u32,
            entry_point: parse_u64_hex(f[6]),
            has_terminator: f[7] == "1",
            specific_hash_additional_size: f[8].parse().unwrap_or(0),
            domain_path: f[9]
                .parse::<usize>()
                .ok()
                .and_then(|id| domains.get(id).cloned())
                .unwrap_or_default(),
            flags: f[10].parse().unwrap_or(0),
            auto_pass: f[11] == "1",
            auto_fail: f[12] == "1",
            force_specific: forced & 2 != 0,
            force_relation: forced & 1 != 0,
        });
    }

    let mut relations = Vec::new();
    for line in FpkReader::open(relations_fpk)?.read_all()? {
        let Some((ids, code)) = line.split_once('|') else {
            continue;
        };
        let Some((function_id, related_id)) = ids.split_once(':') else {
            continue;
        };
        relations.push(FidbfRelation {
            function_id: parse_i64_hex(function_id),
            related_id: parse_i64_hex(related_id),
            relation_type: relation_kind(code),
        });
    }

    Ok(FidbfDatabase::new(
        source_path,
        libraries,
        functions,
        relations,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// Every field of every table has to survive, so this compares them all
    /// rather than a summary: a re-encoding that lost `auto_fail` would still
    /// match on counts and hashes.
    #[test]
    fn a_real_database_round_trips_field_for_field() {
        let src =
            Path::new("/Users/sjkim1127/Fission/utils/signatures/fid/libc-x86.LE.64.default.fidbf");
        let Ok(original) = crate::fidbf::parse_fidbf(src) else {
            return; // bundle not present in this checkout
        };

        let images = encode(&original);
        let dir = std::env::temp_dir().join(format!("fidfpk_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let l = write(&dir, "lib.fpk", &images.libraries);
        let f = write(&dir, "fn.fpk", &images.functions);
        let r = write(&dir, "rel.fpk", &images.relations);
        let d = write(&dir, "dom.fpk", &images.domain_paths);

        let back = decode(original.source_path.clone(), &l, &f, &r, &d).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(back.libraries.len(), original.libraries.len());
        assert_eq!(back.functions.len(), original.functions.len());
        assert_eq!(back.relations.len(), original.relations.len());

        let mut a: Vec<String> = original
            .functions
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let mut b: Vec<String> = back.functions.iter().map(|x| format!("{x:?}")).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "function fields differ");

        let mut a: Vec<String> = original
            .libraries
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let mut b: Vec<String> = back.libraries.iter().map(|x| format!("{x:?}")).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "library fields differ");

        let mut a: Vec<String> = original
            .relations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let mut b: Vec<String> = back.relations.iter().map(|x| format!("{x:?}")).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "relation fields differ");
    }

    /// The two databases that an empty relation table used to reject.
    #[test]
    fn a_database_with_no_relations_parses() {
        for stem in ["gcc-MIPS.BE.32.default", "gcc-avr8.LE.16.extended"] {
            let path = std::path::PathBuf::from(format!(
                "/Users/sjkim1127/Fission/utils/signatures/fid/{stem}.fidbf"
            ));
            if !path.exists() {
                continue; // bundle not present in this checkout
            }
            let db = crate::fidbf::parse_fidbf(&path)
                .unwrap_or_else(|e| panic!("{stem} should parse: {e}"));
            assert!(!db.functions.is_empty(), "{stem} has functions");
            assert!(
                db.relations.is_empty(),
                "{stem} is the empty-relations case"
            );
        }
    }

    #[test]
    fn separators_inside_names_survive() {
        // A symbol containing `|` or a newline would otherwise shift every
        // field after it, silently.
        for raw in [
            "plain",
            "has|pipe",
            "has\nnewline",
            "back\\slash",
            "all|of\nthem\\",
        ] {
            assert_eq!(unesc(&esc(raw)), raw, "escaping lost {raw:?}");
        }
    }
}

// ── Lazy lookup ─────────────────────────────────────────────────────────────

/// A FID database answered from its `.fpk` tables without being built.
///
/// `identify_by_hashes` is the only question either caller asks, and the
/// function table's `full_hash` index answers it directly: a miss touches
/// mapped bytes and nothing else, a hit decodes the one block its candidates
/// live in. Building `FidbfDatabase` to answer the same question costs 65ms of
/// decode, parse and index rebuild, per process, before the first query.
///
/// Library rows are small and are read once, because a match needs its family
/// name. `domain_path` is not read at all: nothing outside the parser uses it.
pub struct LazyFidDatabase {
    functions: FpkReader,
    libraries: Vec<FidbfLibrary>,
    source_path: String,
}

impl LazyFidDatabase {
    /// Open the tables beside `path`, or `None` when they are not all present.
    pub fn open(path: &Path) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let table = |suffix: &str| dir.join(format!("{stem}.{suffix}.fpk"));
        let functions = FpkReader::open(&table("fn")).ok()?;
        if !functions.has_hash_index() {
            return None;
        }
        let libraries_reader = FpkReader::open(&table("lib")).ok()?;
        let mut libraries = Vec::new();
        for line in libraries_reader.read_all().ok()? {
            if let Some(library) = decode_library(&line) {
                libraries.push(library);
            }
        }
        Some(Self {
            functions,
            libraries,
            source_path: path.display().to_string(),
        })
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    /// Whether the database carries any library row at all.
    ///
    /// `libraries` is read once on open, so an empty vector means the `.lib`
    /// table was empty or unreadable -- a database that resolved but carries
    /// nothing.
    pub fn has_any_library(&self) -> bool {
        !self.libraries.is_empty()
    }

    /// Whether any library here matches `language_id`, the check
    /// `discover_for_load_spec` makes before keeping a database.
    pub fn has_language(&self, language_id: &str) -> bool {
        self.libraries
            .iter()
            .any(|l| l.language_id.is_empty() || l.language_id == language_id)
    }

    /// Same contract as [`FidbfDatabase::identify_by_hashes`], including the
    /// auto_fail / auto_pass / force_specific / force_relation rules.
    pub fn identify_by_hashes(&self, full_hash: u64, specific_hash: u64) -> Vec<FidbfMatch> {
        let Ok(records) = self.functions.records_by_key(full_hash) else {
            return Vec::new();
        };
        let mut results: Vec<FidbfMatch> = records
            .iter()
            .filter_map(|line| decode_function_for_match(line))
            .filter(|f| !f.auto_fail)
            .filter(|f| !f.force_relation)
            .filter(|f| !f.force_specific || f.specific_hash == specific_hash)
            .filter_map(|f| {
                let score = (f.code_unit_size as f32
                    + if f.specific_hash == specific_hash {
                        10.0
                    } else {
                        0.0
                    })
                .min(100.0);
                if !f.auto_pass && score < FID_ACCEPT_THRESHOLD {
                    return None;
                }
                let family = self
                    .libraries
                    .iter()
                    .find(|l| l.key == f.library_id)
                    .map(|l| l.family_name.clone())
                    .unwrap_or_default();
                Some(FidbfMatch {
                    name: f.name,
                    library_family: family,
                    score,
                    specific_matched: f.specific_hash == specific_hash,
                })
            })
            .collect();
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results
    }
}

fn decode_library(line: &str) -> Option<FidbfLibrary> {
    let f: Vec<&str> = line.split('|').collect();
    if f.len() < 9 {
        return None;
    }
    Some(FidbfLibrary {
        key: parse_i64_hex(f[0]),
        family_name: unesc(f[1]),
        version: unesc(f[2]),
        variant: unesc(f[3]),
        ghidra_version: unesc(f[4]),
        language_id: unesc(f[5]),
        language_version: f[6].parse().unwrap_or(0),
        language_minor_version: f[7].parse().unwrap_or(0),
        compiler_spec_id: unesc(f[8]),
    })
}

/// The fields a match needs. `domain_path` stays an id: resolving it would mean
/// opening another table for something no caller reads.
fn decode_function_for_match(line: &str) -> Option<FidbfFunction> {
    let f: Vec<&str> = line.split('|').collect();
    if f.len() < 14 {
        return None;
    }
    let forced: u8 = f[13].parse().unwrap_or(0);
    Some(FidbfFunction {
        key: parse_i64_hex(f[0]),
        name: unesc(f[1]),
        library_id: parse_i64_hex(f[2]),
        full_hash: parse_u64_hex(f[3]),
        specific_hash: parse_u64_hex(f[4]),
        code_unit_size: parse_u64_hex(f[5]) as u32,
        entry_point: parse_u64_hex(f[6]),
        has_terminator: f[7] == "1",
        specific_hash_additional_size: f[8].parse().unwrap_or(0),
        domain_path: String::new(),
        flags: f[10].parse().unwrap_or(0),
        auto_pass: f[11] == "1",
        auto_fail: f[12] == "1",
        force_specific: forced & 2 != 0,
        force_relation: forced & 1 != 0,
    })
}
