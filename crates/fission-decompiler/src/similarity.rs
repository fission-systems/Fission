//! Fuzzy function similarity: a Fission-native analog of Ghidra's BSim.
//!
//! Ghidra's BSim identifies functions that are structurally similar but not
//! byte-identical (different compiler/optimization level, minor patches
//! between versions) by turning each function into a sparse "feature vector"
//! -- a multiset of 32-bit hashes describing small pieces of its data-flow
//! and control-flow graph -- and comparing vectors with a TF-IDF-weighted
//! cosine-like similarity. See `signature.cc`/`signature.hh` in Ghidra's
//! decompiler backend for the feature-generation algorithm this is inspired
//! by, and `generic/lsh/vector/LSHCosineVector.java` for the comparison
//! formula this reimplements exactly (the `min(w1,w2)^2` merge-join, not a
//! plain dot product -- see [`compare`]'s doc comment).
//!
//! This is a scoped, Fission-native design, not a byte-compatible BSim
//! clone: feature extraction here is a simplified iterative structural hash
//! over `PcodeOp`/`PcodeBasicBlock` (this crate's own IR, not Ghidra's
//! Varnode graph), so it cannot read or write real `.bsim` databases or
//! query Ghidra's BSim servers. What it reproduces is the *shape* of the
//! approach -- local and k-hop data-flow fingerprints + control-flow fingerprints,
//! collected into a sparse multiset, compared with document-frequency-aware
//! weighting -- which is the part that actually finds similar functions.
//!
//! IDF weighting is corpus-relative (computed from whatever functions have
//! been added to a [`SimilarityCorpus`]), unlike Ghidra's BSim which ships
//! a pre-trained weight table from a large reference corpus -- there's no
//! equivalent reference corpus here, and a self-consistent corpus-relative
//! IDF is the standard fallback for any TF-IDF-style scheme without one.

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
};

use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, Varnode};
use serde::{Deserialize, Serialize};

/// Version of the on-disk feature format and feature-extraction contract.
pub const SIMILARITY_INDEX_VERSION: u32 = 2;

/// Sentinel hash used in place of a "no definer inside this function"
/// neighbor (an external register/memory read) during data-flow refinement.
const LEAF_SENTINEL: u64 = 0x4c45_4146_4c45_4146; // "LEAFLEAF" in ASCII hex

/// Number of data-flow hash-refinement rounds (k-hop neighborhood radius for
/// [`PcodeOp`] features). Mirrors the spirit of Ghidra's `maxiter` setting.
const DEFAULT_DATAFLOW_ITERATIONS: u32 = 4;
/// Number of control-flow hash-refinement rounds for [`PcodeBasicBlock`] features.
const DEFAULT_BLOCK_ITERATIONS: u32 = 2;

