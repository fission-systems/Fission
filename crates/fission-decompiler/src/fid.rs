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
use fission_signatures::fidbf::{FidbfDatabase, FidbfMatch, parse_all_fidbf_for_arch};
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
pub fn load_fid_databases(binary: &LoadedBinary) -> Vec<FidbfDatabase> {
    let (databases, _errors) = parse_all_fidbf_for_arch(binary.is_64bit);
    databases
}

/// Per-binary state needed to identify functions -- built once, reused
/// across every function in the binary.
pub struct FidIdentifier<'a> {
    binary: &'a LoadedBinary,
    lifter: RuntimeSleighFrontend,
    register_model: std::sync::Arc<RegisterModel>,
    databases: &'a [FidbfDatabase],
}

impl<'a> FidIdentifier<'a> {
    /// Returns `None` if the binary has no usable load spec, no register
    /// model for its language, or no compiled SLEIGH frontend -- any of
    /// which make FID identification impossible for this binary.
    pub fn new(binary: &'a LoadedBinary, databases: &'a [FidbfDatabase]) -> Option<Self> {
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

    /// Identify the function at `address`, or `None` if it can't be hashed
    /// (too short, an unsupported operand shape -- e.g. SIB addressing --
    /// or decode failure) or hashes but doesn't clear the acceptance
    /// threshold against every loaded database.
    pub fn identify(&self, address: u64) -> Option<FidIdentification> {
        let bytes = self.binary.view_bytes(address, FID_MAX_BYTES)?;
        let memory_context = decode_memory_context_for(self.binary, address, FID_MAX_BYTES);
        let contract = DecodeContract::decomp_function(FID_INSTRUCTION_LIMIT);
        let decoded = self
            .lifter
            .lift_raw_pcode_function_with_context_and_memory_context(
                bytes,
                address,
                contract,
                &memory_context,
                None,
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

        let best: Option<FidbfMatch> = self
            .databases
            .iter()
            .flat_map(|db| db.identify_by_hashes(full_hash, specific_hash))
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
