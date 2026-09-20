use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct FidbfLibrary {
    pub key: i64,
    pub family_name: String,
    pub version: String,
    pub variant: String,
    pub ghidra_version: String,
    pub language_id: String,
    pub language_version: i32,
    pub language_minor_version: i32,
    pub compiler_spec_id: String,
}

#[derive(Debug, Clone)]
pub struct FidbfFunction {
    pub key: i64,
    pub library_id: i64,
    pub name: String,
    pub full_hash: u64,
    pub specific_hash: u64,
    pub code_unit_size: u32,
    pub entry_point: u64,
    pub has_terminator: bool,
    pub specific_hash_additional_size: u8,
    pub domain_path: String,
    pub flags: u8,
    pub auto_pass: bool,
    pub auto_fail: bool,
    pub force_specific: bool,
    pub force_relation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FidbfRelationType {
    Call,
    Jump,
    Inferior,
    Superior,
    Unknown(i32),
}

impl From<i32> for FidbfRelationType {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Call,
            1 => Self::Jump,
            2 => Self::Inferior,
            3 => Self::Superior,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FidbfRelation {
    /// Ghidra's relation-smash key.  Relation tables intentionally have no
    /// columns: the key combines one function id with the other function's
    /// full hash (see `FidDBUtils.generate*FullHashSmash`).
    pub key: u64,
    pub relation_type: FidbfRelationType,
}

/// Hash neighbourhood of a function in the loaded program.
///
/// FID relation records do not identify a callee by address.  They identify
/// the caller/callee pair by the candidate database key and the neighbouring
/// function's full hash, so this is the smallest context the matcher needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct FidRelationContext<'a> {
    pub children: &'a [(u64, u16)],
    pub parents: &'a [(u64, u16)],
}

/// Score above which a FID match is considered high-confidence (mirrors Ghidra's
/// default threshold of ~14.6 normalised points, scaled here to 0–100 integers).
pub const FID_ACCEPT_THRESHOLD: f32 = 14.6;

#[derive(Debug, Clone)]
pub struct FidbfDatabase {
    pub source_path: String,
    pub libraries: Vec<FidbfLibrary>,
    pub functions: Vec<FidbfFunction>,
    pub relations: Vec<FidbfRelation>,
    /// Pre-built index: `full_hash` → indices into `functions`.
    /// Empty until `build_hash_index` is called (done automatically by the
    /// `parse_fidbf` loader).
    full_hash_index: std::collections::HashMap<u64, Vec<usize>>,
    relation_index: RelationIndex,
}

#[derive(Debug, Clone, Default)]
struct RelationIndex {
    inferior: HashSet<u64>,
    superior: HashSet<u64>,
}

impl RelationIndex {
    fn from_relations(relations: &[FidbfRelation]) -> Self {
        let mut index = Self::default();
        for relation in relations {
            match relation.relation_type {
                FidbfRelationType::Inferior => {
                    index.inferior.insert(relation.key);
                }
                FidbfRelationType::Superior => {
                    index.superior.insert(relation.key);
                }
                // Call/Jump are not emitted by the current raw-table parser,
                // but keeping them out of the FID hash-smash index prevents an
                // unknown relation encoding from becoming an acceptance path.
                FidbfRelationType::Call
                | FidbfRelationType::Jump
                | FidbfRelationType::Unknown(_) => {}
            }
        }
        index
    }
}

const FNV_64_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) fn relation_smash(function_key: i64, other_full_hash: u64) -> u64 {
    (function_key as u64).wrapping_mul(FNV_64_PRIME) ^ other_full_hash
}

impl FidbfDatabase {
    /// Create a new (empty) database with no index.
    pub fn new(
        source_path: String,
        libraries: Vec<FidbfLibrary>,
        functions: Vec<FidbfFunction>,
        relations: Vec<FidbfRelation>,
    ) -> Self {
        let mut db = Self {
            source_path,
            libraries,
            functions,
            relations,
            full_hash_index: std::collections::HashMap::new(),
            relation_index: RelationIndex::default(),
        };
        db.build_hash_index();
        db.relation_index = RelationIndex::from_relations(&db.relations);
        db
    }

    /// Build (or rebuild) the full-hash → function-index lookup table.
    pub fn build_hash_index(&mut self) {
        self.full_hash_index.clear();
        for (idx, func) in self.functions.iter().enumerate() {
            self.full_hash_index
                .entry(func.full_hash)
                .or_default()
                .push(idx);
        }
    }

    pub fn library_by_id(&self, id: i64) -> Option<&FidbfLibrary> {
        self.libraries.iter().find(|library| library.key == id)
    }

    /// Look up functions by their **full hash** (O(1) via pre-built index).
    pub fn find_by_full_hash(&self, full_hash: u64) -> Vec<&FidbfFunction> {
        match self.full_hash_index.get(&full_hash) {
            Some(indices) => indices.iter().map(|&i| &self.functions[i]).collect(),
            None => Vec::new(),
        }
    }