/// FNV-1a mix of a running hash with one more `u64` word. Deterministic
/// across runs/platforms (unlike `std::hash`), which matters here since
/// hashes are compared, stored, and re-derived independently per function.
fn fnv1a_mix(mut h: u64, word: u64) -> u64 {
    const FNV_PRIME: u64 = 0x100_0000_01b3;
    for byte in word.to_le_bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

fn fnv1a_start() -> u64 {
    0xcbf2_9ce4_8422_2325
}

fn fold_to_u32(h: u64) -> u32 {
    ((h >> 32) as u32) ^ (h as u32)
}

/// Identity key for a [`Varnode`] as a dataflow node -- constants are keyed
/// by nothing (they're never "defined" by an op) and are folded straight
/// into whichever op reads them instead.
fn varnode_key(vn: &Varnode) -> Option<(u64, u64)> {
    if vn.is_constant {
        None
    } else {
        Some((vn.space_id, vn.offset))
    }
}

/// A rough magnitude bucket for a constant, used instead of its exact value
/// so the feature is resilient to build-specific literals (stack offsets,
/// relocated addresses) while still distinguishing "zero/one/small" from
/// "large constant" -- the same intent as Ghidra's `SIG_DONOTUSE_CONST`
/// option, applied unconditionally here rather than as a toggle.
fn const_bucket(val: i64) -> u64 {
    match val {
        0 => 0,
        1 => 1,
        -1 => 2,
        2..=15 => 3,
        -15..=-2 => 4,
        16..=255 => 5,
        -255..=-16 => 6,
        _ => 7,
    }
}

fn local_op_hash(op: &PcodeOp) -> u64 {
    let mut h = fnv1a_start();
    h = fnv1a_mix(h, op.opcode as u64);
    let out_size = op.output.as_ref().map_or(0, |v| v.size as u64);
    h = fnv1a_mix(h, out_size);
    h = fnv1a_mix(h, op.inputs.len() as u64);
    for input in &op.inputs {
        if input.is_constant {
            h = fnv1a_mix(h, 0xC0); // marker: constant operand present
            h = fnv1a_mix(h, const_bucket(input.constant_val));
        } else {
            h = fnv1a_mix(h, u64::from(input.size));
        }
    }
    h
}

fn local_block_hash(block: &PcodeBasicBlock) -> u64 {
    let mut h = fnv1a_start();
    let op_count_bucket = match block.ops.len() {
        0 => 0,
        1..=2 => 1,
        3..=6 => 2,
        7..=15 => 3,
        _ => 4,
    };
    h = fnv1a_mix(h, op_count_bucket);
    h = fnv1a_mix(h, block.successors.len() as u64);
    if let Some(last) = block.ops.last() {
        h = fnv1a_mix(h, last.opcode as u64);
    }
    h
}

/// Global op index: `(block position in `blocks`, op position in `block.ops`)`
/// flattened, so ops can be addressed uniformly regardless of which block
/// they're in.
type OpId = usize;

/// Extract a function's raw feature multiset (unsorted, may contain
/// duplicates -- term frequency is exactly "how many times this hash
/// appears"). This is the data Fission-analog of Ghidra's `VarnodeSignature`
/// + `BlockSignature` collection.
pub fn extract_function_features(pcode: &PcodeFunction) -> Vec<u32> {
    extract_function_features_with_iterations(
        pcode,
        DEFAULT_DATAFLOW_ITERATIONS,
        DEFAULT_BLOCK_ITERATIONS,
    )
}

pub fn extract_function_features_with_iterations(
    pcode: &PcodeFunction,
    dataflow_iterations: u32,
    block_iterations: u32,
) -> Vec<u32> {
    let mut features = Vec::new();

    // ---- flatten ops, index them, and map each defined varnode to its definer ----
    let mut flat_ops: Vec<&PcodeOp> = Vec::new();
    let mut op_block: Vec<usize> = Vec::new();
    for (block_idx, block) in pcode.blocks.iter().enumerate() {
        for op in &block.ops {
            flat_ops.push(op);
            op_block.push(block_idx);
        }
    }
    if flat_ops.is_empty() {
        return features;
    }

    let mut definer: HashMap<(u64, u64), OpId> = HashMap::new();
    for (id, op) in flat_ops.iter().enumerate() {
        if let Some(out) = &op.output
            && let Some(key) = varnode_key(out)
        {
            definer.insert(key, id);
        }
    }

    // Each op's list of "input-defining op ids" (None entries are leaves:
    // external register/memory reads with no definer inside this function).
    let mut input_defs: Vec<Vec<Option<OpId>>> = Vec::with_capacity(flat_ops.len());
    for op in &flat_ops {
        let defs = op
            .inputs
            .iter()
            .filter(|vn| !vn.is_constant)
            .map(|vn| varnode_key(vn).and_then(|k| definer.get(&k).copied()))
            .collect();
        input_defs.push(defs);
    }

    // Preserve the local operation signature as a separate feature radius.
    // The refined fingerprint below is intentionally strict: a changed
    // def-use neighbor can change every hash in its dependency cone. Keeping
    // the local radius lets compiler/optimization variants still match the
    // operation shape without discarding the stronger final context hash.
    let mut cur: Vec<u64> = flat_ops.iter().map(|op| local_op_hash(op)).collect();
    features.extend(cur.iter().map(|hash| fold_to_u32(*hash)));
    for _round in 0..dataflow_iterations {
        let mut next = Vec::with_capacity(cur.len());
        for (id, _op) in flat_ops.iter().enumerate() {
            let mut neighbor_hashes: Vec<u64> = input_defs[id]
                .iter()
                .map(|d| d.map_or(LEAF_SENTINEL, |did| cur[did]))
                .collect();
            neighbor_hashes.sort_unstable(); // order-independent (commutativity-safe)
            let mut h = fnv1a_mix(fnv1a_start(), cur[id]);
            for nh in neighbor_hashes {
                h = fnv1a_mix(h, nh);
            }
            next.push(h);
        }
        cur = next;
    }
    if dataflow_iterations > 0 {
        features.extend(cur.iter().map(|hash| fold_to_u32(*hash)));
    }

    // Keep local CFG shape alongside the final refined control-flow hash for
    // the same multi-radius matching behavior.
    let n_blocks = pcode.blocks.len();
    let mut block_hash: Vec<u64> = pcode.blocks.iter().map(local_block_hash).collect();
    features.extend(block_hash.iter().map(|hash| fold_to_u32(*hash)));
    let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); n_blocks];
    for (idx, block) in pcode.blocks.iter().enumerate() {
        for &succ in &block.successors {
            if let Some(succ_idx) = pcode.blocks.iter().position(|b| b.index == succ) {
                predecessors[succ_idx].push(idx);
            }
        }
    }
    for _round in 0..block_iterations {
        let mut next = Vec::with_capacity(n_blocks);
        for (idx, block) in pcode.blocks.iter().enumerate() {
            let mut neighbor_hashes: Vec<u64> = Vec::new();
            for &succ in &block.successors {
                if let Some(succ_idx) = pcode.blocks.iter().position(|b| b.index == succ) {
                    neighbor_hashes.push(block_hash[succ_idx]);
                }
            }
            for &pred_idx in &predecessors[idx] {
                neighbor_hashes.push(block_hash[pred_idx] ^ 0x5555_5555_5555_5555);
            }
            neighbor_hashes.sort_unstable();
            let mut h = fnv1a_mix(fnv1a_start(), block_hash[idx]);
            for nh in neighbor_hashes {
                h = fnv1a_mix(h, nh);
            }
            next.push(h);
        }
        block_hash = next;
    }
    if block_iterations > 0 {
        features.extend(block_hash.iter().map(|hash| fold_to_u32(*hash)));
    }

    features
}

