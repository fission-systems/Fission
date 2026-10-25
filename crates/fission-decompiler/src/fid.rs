//! Function ID (FID) identification: match a function's decoded bytes
//! against Ghidra-shipped `.fidbf` signature databases via
//! `fission_sleigh::runtime::RuntimeSleighFrontend::fid_hashes`.
//!
//! See `PROJECT.md`'s FID entry for the algorithm's provenance and
//! validation history (byte-for-byte matched against real Ghidra 12.0.4 on
//! register/immediate/simple-memory/SIB/RIP-relative operands and both full
//! and specific hashes). Current scope: x86-64 only, not relocation-aware
//! yet (see `fid_hash.rs`'s doc comment for the bounded impact of that
//! gap).

use fission_loader::loader::LoadedBinary;
use fission_pcode::midend::cspec::{RegisterModel, register_model_for_language};
use fission_signatures::fid::{FidDatabase, FidDatabaseSet};
use fission_signatures::fidbf::{FidRelationContext, FidbfMatch};
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;
use fission_static::analysis::{CallGraph, XrefDatabase};

/// Generous upper bound on a single function's instruction count for FID
/// purposes. FID hashing itself further requires the caller to keep the
/// function extent short enough to be meaningful; this is just a decode
/// safety valve, not a tuning knob.
const FID_INSTRUCTION_LIMIT: usize = 4000;

/// Byte window read per function before decoding stops naturally (falls off
/// a `ret`/unconditional jump) or hits [`FID_INSTRUCTION_LIMIT`].
const FID_MAX_BYTES: usize = 1 << 16;

/// A confident FID match for one function (already past
/// `FidbfDatabase::identify_by_hashes`' `FID_ACCEPT_THRESHOLD` scoring gate).
#[derive(Debug, Clone)]
pub struct FidIdentification {
    pub name: String,
    pub library_family: String,
    pub score: f32,
    pub full_hash: u64,
    pub code_unit_count: u16,
    /// Whether the specific hash also matched (the `+10` score bonus).
    pub specific_matched: bool,
}

/// Load every FID database (both packed `.fpk` and `.fidbf`) matching the binary's load spec.
///
/// Discovers databases for the binary's architecture and language, falling back to
/// architecture-wide databases if no language-specific databases match.
/// Call once per binary and reuse across functions -- this is not cheap enough to call per function.
pub fn load_fid_databases(binary: &LoadedBinary) -> Vec<FidDatabase> {
    let compiler_id = binary.get_ghidra_compiler_id();
    let processor = binary.architecture.as_ref().map(|a| a.processor.as_str());
    let language_id = binary.sleigh_language_id();
    let mut db_set = FidDatabaseSet::discover_for_load_spec(
        language_id,
        compiler_id.as_deref(),
        Some(&binary.format),
        binary.is_64bit,
        processor,
    );
    if db_set.databases.is_empty() && language_id.is_some() {
        db_set = FidDatabaseSet::discover_for_load_spec(
            None,
            compiler_id.as_deref(),
            Some(&binary.format),
            binary.is_64bit,
            processor,
        );
    }
    db_set.databases
}

/// Per-binary state needed to identify functions -- built once, reused
/// across every function in the binary.
pub struct FidIdentifier<'a> {
    binary: &'a LoadedBinary,
    lifter: RuntimeSleighFrontend,
    register_model: std::sync::Arc<RegisterModel>,
    databases: &'a [FidDatabase],
    call_graph: Option<&'a CallGraph>,
    lazy_call_graph: std::sync::Mutex<Option<CallGraph>>,
    hash_cache: std::sync::Mutex<std::collections::HashMap<u64, (u16, u64, u64)>>,
}