    /// Look up functions by their **specific hash**.
    pub fn functions_by_specific_hash(&self, hash: u64) -> Vec<&FidbfFunction> {
        self.functions
            .iter()
            .filter(|function| function.specific_hash == hash)
            .collect()
    }

    /// Score a candidate match against a query's specific hash.
    ///
    /// Returns a value in `[0.0, 100.0]`.  A score ≥ `FID_ACCEPT_THRESHOLD` is
    /// considered acceptable (mirrors Ghidra's `14.6f` threshold).
    ///
    /// Scoring logic (simplified from Ghidra `FidMatchScore`):
    /// - Base: `codeUnitSize` points (function size contribution)
    /// - Bonus: +10 if `specific_hash` also matches
    /// - Cap: 100
    pub fn score_match(&self, func: &FidbfFunction, specific_hash: u64) -> f32 {
        let base = func.code_unit_size as f32;
        let bonus = if func.specific_hash == specific_hash {
            10.0
        } else {
            0.0
        };
        (base + bonus).min(100.0)
    }

    fn hash_candidate_is_eligible(&self, func: &FidbfFunction, specific_hash: u64) -> bool {
        !func.auto_fail && (!func.force_specific || func.specific_hash == specific_hash)
    }

    fn relation_scores(&self, func: &FidbfFunction, context: FidRelationContext<'_>) -> (u32, u32) {
        // Ghidra's HashFamily de-duplicates neighbouring functions by full
        // hash before scoring.  Do the same here so repeated call sites do not
        // inflate a match's relation score.
        let mut child_hashes = HashSet::new();
        let child_score = context
            .children
            .iter()
            .filter(|(hash, _)| child_hashes.insert(*hash))
            .filter(|(hash, _)| {
                self.relation_index
                    .superior
                    .contains(&relation_smash(func.key, *hash))
            })
            .map(|(_, code_units)| u32::from(*code_units))
            .sum();

        let mut parent_hashes = HashSet::new();
        let parent_score = context
            .parents
            .iter()
            .filter(|(hash, _)| parent_hashes.insert(*hash))
            .filter(|(hash, _)| {
                self.relation_index
                    .inferior
                    .contains(&relation_smash(func.key, *hash))
            })
            .map(|(_, code_units)| u32::from(*code_units))
            .sum();

        (child_score, parent_score)
    }

    /// Whether a full-hash lookup contains an eligible forced-relation
    /// candidate.  This lets callers defer building a whole-binary call graph
    /// unless the current program actually needs relation context.
    pub fn has_force_relation_candidate(&self, full_hash: u64, specific_hash: u64) -> bool {
        self.find_by_full_hash(full_hash).into_iter().any(|func| {
            func.force_relation
                && self.hash_candidate_is_eligible(func, specific_hash)
                && (func.auto_pass || self.score_match(func, specific_hash) >= FID_ACCEPT_THRESHOLD)
        })
    }

    /// Identify a function by its dual FID hashes and return matching library
    /// function names.  Only returns matches with a score above `FID_ACCEPT_THRESHOLD`.
    ///
    /// Results are sorted by score descending.
    pub fn identify_by_hashes(&self, full_hash: u64, specific_hash: u64) -> Vec<FidbfMatch> {
        self.identify_by_hashes_with_relations(
            full_hash,
            specific_hash,
            FidRelationContext::default(),
        )
    }