// ─── Weighted vectors + comparison ────────────────────────────────────────

/// Term-frequency weight curve: Ghidra's `WeightFactory.setLogarithmicTFWeights()`
/// (`tfweight[i] = sqrt(1 + log2(i+1))` where `i = count-1`), reproduced exactly
/// since it's a simple, well-specified, corpus-independent formula.
fn tf_weight(count: u32) -> f64 {
    (1.0 + f64::from(count.max(1)).log2()).sqrt()
}

/// Smoothed inverse-document-frequency weight: `ln(1 + N/df)`. Ghidra ships a
/// pre-trained IDF table from a large reference corpus; there's no equivalent
/// reference corpus here, so this uses the standard smoothed-IDF formula
/// against whatever corpus [`SimilarityCorpus`] has actually indexed --
/// self-consistent, always positive, and decreasing in `df` like Ghidra's
/// table, without claiming to match its exact trained numbers.
fn idf_weight(doc_freq: u32, total_docs: u32) -> f64 {
    (1.0 + f64::from(total_docs.max(1)) / f64::from(doc_freq.max(1))).ln()
}

/// One (hash, weight-coefficient) entry, sorted by `hash` within a
/// [`WeightedVector`] -- mirrors Ghidra's `HashEntry`/`LSHCosineVector`.
#[derive(Debug, Clone, Copy)]
struct WeightedEntry {
    hash: u32,
    coeff: f64,
}

/// A function's feature multiset turned into TF-IDF-weighted coefficients
/// against a specific corpus's document-frequency statistics, ready for
/// [`compare`]. Corpus-relative: the same raw features produce a different
/// `WeightedVector` depending on which corpus weighted them.
#[derive(Debug, Clone)]
pub struct WeightedVector {
    entries: Vec<WeightedEntry>,
    length: f64,
}

impl WeightedVector {
    fn from_raw_features(mut raw: Vec<u32>, idf: &IdfTable) -> Self {
        raw.sort_unstable();
        Self::from_sorted_features(&raw, idf)
    }