impl<'a> FidIdentifier<'a> {
    /// Returns `None` if the binary has no usable load spec, no register
    /// model for its language, or no compiled SLEIGH frontend -- any of
    /// which make FID identification impossible for this binary.
    pub fn new(binary: &'a LoadedBinary, databases: &'a [FidDatabase]) -> Option<Self> {
        let load_spec = binary.load_spec()?;
        let register_model = register_model_for_language(load_spec.pair.language_id.as_str())?;
        let lifter = RuntimeSleighFrontend::new_for_load_spec(load_spec).ok()?;
        Some(Self {
            binary,
            lifter,
            register_model,
            databases,
            call_graph: None,
            lazy_call_graph: std::sync::Mutex::new(None),
            hash_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Provide a pre-built call graph for resolving relation context when a candidate
    /// has `force_relation` set.
    pub fn with_call_graph(mut self, call_graph: &'a CallGraph) -> Self {
        self.call_graph = Some(call_graph);
        self
    }

    /// Seed the internal hash cache with known `(code_unit_count, full_hash, specific_hash)` triples.
    pub fn with_known_hashes(
        self,
        hashes: impl IntoIterator<Item = (u64, (u16, u64, u64))>,
    ) -> Self {
        self.hash_cache.lock().unwrap().extend(hashes);
        self
    }

    /// The raw FID hashes for the function at `address`, before any
    /// database lookup: `(code_unit_count, full_hash, specific_hash)`.
    ///
    /// Exposed because "no match" has two very different causes -- the
    /// hash was never computed (decode failure, unsupported operand shape)
    /// versus the hash was computed and no database holds it -- and a
    /// caller that only sees `identify`'s `None` cannot tell them apart.
    pub fn hashes(&self, address: u64) -> Option<(u16, u64, u64)> {
        if let Some(&entry) = self.hash_cache.lock().unwrap().get(&address) {
            return Some(entry);
        }

        // A signature hash is only as good as the decode under it: lifting
        // with `None` hashed an ARM misreading of Thumb bytes, which is why
        // ARM images matched nothing at all.
        let address_state = self.lifter.normalize_low_bit_code_address(address);
        let decode_address = address_state.address;
        let context_override = fission_static::analysis::decode_context_for_address(
            self.binary,
            &self.lifter,
            address_state.context_override,
        );
        let bytes = self.binary.view_bytes(decode_address, FID_MAX_BYTES)?;
        let memory_context = decode_memory_context_for(self.binary, decode_address, FID_MAX_BYTES);
        let contract = DecodeContract::decomp_function(FID_INSTRUCTION_LIMIT);
        let decoded = self
            .lifter
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                decode_address,
                contract,
                &memory_context,
                context_override,
            )
            .ok()?;

        let register_model = &self.register_model;
        let resolve_register_offset = move |name: &str| -> Option<i64> {
            register_model
                .lookup_name(name)
                .map(|(offset, _size)| offset as i64)
        };
        let (code_unit_count, full_hash, _specific_count, specific_hash) = self
            .lifter
            .fid_hashes(&decoded.instructions, &resolve_register_offset)?;
        let result = (code_unit_count, full_hash, specific_hash);
        self.hash_cache.lock().unwrap().insert(address, result);
        Some(result)
    }

    /// Identify the function at `address`, or `None` if it can't be hashed
    /// (too short, an unsupported operand shape -- e.g. SIB addressing --
    /// or decode failure) or hashes but doesn't clear the acceptance
    /// threshold against every loaded database.
    ///
    /// If a candidate requires caller/callee relation context (`has_force_relation_candidate`),
    /// this automatically gathers relation hashes from the call graph (either pre-supplied or
    /// lazily constructed from static xrefs) to disambiguate the match.
    pub fn identify(&self, address: u64) -> Option<FidIdentification> {
        let (code_unit_count, full_hash, specific_hash) = self.hashes(address)?;
        self.identify_hash_with_relation_context(
            address,
            code_unit_count,
            full_hash,
            specific_hash,
            None,
        )
    }

    /// Identify the function at `address` using an explicit [`FidRelationContext`].
    pub fn identify_with_relations(
        &self,
        address: u64,
        context: FidRelationContext<'_>,
    ) -> Option<FidIdentification> {
        let (code_unit_count, full_hash, specific_hash) = self.hashes(address)?;
        self.identify_hash_with_relation_context(
            address,
            code_unit_count,
            full_hash,
            specific_hash,
            Some(context),
        )
    }

    fn identify_hash_with_relation_context(
        &self,
        address: u64,
        code_unit_count: u16,
        full_hash: u64,
        specific_hash: u64,
        explicit_context: Option<FidRelationContext<'_>>,
    ) -> Option<FidIdentification> {
        let needs_relations = self
            .databases
            .iter()
            .any(|db| db.has_force_relation_candidate(full_hash, specific_hash));

        let best: Option<FidbfMatch> = if let Some(context) = explicit_context {
            self.databases
                .iter()
                .flat_map(|db| {
                    db.identify_by_hashes_with_relations(full_hash, specific_hash, context)
                })
                .max_by(|a, b| a.score.total_cmp(&b.score))
        } else if needs_relations {
            let (children, parents) = self.collect_relation_hashes_for(address);
            let context = FidRelationContext {
                children: &children,
                parents: &parents,
            };
            self.databases
                .iter()
                .flat_map(|db| {
                    db.identify_by_hashes_with_relations(full_hash, specific_hash, context)
                })
                .max_by(|a, b| a.score.total_cmp(&b.score))
        } else {
            self.databases
                .iter()
                .flat_map(|db| db.identify_by_hashes(full_hash, specific_hash))
                .max_by(|a, b| a.score.total_cmp(&b.score))
        };

        best.map(|m| FidIdentification {
            name: m.name,
            library_family: m.library_family,
            score: m.score,
            full_hash,
            code_unit_count,
            specific_matched: m.specific_matched,
        })
    }

    fn with_call_graph_ref<R>(&self, f: impl FnOnce(&CallGraph) -> R) -> R {
        if let Some(cg) = self.call_graph {
            return f(cg);
        }
        let mut guard = self.lazy_call_graph.lock().unwrap();
        if guard.is_none() {
            let xrefs = XrefDatabase::build_from_binary(self.binary);
            let cg = CallGraph::build_from_xrefs(&self.binary.functions, &xrefs, 0x40);
            *guard = Some(cg);
        }
        f(guard.as_ref().unwrap())
    }

    fn collect_relation_hashes_for(&self, address: u64) -> (Vec<(u64, u16)>, Vec<(u64, u16)>) {
        let (callees, callers) = self.with_call_graph_ref(|cg| {
            let callees: Vec<u64> = cg.callees_of(address).iter().map(|e| e.addr).collect();
            let callers: Vec<u64> = cg.callers_of(address).iter().map(|e| e.addr).collect();
            (callees, callers)
        });

        let mut child_seen = std::collections::HashSet::new();
        let children: Vec<(u64, u16)> = callees
            .into_iter()
            .filter(|&addr| child_seen.insert(addr))
            .filter_map(|callee_addr| {
                self.hashes(callee_addr)
                    .map(|(units, full, _)| (full, units))
            })
            .collect();

        let mut parent_seen = std::collections::HashSet::new();
        let parents: Vec<(u64, u16)> = callers
            .into_iter()
            .filter(|&addr| parent_seen.insert(addr))
            .filter_map(|caller_addr| {
                self.hashes(caller_addr)
                    .map(|(units, full, _)| (full, units))
            })
            .collect();

        (children, parents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_signatures::fid::FidDatabase;
    use fission_signatures::fidbf::{
        FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfRelation, FidbfRelationType,
        relation_smash,
    };
    use std::path::PathBuf;

    fn fixture_binary() -> LoadedBinary {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fission-loader/testdata/x64_dyn_puts.elf");
        LoadedBinary::from_file(&path).expect("load x64_dyn_puts.elf fixture")
    }

    #[test]
    fn test_load_fid_databases_discovers_databases() {
        let binary = fixture_binary();
        let databases = load_fid_databases(&binary);
        for db in &databases {
            assert!(!db.source_path().is_empty());
        }
    }

    #[test]
    fn test_fid_identifier_forced_relation_disambiguation() {
        let binary = fixture_binary();
        let target_func = binary
            .functions
            .iter()
            .find(|f| !f.is_import)
            .expect("must have a non-import function");
        let address = target_func.address;

        let empty_dbs: Vec<FidDatabase> = Vec::new();
        let identifier = FidIdentifier::new(&binary, &empty_dbs).expect("identifier construction");
        let (_units, full_hash, specific_hash) =
            identifier.hashes(address).expect("hashes calculated");

        let child_hash = 0x5555_6666_7777_8888_u64;
        let candidate = FidbfFunction {
            key: 42,
            library_id: 1,
            name: "target_identified_by_relation".to_string(),
            full_hash,
            specific_hash,
            code_unit_size: 20,
            entry_point: 0,
            has_terminator: true,
            specific_hash_additional_size: 0,
            domain_path: String::new(),
            flags: 0,
            auto_pass: false,
            auto_fail: false,
            force_specific: false,
            force_relation: true,
        };

        let relation = FidbfRelation {
            key: relation_smash(candidate.key, child_hash),
            relation_type: FidbfRelationType::Superior,
        };

        let db = FidbfDatabase::new(
            "synthetic_test.fidbf".to_string(),
            vec![FidbfLibrary {
                key: 1,
                family_name: "test_lib".to_string(),
                version: "1.0".to_string(),
                variant: String::new(),
                ghidra_version: "12.0.4".to_string(),
                language_id: "x86:LE:64:default".to_string(),
                language_version: 1,
                language_minor_version: 0,
                compiler_spec_id: "gcc".to_string(),
            }],
            vec![candidate],
            vec![relation],
        );

        let databases = vec![FidDatabase::Eager(Box::new(db))];
        let identifier = FidIdentifier::new(&binary, &databases).expect("identifier");

        // Without relation context matching the child relation, candidate must be rejected.
        let unaugmented_id = identifier.identify(address);
        assert!(unaugmented_id.is_none());

        // With explicit relation context containing the superior relation's child hash, candidate matches.
        let context = FidRelationContext {
            children: &[(child_hash, 10)],
            parents: &[],
        };
        let matched = identifier
            .identify_with_relations(address, context)
            .expect("should match with child relation");
        assert_eq!(matched.name, "target_identified_by_relation");
        assert_eq!(matched.library_family, "test_lib");
        assert!(matched.score > 20.0);
    }
}
