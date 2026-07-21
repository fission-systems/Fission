//! Real, compiled-binary end-to-end run of DIR/HIR differential
//! verification -- everything in `src/diff.rs`'s own unit tests uses
//! hand-built `HirStmt` fixtures; this is the first time the pipeline runs
//! against a snapshot the *real* production pipeline (builder -> normalize
//! -> structuring) produced, through the same public
//! `decompile_with_rust_sleigh` entrypoint `fission_cli` uses.
//!
//! Fixture: `testdata/max2.elf` (`testdata/src_max2.c`), a tiny statically
//! linked x86-64 function --
//! `int max2(int a, int b) { if (a > b) return a; return b; }` -- built
//! at `-O0` (real stack-spilled args/locals, not everything folded to
//! constants) specifically so this test exercises Fission's stack-local
//! promotion (turning `[rbp-8]`/`[rbp-0xc]`/`[rbp-4]` spills back into
//! plain HIR variables) rather than asserting behavior on synthetic p-code.

use fission_decompiler::pipeline::rust_sleigh::{
    RustSleighDecompileConfig, decompile_with_rust_sleigh,
};
use fission_decompiler::{take_last_dir_snapshot, take_last_hir_function_snapshot};
use fission_loader::loader::LoadedBinary;
use fission_midend_core::ir::Hir;

use fission_dir::diff::{VerifyOutcome, default_samples, diff_dir_hir};

const MAX2_ADDRESS: u64 = 0x10093d0;

#[test]
fn max2_dir_and_hir_are_equivalent_on_a_real_compiled_function() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/max2.elf");
    let binary = LoadedBinary::from_file(&path).expect("load real test ELF");

    let config = RustSleighDecompileConfig::default();
    let result = decompile_with_rust_sleigh(&binary, MAX2_ADDRESS, "max2", &config, None, None)
        .expect("decompile_with_rust_sleigh");

    let dir = take_last_dir_snapshot().expect("DIR snapshot captured during decompile");
    let hir_func =
        take_last_hir_function_snapshot().expect("HIR function snapshot captured during decompile");
    let hir = Hir(hir_func.body.clone());

    let samples = default_samples(hir_func.params.len());
    let outcome = diff_dir_hir(&dir, &hir, &hir_func.params, &hir_func.locals, &samples);

    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(checked > 0, "expected at least one comparable sample");
        }
        VerifyOutcome::Diverged(divs) => {
            panic!(
                "real structuring bug found on a real compiled function! \
                 rendered HIR:\n{}\ndivergences: {divs:?}",
                result.code
            );
        }
        VerifyOutcome::Unsupported { reason } => {
            panic!(
                "max2 hit an unsupported construct -- expected pure interp \
                 coverage for this fixture. rendered HIR:\n{}\nreason: {reason}",
                result.code
            );
        }
    }
}