    fn from_sorted_features(raw: &[u32], idf: &IdfTable) -> Self {
        let mut entries = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let hash = raw[i];
            let mut tf = 0u32;
            while i < raw.len() && raw[i] == hash {
                tf += 1;
                i += 1;
            }
            let df = idf.doc_freq(hash);
            let coeff = idf_weight(df, idf.total_docs) * tf_weight(tf);
            entries.push(WeightedEntry { hash, coeff });
        }
        let length = entries
            .iter()
            .map(|e| e.coeff * e.coeff)
            .sum::<f64>()
            .sqrt();
        Self { entries, length }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Compare two weighted vectors, returning a similarity score in `[0, 1]`
/// (0 = no shared features, 1 = identical vectors).
///
/// This is a merge-join over both vectors' sorted (hash, coeff) lists: for
/// each hash present in both, the contribution is `min(coeff_a, coeff_b)^2`
/// -- NOT the classic cosine dot product `coeff_a * coeff_b`. This exactly
/// mirrors Ghidra's `LSHCosineVector.compare()`: taking the smaller of the
/// two weights means a feature that's unusually over-represented in only
/// one of the two functions (e.g. from an inlined loop unrolled differently)
/// doesn't inflate the score past what the LESS-confident side supports.
/// The final sum is normalized by both vectors' L2 lengths, same as
/// standard cosine similarity.
pub fn compare(a: &WeightedVector, b: &WeightedVector) -> f64 {
    if a.length == 0.0 || b.length == 0.0 {
        return 0.0;
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut dot = 0.0;
    while i < a.entries.len() && j < b.entries.len() {
        let ea = a.entries[i];
        let eb = b.entries[j];
        match ea.hash.cmp(&eb.hash) {
            std::cmp::Ordering::Equal => {
                let w = ea.coeff.min(eb.coeff);
                dot += w * w;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    dot / (a.length * b.length)
}

// ─── Corpus / query API ────────────────────────────────────────────────────

/// Document-frequency table: for each feature hash, how many indexed
/// functions ("documents") contain it at least once, plus the total
/// document count -- everything [`idf_weight`] needs.
#[derive(Debug, Clone, Default)]
struct IdfTable {
    doc_freq: HashMap<u32, u32>,
    total_docs: u32,
}

impl IdfTable {
    fn doc_freq(&self, hash: u32) -> u32 {
        self.doc_freq.get(&hash).copied().unwrap_or(1)
    }

    fn add_sorted_document(&mut self, raw_features: &[u32]) {
        self.total_docs += 1;
        let mut previous = None;
        for &hash in raw_features {
            if previous == Some(hash) {
                continue;
            }
            *self.doc_freq.entry(hash).or_insert(0) += 1;
            previous = Some(hash);
        }
    }
}

/// A small in-memory corpus of named functions' raw feature multisets, with
/// fuzzy nearest-neighbor query support.
///
/// IDF statistics are corpus-relative and rebuilt from the currently-indexed
/// documents (see [`idf_weight`]'s doc comment) -- adding more functions
/// changes every other function's weighting, so [`Self::query_top_k`]
/// recomputes weighted vectors on demand rather than caching them.
#[derive(Debug, Clone, Default)]
pub struct SimilarityCorpus {
    idf: IdfTable,
    entries: Vec<(String, Vec<u32>)>,
}

impl SimilarityCorpus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add a function's raw feature multiset (from [`extract_function_features`])
    /// to the corpus under `name`.
    pub fn add(&mut self, name: impl Into<String>, mut raw_features: Vec<u32>) {
        if raw_features.is_empty() {
            return;
        }
        raw_features.sort_unstable();
        self.idf.add_sorted_document(&raw_features);
        self.entries.push((name.into(), raw_features));
    }

    /// Rank every OTHER function in the corpus by similarity to `query_name`,
    /// most similar first. Returns an empty vec if `query_name` isn't in the
    /// corpus.
    pub fn most_similar_to(&self, query_name: &str, top_k: usize) -> Vec<(String, f64)> {
        let Some((_, query_raw)) = self.entries.iter().find(|(n, _)| n == query_name) else {
            return Vec::new();
        };
        self.query_top_k_excluding(query_raw, top_k, Some(query_name))
    }

    /// Rank every function in the corpus by similarity to an external query
    /// feature set (e.g. from a function not itself added to this corpus).
    pub fn query_top_k(&self, query_raw: &[u32], top_k: usize) -> Vec<(String, f64)> {
        self.query_top_k_excluding(query_raw, top_k, None)
    }

    fn query_top_k_excluding(
        &self,
        query_raw: &[u32],
        top_k: usize,
        exclude: Option<&str>,
    ) -> Vec<(String, f64)> {
        let query_vec = WeightedVector::from_raw_features(query_raw.to_vec(), &self.idf);
        if query_vec.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(String, f64)> = self
            .entries
            .iter()
            .filter(|(name, _)| exclude != Some(name.as_str()))
            .map(|(name, raw)| {
                let v = WeightedVector::from_sorted_features(raw, &self.idf);
                (name.clone(), compare(&query_vec, &v))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);
        scored
    }
}

/// A stable function record persisted in a cross-binary similarity index.
///
/// Function identity is `(binary_hash, language_id, function_address)`. The
/// path and name are retained as result provenance, but do not participate in
/// identity. Language is necessary for multi-architecture containers such as
/// universal binaries, where one file hash can contain several code images.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityIndexDocument {
    pub binary_hash: String,
    pub binary_path: String,
    pub function_address: u64,
    pub function_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub language_id: String,
    pub features: Vec<u32>,
}

/// Ranked function result with enough provenance to identify the source file
/// and entry point in another binary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilaritySearchHit {
    pub binary_hash: String,
    pub binary_path: String,
    pub function_address: u64,
    pub function_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub language_id: String,
    pub score: f64,
}

/// Versioned, deterministic, offline corpus index for cross-binary queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityIndex {
    format_version: u32,
    documents: Vec<SimilarityIndexDocument>,
}

/// In-memory search view prepared once for one or more queries against an
/// immutable [`SimilarityIndex`]. Feature postings avoid reweighting and
/// rescanning every candidate vector for each query function.
#[derive(Debug)]
pub struct PreparedSimilaritySearch<'a> {
    index: &'a SimilarityIndex,
    idf: IdfTable,
    document_lengths: Vec<f64>,
    postings: HashMap<u32, Vec<SimilarityPosting>>,
}

#[derive(Debug, Clone, Copy)]
struct SimilarityPosting {
    document_index: usize,
    coefficient: f64,
}

impl Default for SimilarityIndex {
    fn default() -> Self {
        Self {
            format_version: SIMILARITY_INDEX_VERSION,
            documents: Vec::new(),
        }
    }
}

impl SimilarityIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_compatible(&self) -> bool {
        self.format_version == SIMILARITY_INDEX_VERSION
    }

    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    pub fn documents(&self) -> &[SimilarityIndexDocument] {
        &self.documents
    }

    /// Canonicalize records loaded from an external file before they enter a
    /// query path. Identity duplicates remain detectable via
    /// [`Self::has_unique_identities`].
    pub fn canonicalize(&mut self) {
        for document in &mut self.documents {
            document.features.sort_unstable();
        }
        self.documents.sort_by(|left, right| {
            left.binary_hash
                .cmp(&right.binary_hash)
                .then_with(|| left.language_id.cmp(&right.language_id))
                .then_with(|| left.function_address.cmp(&right.function_address))
        });
    }

    pub fn has_unique_identities(&self) -> bool {
        let mut identities = HashSet::with_capacity(self.documents.len());
        self.documents.iter().all(|document| {
            identities.insert((
                document.binary_hash.as_str(),
                document.language_id.as_str(),
                document.function_address,
            ))
        })
    }

    /// Insert a function or refresh its provenance/features if the same
    /// binary hash and entry address were already indexed. Returns `true` only
    /// when a new identity was inserted.
    pub fn upsert(&mut self, document: SimilarityIndexDocument) -> bool {
        self.upsert_many([document]).0 > 0
    }

