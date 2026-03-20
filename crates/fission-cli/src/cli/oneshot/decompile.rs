use crate::cli::args::{OneShotArgs, parse_hex_address};
use crate::cli::oneshot::common::{
    EngineMode, apply_profile, fallback_reason_with_kind, init_decompiler, resolve_compiler_id,
    resolve_engine_mode, resolve_profile,
};
use crate::cli::oneshot::disasm::render_function_disassembly_text;
use crate::cli::output::OutputSilencer;
use fission_core::FissionError;
use fission_ffi::DecompilerNative;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_pcode::{NirBuildStats, NirHintStats, PcodeFunction, PcodeOpcode};
use fission_static::analysis::decomp::postprocess::PostProcessor;
use fission_static::analysis::decomp::{
    FactStore, NirEngineMode, NirSurfaceKind, PrepareOptions, PrepareTimings, auto_nir_eligible,
    classify_native_failure_kind, log_type_diag, prepare_native_decompiler_for_binary,
    rescue_nir_output_with_facts, select_nir_output_with_facts, serialize_win_api_signatures_json,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tracing::warn;

#[cfg(feature = "native_decomp")]
use rayon::prelude::*;

mod decompile_exec;
mod decompile_render;
mod decompile_targets;
mod nir_candidates;

pub(super) use decompile_exec::{
    emit_preview_candidate_inventory, emit_preview_candidate_scan_batch, run_decompilation,
};
pub(super) use decompile_targets::select_candidate_functions;
pub(super) use nir_candidates::{
    PreviewCandidateEntry, PreviewCandidateScanSummary, ScopedQuietPanicHook,
    preview_candidate_entry_with_recovery, update_scan_summary,
};
