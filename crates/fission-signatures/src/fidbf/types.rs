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
    pub function_id: i64,
    pub related_id: i64,
    pub relation_type: FidbfRelationType,
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
        };
        db.build_hash_index();
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

    /// Identify a function by its dual FID hashes and return matching library
    /// function names.  Only returns matches with a score above `FID_ACCEPT_THRESHOLD`.
    ///
    /// Results are sorted by score descending.
    pub fn identify_by_hashes(&self, full_hash: u64, specific_hash: u64) -> Vec<FidbfMatch> {
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
            .filter(|func| !func.force_relation)
            .filter(|func| !func.force_specific || func.specific_hash == specific_hash)
            .filter_map(|func| {
                let score = self.score_match(func, specific_hash);
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
        assert!((tiny as f32) < FID_ACCEPT_THRESHOLD, "test needs a sub-threshold size");
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
}