    /// Batch variant used while indexing a whole binary. It avoids sorting or
    /// rescanning the full corpus once per function.
    pub fn upsert_many(
        &mut self,
        documents: impl IntoIterator<Item = SimilarityIndexDocument>,
    ) -> (usize, usize) {
        let mut positions: HashMap<(String, String, u64), usize> = self
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                (
                    (
                        document.binary_hash.clone(),
                        document.language_id.clone(),
                        document.function_address,
                    ),
                    index,
                )
            })
            .collect();
        let original_identities: HashSet<_> = positions.keys().cloned().collect();
        let mut refreshed_identities = HashSet::new();
        let (mut inserted, mut refreshed) = (0, 0);
        for document in documents {
            if document.features.is_empty() {
                continue;
            }
            let key = (
                document.binary_hash.clone(),
                document.language_id.clone(),
                document.function_address,
            );
            if let Some(index) = positions.get(&key).copied() {
                let current = &mut self.documents[index];
                current.binary_path = document.binary_path;
                current.features = document.features;
                let mut names: Vec<_> = current
                    .aliases
                    .iter()
                    .chain(std::iter::once(&current.function_name))
                    .chain(document.aliases.iter())
                    .chain(std::iter::once(&document.function_name))
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .collect();
                names.sort();
                names.dedup();
                if names.is_empty() {
                    current.function_name.clear();
                    current.aliases.clear();
                } else {
                    current.function_name = names.remove(0);
                    current.aliases = names;
                }
                if original_identities.contains(&key) && refreshed_identities.insert(key) {
                    refreshed += 1;
                }
            } else {
                let index = self.documents.len();
                positions.insert(key, index);
                self.documents.push(document);
                inserted += 1;
            }
        }
        self.canonicalize();
        (inserted, refreshed)
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Prepare corpus-relative weights and an inverted feature index for
    /// repeated queries. The prepared view borrows this immutable index and
    /// remains valid until it is dropped.
    pub fn prepare_search(&self) -> PreparedSimilaritySearch<'_> {
        let mut idf = IdfTable::default();
        for document in &self.documents {
            idf.add_sorted_document(&document.features);
        }

        let mut document_lengths = Vec::with_capacity(self.documents.len());
        let mut postings: HashMap<u32, Vec<SimilarityPosting>> = HashMap::new();
        for (document_index, document) in self.documents.iter().enumerate() {
            let weighted = WeightedVector::from_sorted_features(&document.features, &idf);
            document_lengths.push(weighted.length);
            for entry in weighted.entries {
                postings
                    .entry(entry.hash)
                    .or_default()
                    .push(SimilarityPosting {
                        document_index,
                        coefficient: entry.coeff,
                    });
            }
        }

        PreparedSimilaritySearch {
            index: self,
            idf,
            document_lengths,
            postings,
        }
    }

    /// Query the index using an external function's raw feature vector.
    /// `exclude` removes that exact source identity if it is already indexed.
    pub fn query_top_k(
        &self,
        query_features: &[u32],
        exclude: Option<(&str, &str, u64)>,
        top_k: usize,
    ) -> Vec<SimilaritySearchHit> {
        self.prepare_search()
            .query_top_k(query_features, exclude, top_k)
    }
}

impl PreparedSimilaritySearch<'_> {
    /// Rank indexed documents against one query feature set. Only documents
    /// sharing at least one query feature need a full score; zero-score
    /// entries are appended deterministically only when top-k has room.
    pub fn query_top_k(
        &self,
        query_features: &[u32],
        exclude: Option<(&str, &str, u64)>,
        top_k: usize,
    ) -> Vec<SimilaritySearchHit> {
        if top_k == 0 || query_features.is_empty() || self.index.documents.is_empty() {
            return Vec::new();
        }

        let query = WeightedVector::from_raw_features(query_features.to_vec(), &self.idf);
        if query.is_empty() {
            return Vec::new();
        }

        let mut dot_products = HashMap::<usize, f64>::new();
        for query_entry in &query.entries {
            let Some(postings) = self.postings.get(&query_entry.hash) else {
                continue;
            };
            for posting in postings {
                let document = &self.index.documents[posting.document_index];
                if exclude
                    == Some((
                        document.binary_hash.as_str(),
                        document.language_id.as_str(),
                        document.function_address,
                    ))
                {
                    continue;
                }
                let common_weight = query_entry.coeff.min(posting.coefficient);
                *dot_products.entry(posting.document_index).or_default() +=
                    common_weight * common_weight;
            }
        }

        let mut ranked = BinaryHeap::with_capacity(top_k.min(self.index.documents.len()));
        for (document_index, dot_product) in &dot_products {
            let document = &self.index.documents[*document_index];
            let score = if self.document_lengths[*document_index] == 0.0 {
                0.0
            } else {
                dot_product / (query.length * self.document_lengths[*document_index])
            };
            retain_top_k(
                &mut ranked,
                RankedDocument {
                    score,
                    binary_hash: document.binary_hash.clone(),
                    language_id: document.language_id.clone(),
                    function_address: document.function_address,
                    document_index: *document_index,
                },
                top_k,
            );
        }

        if ranked.len() < top_k {
            for (document_index, document) in self.index.documents.iter().enumerate() {
                if dot_products.contains_key(&document_index)
                    || exclude
                        == Some((
                            document.binary_hash.as_str(),
                            document.language_id.as_str(),
                            document.function_address,
                        ))
                {
                    continue;
                }
                retain_top_k(
                    &mut ranked,
                    RankedDocument {
                        score: 0.0,
                        binary_hash: document.binary_hash.clone(),
                        language_id: document.language_id.clone(),
                        function_address: document.function_address,
                        document_index,
                    },
                    top_k,
                );
                if ranked.len() == top_k {
                    break;
                }
            }
        }

        let mut ranked = ranked.into_vec();
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.binary_hash.cmp(&right.binary_hash))
                .then_with(|| left.language_id.cmp(&right.language_id))
                .then_with(|| left.function_address.cmp(&right.function_address))
                .then_with(|| left.document_index.cmp(&right.document_index))
        });
        ranked
            .into_iter()
            .map(|rank| {
                let document = &self.index.documents[rank.document_index];
                SimilaritySearchHit {
                    binary_hash: document.binary_hash.clone(),
                    binary_path: document.binary_path.clone(),
                    function_address: document.function_address,
                    function_name: document.function_name.clone(),
                    aliases: document.aliases.clone(),
                    language_id: document.language_id.clone(),
                    score: rank.score,
                }
            })
            .collect()
    }
}