    /// Identify a function and score the candidate against its caller/callee
    /// hash neighbourhood.  Forced-relation candidates are accepted only when
    /// at least one known child satisfies the database's superior relation.
    pub fn identify_by_hashes_with_relations(
        &self,
        full_hash: u64,
        specific_hash: u64,
        context: FidRelationContext<'_>,
    ) -> Vec<FidbfMatch> {
        let mut results: Vec<FidbfMatch> = self
            .find_by_full_hash(full_hash)
            .into_iter()
            // Ghidra's own build process sets these, and `building_fid.txt`
            // states what they mean. Auto-fail is "a full-hash match will not be
            // returned under any circumstances" -- it marks hashes known to
            // collide across unrelated functions -- and it went unchecked here,
            // so 38,465 of the corpus's 1,832,079 functions (2.10%) were
            // returnable when the database says they never are.
            .filter(|func| !func.auto_fail)
            .filter(|func| self.hash_candidate_is_eligible(func, specific_hash))
            .filter_map(|func| {
                let (child_score, parent_score) = self.relation_scores(func, context);
                if func.force_relation && child_score == 0 {
                    return None;
                }
                let score = (self.score_match(func, specific_hash)
                    + child_score as f32
                    + parent_score as f32)
                    .min(100.0);
                // Auto-pass is "a full-hash match is always returned, even if
                // the function is tiny", which is exactly a waiver of the size
                // threshold: all 156 auto-pass functions in the corpus score
                // below it, so every one of them was being dropped.
                if func.auto_pass || score >= FID_ACCEPT_THRESHOLD {
                    let library = self.library_by_id(func.library_id);
                    Some(FidbfMatch {
                        name: func.name.clone(),
                        library_family: library.map(|l| l.family_name.clone()).unwrap_or_default(),
                        score,
                        specific_matched: func.specific_hash == specific_hash,
                    })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

/// A single match returned by `FidbfDatabase::identify_by_hashes`.
#[derive(Debug, Clone)]
pub struct FidbfMatch {
    /// Library function name (e.g. `"memcpy"`).
    pub name: String,
    /// Library family name (e.g. `"VS2019"`).
    pub library_family: String,
    /// Computed match score (0–100).
    pub score: f32,
    /// Whether the specific hash also matched (higher confidence).
    pub specific_matched: bool,
}

#[cfg(test)]
mod flag_tests {
    use super::*;

    fn func(name: &str, full: u64, specific: u64, size: u32) -> FidbfFunction {
        FidbfFunction {
            key: 1,
            library_id: 1,
            name: name.to_string(),
            full_hash: full,
            specific_hash: specific,
            code_unit_size: size,
            entry_point: 0,
            has_terminator: true,
            specific_hash_additional_size: 0,
            domain_path: String::new(),
            flags: 0,
            auto_pass: false,
            auto_fail: false,
            force_specific: false,
            force_relation: false,
        }
    }

    fn db(functions: Vec<FidbfFunction>) -> FidbfDatabase {
        FidbfDatabase::new(
            "test".to_string(),
            vec![FidbfLibrary {
                key: 1,
                family_name: "TEST".to_string(),
                version: String::new(),
                variant: String::new(),
                ghidra_version: String::new(),
                language_id: String::new(),
                language_version: 0,
                language_minor_version: 0,
                compiler_spec_id: String::new(),
            }],
            functions,
            vec![],
        )
    }

    /// `building_fid.txt`: "Auto-fail means a full-hash match will not be
    /// returned under any circumstances (even though the function is still in
    /// the database)." 38,465 corpus functions carry it.
    #[test]
    fn auto_fail_is_never_returned() {
        let mut f = func("collides", 0xabc, 0xdef, 100);
        f.auto_fail = true;
        let db = db(vec![f]);
        assert!(db.identify_by_hashes(0xabc, 0xdef).is_empty());
    }

    /// "Auto-pass means a full-hash match is always returned, even if the
    /// function is tiny" -- a waiver of the size threshold, which every one of
    /// the corpus's 156 auto-pass functions falls below.
    #[test]
    fn auto_pass_is_returned_below_the_size_threshold() {
        let tiny = 4u32;
        assert!(
            (tiny as f32) < FID_ACCEPT_THRESHOLD,
            "test needs a sub-threshold size"
        );
        let plain = db(vec![func("tiny", 0xabc, 0xdef, tiny)]);
        assert!(plain.identify_by_hashes(0xabc, 0xdef).is_empty());

        let mut passing = func("tiny", 0xabc, 0xdef, tiny);
        passing.auto_pass = true;
        let db = db(vec![passing]);
        assert_eq!(db.identify_by_hashes(0xabc, 0xdef).len(), 1);
    }

    /// Auto-fail outranks auto-pass: "under any circumstances".
    #[test]
    fn auto_fail_beats_auto_pass() {
        let mut f = func("both", 0xabc, 0xdef, 4);
        f.auto_pass = true;
        f.auto_fail = true;
        let db = db(vec![f]);
        assert!(db.identify_by_hashes(0xabc, 0xdef).is_empty());
    }

    /// force_specific was already implemented; pinned so the added filters do
    /// not disturb it.
    #[test]
    fn force_specific_still_requires_the_specific_hash() {
        let mut f = func("strict", 0xabc, 0xdef, 100);
        f.force_specific = true;
        let db = db(vec![f]);
        assert_eq!(db.identify_by_hashes(0xabc, 0xdef).len(), 1);
        assert!(db.identify_by_hashes(0xabc, 0x999).is_empty());
    }

    #[test]
    fn forced_relation_requires_the_matching_child_hash() {
        let mut candidate = func("relation", 0xabc, 0xdef, 20);
        candidate.key = 7;
        candidate.force_relation = true;
        let child_hash = 0x1234_u64;
        let relation = FidbfRelation {
            key: relation_smash(candidate.key, child_hash),
            relation_type: FidbfRelationType::Superior,
        };
        let database = FidbfDatabase::new(
            "test".to_string(),
            vec![FidbfLibrary {
                key: 1,
                family_name: "TEST".to_string(),
                version: String::new(),
                variant: String::new(),
                ghidra_version: String::new(),
                language_id: String::new(),
                language_version: 0,
                language_minor_version: 0,
                compiler_spec_id: String::new(),
            }],
            vec![candidate],
            vec![relation],
        );

        assert!(database.identify_by_hashes(0xabc, 0xdef).is_empty());
        let context = FidRelationContext {
            children: &[(child_hash, 4)],
            parents: &[],
        };
        let matches = database.identify_by_hashes_with_relations(0xabc, 0xdef, context);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].score > 20.0);
    }
}
