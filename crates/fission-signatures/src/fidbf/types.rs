#[derive(Debug, Clone)]
pub struct FidbfLibrary {
    pub key: i64,
    pub family_name: String,
    pub version: String,
    pub variant: String,
    pub ghidra_version: String,
    pub language_id: String,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FidbfRelationType {
    Call,
    Jump,
    Unknown(i32),
}

impl From<i32> for FidbfRelationType {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Call,
            1 => Self::Jump,
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
            .filter_map(|func| {
                let score = self.score_match(func, specific_hash);
                if score >= FID_ACCEPT_THRESHOLD {
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