fn retain_top_k(ranked: &mut BinaryHeap<RankedDocument>, rank: RankedDocument, top_k: usize) {
    if ranked.len() < top_k {
        ranked.push(rank);
    } else if ranked.peek().is_some_and(|worst| rank < *worst) {
        ranked.pop();
        ranked.push(rank);
    }
}

/// Heap ordering keeps the worst currently-selected hit at the root, so the
/// result heap is bounded by `top_k`. The per-query score accumulator is
/// bounded by the documents touched by the query's feature postings.
#[derive(Debug, Clone)]
struct RankedDocument {
    score: f64,
    binary_hash: String,
    language_id: String,
    function_address: u64,
    document_index: usize,
}

impl PartialEq for RankedDocument {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedDocument {}

impl PartialOrd for RankedDocument {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedDocument {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.binary_hash.cmp(&other.binary_hash))
            .then_with(|| self.language_id.cmp(&other.language_id))
            .then_with(|| self.function_address.cmp(&other.function_address))
            .then_with(|| self.document_index.cmp(&other.document_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_pcode::{PcodeBasicBlock, PcodeOpcode};

    fn varnode(space_id: u64, offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn const_vn(val: i64, size: u32) -> Varnode {
        Varnode::constant(val, size)
    }

    fn index_document(
        binary_hash: &str,
        binary_path: &str,
        function_address: u64,
        function_name: &str,
        language_id: &str,
        features: &[u32],
    ) -> SimilarityIndexDocument {
        SimilarityIndexDocument {
            binary_hash: binary_hash.into(),
            binary_path: binary_path.into(),
            function_address,
            function_name: function_name.into(),
            aliases: Vec::new(),
            language_id: language_id.into(),
            features: features.into(),
        }
    }

    fn simple_add_function(reg_offset: u64) -> PcodeFunction {
        // out = reg_offset_var + 5; return out
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1000,
                        output: Some(varnode(1, 0x100, 4)),
                        inputs: vec![varnode(2, reg_offset, 4), const_vn(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x1004,
                        output: None,
                        inputs: vec![varnode(1, 0x100, 4)],
                        asm_mnemonic: None,
                    },
                ],
            }],
        }
    }

    fn different_function() -> PcodeFunction {
        // out = reg - reg2; store out; return
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x2000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x2000,
                        output: Some(varnode(1, 0x200, 8)),
                        inputs: vec![varnode(2, 0x38, 8), varnode(2, 0x40, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x2004,
                        output: None,
                        inputs: vec![varnode(3, 0, 8), const_vn(0x30, 8), varnode(1, 0x200, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x2008,
                        output: None,
                        inputs: vec![],
                        asm_mnemonic: None,
                    },
                ],
            }],
        }
    }

    #[test]
    fn identical_functions_score_near_one() {
        let f1 = simple_add_function(0x38);
        let f2 = simple_add_function(0x38);
        let feat1 = extract_function_features(&f1);
        let feat2 = extract_function_features(&f2);
        let mut corpus = SimilarityCorpus::new();
        corpus.add("f1", feat1);
        corpus.add("f2", feat2);
        let results = corpus.most_similar_to("f1", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "f2");
        assert!(
            results[0].1 > 0.99,
            "expected near-1.0 similarity for identical functions, got {}",
            results[0].1
        );
    }

    #[test]
    fn same_shape_different_register_still_scores_high() {
        // Same operation shape (IntAdd reg+const, then Return), but the
        // input register lives at a different offset -- like the same
        // source function compiled with a different register allocation.
        let f1 = simple_add_function(0x38);
        let f2 = simple_add_function(0x40);
        let feat1 = extract_function_features(&f1);
        let feat2 = extract_function_features(&f2);
        let mut corpus = SimilarityCorpus::new();
        corpus.add("f1", feat1);
        corpus.add("f2", feat2);
        let results = corpus.most_similar_to("f1", 5);
        assert_eq!(results[0].0, "f2");
        assert!(
            results[0].1 > 0.5,
            "expected shape-similarity to survive a register rename, got {}",
            results[0].1
        );
    }

