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
use fission_signatures::fidbf::{FidRelationContext, FidbfMatch, parse_all_fidbf_for_arch};
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;

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

/// Load every `.fidbf` database matching the binary's pointer width.
///
/// Deliberately doesn't pre-filter by processor family (`parse_all_fidbf_for_arch`
/// only distinguishes 32- vs 64-bit) -- a handful of bundled databases parse
/// fast enough that pre-filtering isn't worth the complexity yet, and a
/// hash collision against an unrelated architecture's database is filtered
/// out by `identify_by_hashes`' scoring gate the same way any other
/// coincidental collision would be. Call once per binary and reuse across
/// functions -- this is not cheap enough to call per function.
pub fn load_fid_databases(binary: &LoadedBinary) -> Vec<FidDatabase> {
    let compiler_id = binary.get_ghidra_compiler_id();
    let processor = binary.architecture.as_ref().map(|a| a.processor.as_str());
    let db_set = FidDatabaseSet::discover_for_load_spec(
        binary.sleigh_language_id(),
        compiler_id.as_deref(),
        Some(&binary.format),
        binary.is_64bit,
        processor,
    );
    let mut databases = db_set.databases;
    if databases.is_empty() {
        let (fallback, _errors) = parse_all_fidbf_for_arch(binary.is_64bit);
        databases.extend(
            fallback
                .into_iter()
                .map(|d| FidDatabase::Eager(Box::new(d))),
        );
    }
    databases
}

/// Per-binary state needed to identify functions -- built once, reused
/// across every function in the binary.
pub struct FidIdentifier<'a> {
    binary: &'a LoadedBinary,
    lifter: RuntimeSleighFrontend,
    register_model: std::sync::Arc<RegisterModel>,
    databases: &'a [FidDatabase],
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
        })
    }

    /// The raw FID hashes for the function at `address`, before any
    /// database lookup: `(code_unit_count, full_hash, specific_hash)`.
    ///
    /// Exposed because "no match" has two very different causes -- the
    /// hash was never computed (decode failure, unsupported operand shape)
    /// versus the hash was computed and no database holds it -- and a
    /// caller that only sees `identify`'s `None` cannot tell them apart.
    pub fn hashes(&self, address: u64) -> Option<(u16, u64, u64)> {
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
        Some((code_unit_count, full_hash, specific_hash))
    }

    /// Identify the function at `address`, or `None` if it can't be hashed
    /// (too short, an unsupported operand shape -- e.g. SIB addressing --
    /// or decode failure) or hashes but doesn't clear the acceptance
    /// threshold against every loaded database.
    pub fn identify(&self, address: u64) -> Option<FidIdentification> {
        self.identify_with_relations(address, None)
    }

    /// Identify the function at `address` taking into account caller/callee relation context.
    pub fn identify_with_relations(
        &self,
        address: u64,
        relation_context: Option<FidRelationContext<'_>>,
    ) -> Option<FidIdentification> {
        let (code_unit_count, full_hash, specific_hash) = self.hashes(address)?;

        let empty = FidRelationContext {
            children: &[],
            parents: &[],
        };
        let context = relation_context.unwrap_or(empty);

        let best: Option<FidbfMatch> = self
            .databases
            .iter()
            .flat_map(|db| db.identify_by_hashes_with_relations(full_hash, specific_hash, context))
            .max_by(|a, b| a.score.total_cmp(&b.score));

        best.map(|m| FidIdentification {
            name: m.name,
            library_family: m.library_family,
            score: m.score,
            full_hash,
            code_unit_count,
            specific_matched: m.specific_matched,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_signatures::fidbf::{FidbfDatabase, FidbfFunction};

    #[test]
    fn load_fid_databases_loads_without_panic() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let databases = load_fid_databases(&binary);
        let _ = databases.len();
    }

    #[test]
    fn fid_database_eager_metadata_and_scoring() {
        let dummy_db = FidbfDatabase::new(
            "test.fidbf".to_string(),
            vec![],
            vec![FidbfFunction {
                key: 1,
                library_id: 0,
                name: "test_fn".to_string(),
                full_hash: 0x1234,
                specific_hash: 0x5678,
                code_unit_size: 50,
                entry_point: 0x1000,
                has_terminator: true,
                specific_hash_additional_size: 0,
                domain_path: String::new(),
                flags: 0,
                auto_pass: true,
                auto_fail: false,
                force_specific: false,
                force_relation: false,
            }],
            vec![],
        );
        let fid_dbs = vec![FidDatabase::Eager(Box::new(dummy_db))];
        assert_eq!(fid_dbs.len(), 1);
        assert_eq!(fid_dbs[0].len(), 1);
        assert_eq!(fid_dbs[0].source_path(), "test.fidbf");

        let matches = fid_dbs[0].identify_by_hashes_with_relations(
            0x1234,
            0x5678,
            FidRelationContext {
                children: &[],
                parents: &[],
            },
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "test_fn");
    }
}