    #[test]
    fn feature_extraction_keeps_local_and_refined_radii() {
        let function = simple_add_function(0x38);
        let local_only = extract_function_features_with_iterations(&function, 0, 0);
        let local_and_refined = extract_function_features_with_iterations(&function, 1, 1);

        assert_eq!(local_only.len(), 3); // two operations and one block
        assert_eq!(local_and_refined.len(), 6);
        assert!(
            local_and_refined.contains(&fold_to_u32(local_op_hash(&function.blocks[0].ops[0])))
        );
        assert!(local_and_refined.contains(&fold_to_u32(local_block_hash(&function.blocks[0]))));
    }

    #[test]
    fn dissimilar_functions_score_lower_than_identical() {
        let f1a = simple_add_function(0x38);
        let f1b = simple_add_function(0x38);
        let f2 = different_function();
        let feat1a = extract_function_features(&f1a);
        let feat1b = extract_function_features(&f1b);
        let feat2 = extract_function_features(&f2);
        let mut corpus = SimilarityCorpus::new();
        corpus.add("f1a", feat1a);
        corpus.add("f1b", feat1b);
        corpus.add("f2", feat2);
        let identical_score = compare(
            &WeightedVector::from_raw_features(
                extract_function_features(&simple_add_function(0x38)),
                &corpus.idf,
            ),
            &WeightedVector::from_raw_features(
                extract_function_features(&simple_add_function(0x38)),
                &corpus.idf,
            ),
        );
        let different_score = compare(
            &WeightedVector::from_raw_features(
                extract_function_features(&simple_add_function(0x38)),
                &corpus.idf,
            ),
            &WeightedVector::from_raw_features(
                extract_function_features(&different_function()),
                &corpus.idf,
            ),
        );
        assert!(
            identical_score > different_score,
            "identical={identical_score} different={different_score}"
        );
    }

    #[test]
    fn empty_function_yields_no_features() {
        let empty = PcodeFunction { blocks: vec![] };
        assert!(extract_function_features(&empty).is_empty());
    }

    #[test]
    fn cross_binary_index_is_versioned_deterministic_and_round_trips() {
        let mut index = SimilarityIndex::new();
        assert!(index.is_compatible());
        assert!(index.upsert(index_document(
            "hash-b",
            "b.elf",
            0x2000,
            "same_name",
            "ARM:LE:32:v8",
            &[3, 1, 2],
        )));
        assert!(index.upsert(index_document(
            "hash-a",
            "a.elf",
            0x1000,
            "same_name",
            "x86:LE:64:default",
            &[1, 2, 3],
        )));

        assert_eq!(index.documents[0].binary_hash, "hash-a");
        assert_eq!(index.documents[0].features, vec![1, 2, 3]);
        assert!(!index.upsert(index_document(
            "hash-a",
            "moved/a.elf",
            0x1000,
            "renamed",
            "x86:LE:64:default",
            &[1, 2, 3],
        )));
        assert_eq!(index.len(), 2);
        assert_eq!(index.documents[0].binary_path, "moved/a.elf");

        let json = serde_json::to_string_pretty(&index).expect("serialize index");
        let decoded: SimilarityIndex = serde_json::from_str(&json).expect("deserialize index");
        assert_eq!(decoded, index);
        assert_eq!(
            serde_json::to_string_pretty(&decoded).expect("serialize again"),
            json,
            "serialization should be stable after canonical upsert ordering"
        );
    }

    #[test]
    fn cross_binary_index_serialization_is_insertion_order_independent() {
        let documents = [
            index_document(
                "hash-b",
                "b.elf",
                0x2000,
                "function_b",
                "AARCH64:LE:64:v8A",
                &[3, 1, 2],
            ),
            index_document(
                "hash-a",
                "a.elf",
                0x1000,
                "function_a",
                "x86:LE:64:default",
                &[6, 4, 5],
            ),
        ];
        let mut forward = SimilarityIndex::new();
        forward.upsert_many(documents.clone());

        let mut reversed = SimilarityIndex::new();
        reversed.upsert_many(documents.into_iter().rev().map(|mut document| {
            document.features.reverse();
            document
        }));

        assert_eq!(
            serde_json::to_vec(&forward).expect("serialize forward index"),
            serde_json::to_vec(&reversed).expect("serialize reversed index")
        );
    }

    #[test]
    fn cross_binary_query_ranks_other_architecture_and_excludes_source_identity() {
        let mut index = SimilarityIndex::new();
        index.upsert(index_document(
            "same-binary",
            "source.elf",
            0x1000,
            "source_function",
            "x86:LE:64:default",
            &[1, 2, 3],
        ));
        index.upsert(index_document(
            "other-binary",
            "arm.elf",
            0x8000,
            "candidate",
            "AARCH64:LE:64:v8A",
            &[1, 2, 3],
        ));
        index.upsert(index_document(
            "unrelated-binary",
            "unrelated.elf",
            0x9000,
            "unrelated",
            "x86:LE:64:default",
            &[90, 91],
        ));

        let prepared = index.prepare_search();
        let exclude = Some(("same-binary", "x86:LE:64:default", 0x1000));
        let results = prepared.query_top_k(&[1, 2, 3], exclude, 1);
        let rebuilt_results = index.query_top_k(&[1, 2, 3], exclude, 1);
        assert_eq!(results, rebuilt_results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].binary_hash, "other-binary");
        assert_eq!(results[0].function_address, 0x8000);
        assert_eq!(results[0].language_id, "AARCH64:LE:64:v8A");
        assert!(results[0].score > 0.99);
        assert!(
            results
                .iter()
                .all(|hit| { hit.binary_hash != "same-binary" || hit.function_address != 0x1000 })
        );
    }

    #[test]
    fn prepared_search_matches_exhaustive_scores_with_duplicates_and_ties() {
        let mut index = SimilarityIndex::new();
        for (hash, address, features) in [
            ("binary-c", 0x3000, vec![1, 2, 2, 5]),
            ("binary-a", 0x1000, vec![1, 1, 2]),
            ("binary-b", 0x2000, vec![2, 1, 1]),
            ("binary-d", 0x4000, vec![1, 9]),
            ("binary-e", 0x5000, vec![88, 89]),
        ] {
            index.upsert(index_document(
                hash,
                "corpus.elf",
                address,
                "candidate",
                "x86:LE:64:default",
                &features,
            ));
        }

        let query_features = [2, 1, 1, 42];
        let excluded = Some(("binary-a", "x86:LE:64:default", 0x1000));
        let prepared = index.prepare_search();
        let actual = prepared.query_top_k(&query_features, excluded, 4);

        let mut idf = IdfTable::default();
        for document in index.documents() {
            idf.add_sorted_document(&document.features);
        }
        let query = WeightedVector::from_raw_features(query_features.to_vec(), &idf);
        let mut expected: Vec<_> = index
            .documents()
            .iter()
            .enumerate()
            .filter(|(_, document)| {
                excluded
                    != Some((
                        document.binary_hash.as_str(),
                        document.language_id.as_str(),
                        document.function_address,
                    ))
            })
            .map(|(document_index, document)| {
                let candidate = WeightedVector::from_sorted_features(&document.features, &idf);
                (
                    compare(&query, &candidate),
                    document.binary_hash.as_str(),
                    document.language_id.as_str(),
                    document.function_address,
                    document_index,
                )
            })
            .collect();
        expected.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.cmp(right.1))
                .then_with(|| left.2.cmp(right.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(&right.4))
        });
        expected.truncate(4);

        assert_eq!(actual.len(), expected.len());
        for (hit, (score, binary_hash, language_id, address, _)) in actual.iter().zip(expected) {
            assert_eq!(hit.binary_hash, binary_hash);
            assert_eq!(hit.language_id, language_id);
            assert_eq!(hit.function_address, address);
            assert_eq!(hit.score, score);
        }
        assert_eq!(actual[0].binary_hash, "binary-b");
        assert_eq!(actual[1].binary_hash, "binary-c");
    }

    #[test]
    fn same_universal_binary_can_index_multiple_architectures_at_one_address() {
        let mut index = SimilarityIndex::new();
        assert!(index.upsert(index_document(
            "fat-binary",
            "universal",
            0x1000,
            "entry_x86",
            "x86:LE:64:default",
            &[1, 2],
        )));
        assert!(index.upsert(index_document(
            "fat-binary",
            "universal",
            0x1000,
            "entry_arm",
            "AARCH64:LE:64:v8A",
            &[1, 2],
        )));
        assert_eq!(index.len(), 2);
        assert!(index.has_unique_identities());
    }

    #[test]
    fn cross_binary_index_rejects_unknown_format_versions() {
        let mut index = SimilarityIndex::new();
        index.format_version += 1;
        assert!(!index.is_compatible());

        index.format_version = 1;
        assert!(
            !index.is_compatible(),
            "v1 hashes use the old feature contract"
        );
    }

    #[test]
    fn duplicate_function_entries_merge_names_without_inflating_update_counts() {
        let mut index = SimilarityIndex::new();
        let (inserted, refreshed) = index.upsert_many([
            index_document(
                "binary",
                "app.elf",
                0x1000,
                "",
                "x86:LE:64:default",
                &[1, 2],
            ),
            index_document(
                "binary",
                "app.elf",
                0x1000,
                "main",
                "x86:LE:64:default",
                &[1, 2],
            ),
            index_document(
                "binary",
                "app.elf",
                0x1000,
                "sub_1000",
                "x86:LE:64:default",
                &[1, 2],
            ),
            index_document(
                "other",
                "other.elf",
                0x2000,
                "helper",
                "AARCH64:LE:64:v8A",
                &[3, 4],
            ),
        ]);
        assert_eq!((inserted, refreshed), (2, 0));
        assert_eq!(index.len(), 2);
        assert_eq!(index.documents[0].function_name, "main");
        assert_eq!(index.documents[0].aliases, vec!["sub_1000"]);

        let (inserted, refreshed) = index.upsert_many([index_document(
            "binary",
            "app.elf",
            0x1000,
            "renamed_main",
            "x86:LE:64:default",
            &[1, 2],
        )]);
        assert_eq!((inserted, refreshed), (0, 1));
        assert_eq!(index.documents[0].function_name, "main");
        assert!(
            index.documents[0]
                .aliases
                .contains(&"renamed_main".to_string())
        );
    }
}
