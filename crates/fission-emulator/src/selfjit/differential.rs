//! Differential testing: capture real, decoded p-code translation blocks
//! from a real corpus binary and replay each one through both JIT
//! backends (`crate::jit::JitCompiler`, the active/production one, and
//! `SelfJitCompiler`, this scaffold), diffing final register state.
//!
//! Every other test in `selfjit::compiler` exercises `SelfJitCompiler`
//! against hand-built synthetic p-code -- necessary for isolating one
//! opcode at a time, but not sufficient on its own: this session's own
//! signed-comparison JIT bug had zero synthetic-test coverage and was
//! only found by testing against a real binary. This module is the
//! reusable tool `selfjit/mod.rs`'s own roadmap calls for (item 4:
//! "differential-test `SelfJitCompiler` against `JitCompiler` on the
//! same real corpus binaries... before ever considering flipping the
//! default") -- built now, alongside `Load`/`Store`, rather than
//! deferred to whenever the opcode list is "done".
//!
//! # Design
//!
//! Cranelift (`JitCompiler`) is treated as the trusted pathfinder: real
//! control flow is data-dependent (a `CBranch`'s target depends on
//! register values this harness doesn't set up a meaningful calling
//! convention for -- and doesn't need to, see below), so rather than
//! trying to make `SelfJitCompiler` drive its own execution across a
//! whole function, each translation block Cranelift actually visits
//! (starting from a chosen entry PC, walking real fallthrough/branch
//! targets as Cranelift's own compiled code reports them) is captured via
//! [`crate::core::Emulator::collect_translation_block`] and replayed
//! *independently* through `SelfJitCompiler` against a second `Emulator`
//! whose state was synced to match Cranelift's *before* that TB ran.
//!
//! Concrete argument values are never set up (no calling-convention
//! injection) -- for a pure A-vs-B differential check this doesn't matter:
//! both backends see whatever garbage is in memory/registers at the
//! chosen entry PC, and the only thing being checked is that both
//! backends compute the *same* thing from it, not that either computes
//! anything semantically meaningful. If a TB uses an opcode
//! `SelfJitCompiler` doesn't support yet, it's skipped (reported, not
//! failed) -- full opcode coverage isn't this milestone's goal.

use crate::core::Emulator;
use crate::jit::backend::TbBackend;
use crate::jit::compiler::JitCompiler;
use crate::os::LinuxEnv;
use crate::pcode::state::MachineState;
use crate::selfjit::compiler::SelfJitCompiler;
use anyhow::{Context, Result};
use fission_pcode::ir::PcodeOpcode;
use std::path::Path;

/// Whether every op in `ops` uses an opcode (and, for `Load`/`Store`, a
/// width) `SelfJitCompiler` currently implements -- see `selfjit/
/// compiler.rs`'s own module doc for the authoritative list; kept in sync
/// by hand since there's no programmatic way to ask `SelfJitCompiler`
/// "would you accept this" without actually trying to compile it (which
/// is exactly what the caller does next for TBs this returns `true` for).
fn selfjit_supports<'a>(ops: impl IntoIterator<Item = &'a fission_pcode::ir::PcodeOp>) -> bool {
    ops.into_iter().all(|op| match op.opcode {
        PcodeOpcode::Copy
        | PcodeOpcode::IntZExt
        | PcodeOpcode::IntSExt
        | PcodeOpcode::IntAdd
        | PcodeOpcode::IntSub
        | PcodeOpcode::IntAnd
        | PcodeOpcode::IntOr
        | PcodeOpcode::IntXor
        | PcodeOpcode::IntMult
        | PcodeOpcode::IntDiv
        | PcodeOpcode::IntSDiv
        | PcodeOpcode::IntRem
        | PcodeOpcode::IntSRem
        | PcodeOpcode::IntLeft
        | PcodeOpcode::IntRight
        | PcodeOpcode::IntSRight
        | PcodeOpcode::Int2Comp
        | PcodeOpcode::IntNegate
        | PcodeOpcode::BoolAnd
        | PcodeOpcode::BoolOr
        | PcodeOpcode::BoolXor
        | PcodeOpcode::BoolNegate
        | PcodeOpcode::IntEqual
        | PcodeOpcode::IntNotEqual
        | PcodeOpcode::IntSLess
        | PcodeOpcode::IntLess
        | PcodeOpcode::IntSLessEqual
        | PcodeOpcode::IntLessEqual
        | PcodeOpcode::Branch
        | PcodeOpcode::CBranch
        | PcodeOpcode::IntCarry
        | PcodeOpcode::IntSCarry
        | PcodeOpcode::IntSBorrow
        | PcodeOpcode::PopCount
        | PcodeOpcode::PtrSub
        | PcodeOpcode::PtrAdd
        | PcodeOpcode::Piece
        | PcodeOpcode::SubPiece
        | PcodeOpcode::LzCount
        | PcodeOpcode::FloatAdd
        | PcodeOpcode::FloatSub
        | PcodeOpcode::FloatMult
        | PcodeOpcode::FloatDiv
        | PcodeOpcode::FloatEqual
        | PcodeOpcode::FloatNotEqual
        | PcodeOpcode::FloatLess
        | PcodeOpcode::FloatLessEqual
        | PcodeOpcode::FloatNeg
        | PcodeOpcode::FloatAbs
        | PcodeOpcode::FloatSqrt
        | PcodeOpcode::FloatNan
        | PcodeOpcode::FloatCeil
        | PcodeOpcode::FloatFloor
        | PcodeOpcode::FloatRound
        | PcodeOpcode::FloatTrunc
        | PcodeOpcode::FloatInt2Float
        | PcodeOpcode::FloatFloat2Float
        | PcodeOpcode::Extract
        | PcodeOpcode::Insert
        | PcodeOpcode::Call
        | PcodeOpcode::CallInd
        | PcodeOpcode::BranchInd
        | PcodeOpcode::Return
        | PcodeOpcode::MultiEqual
        | PcodeOpcode::Indirect
        | PcodeOpcode::SegmentOp => true,
        // Load/Store: implemented, but only the <=8-byte path (see
        // `selfjit/compiler.rs`'s wide-path gap note).
        PcodeOpcode::Load => op.output.as_ref().is_none_or(|o| o.size <= 8),
        PcodeOpcode::Store => op.inputs.len() < 3 || op.inputs[2].size <= 8,
        _ => false,
    })
}

/// Build a fresh `Emulator` with `binary_path` loaded, PC at `entry_pc`.
/// Format-aware (PE or ELF), mirroring `fission-verify::emu_driver::
/// EmulatorHarness::build`'s and `fission-cli`'s `sandbox` setup sequence
/// -- so this harness can target either a self-contained ELF fixture
/// (e.g. `testdata/x64_static_printf_malloc.elf`, `selfjit::compiler`'s
/// own tests already load it) or a real function address from this
/// session's PE corpus (`fission-benchmark/corpus/dev/binaries/c`).
fn build_emulator(binary_path: &Path, entry_pc: u64) -> Result<Emulator> {
    let binary = fission_loader::loader::LoadedBinary::from_file(binary_path)
        .with_context(|| format!("load {}", binary_path.display()))?;
    let mut state = MachineState::new();
    match binary.format.as_str() {
        "PE" => {
            crate::os::windows::loader::load_pe(&mut state, &binary).context("load_pe")?;
        }
        "ELF" | "ELF64" => {
            crate::os::linux::loader::load_elf(&mut state, &binary).context("load_elf")?;
        }
        fmt => anyhow::bail!("unsupported binary format for differential testing: {fmt}"),
    }
    let load_spec = binary.load_spec().context("load spec")?.clone();
    let sleigh =
        fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
            &load_spec,
        )
        .context("sleigh frontend candidates")?
        .into_iter()
        .next()
        .context("at least one sleigh frontend")?;
    let arch = crate::arch::ArchInfo::from_language_id(
        load_spec.pair.language_id.as_str(),
        Some(&binary),
    )
    .context("arch info")?;
    let os: Box<dyn crate::os::OsEnvironment> = match binary.format.as_str() {
        "PE" => Box::new(crate::os::WindowsEnv::new()),
        _ => Box::new(LinuxEnv::new()),
    };
    let mut emu = Emulator::new(state, binary, sleigh, arch, os)?;
    emu.pc = entry_pc;
    Ok(emu)
}

#[derive(Default, Debug)]
pub(crate) struct DifferentialReport {
    pub matched: usize,
    pub skipped_unsupported_opcode: usize,
    pub skipped_compile_error: Vec<String>,
    pub diverged: Vec<String>,
}

impl DifferentialReport {
    fn is_clean(&self) -> bool {
        self.diverged.is_empty()
    }
}

/// Walk up to `max_tbs` translation blocks starting at `entry_pc`,
/// following Cranelift's own real next-PC results (the "trusted
/// pathfinder", see module doc), and for every TB `SelfJitCompiler`
/// claims to support, replay it independently and diff final register
/// state against what Cranelift produced for that same TB.
pub(crate) fn run_differential(binary_path: &Path, entry_pc: u64, max_tbs: usize) -> Result<DifferentialReport> {
    let mut cranelift_emu = build_emulator(binary_path, entry_pc)?;
    let register_space = cranelift_emu.state.register_space();
    let mut report = DifferentialReport::default();
    // Persistent JitCompiler, matching production's Emulator.jit lifetime
    // (one instance reused across TBs) rather than a fresh instance per TB.
    let mut cl_compiler = JitCompiler::new().context("build JitCompiler")?;

    let mut pc = entry_pc;
    for _ in 0..max_tbs {
        cranelift_emu.pc = pc;
        let insns = match cranelift_emu.collect_translation_block() {
            Ok(v) => v,
            Err(_) => break, // e.g. ran off into an unmapped/HLE address -- stop, not an error.
        };
        let ops_flat: Vec<&fission_pcode::ir::PcodeOp> =
            insns.iter().flat_map(|i| i.ops.iter()).collect();

        // Snapshot the exact state Cranelift is about to run this TB
        // from, so SelfJit (if we replay this TB) starts from the
        // identical register/memory state, not whatever state a prior TB
        // in *this* loop already mutated.
        let mut pre_state = cranelift_emu.state.clone();

        let cl_func_ptr = cl_compiler
            .compile_translation_block(&insns, register_space)
            .with_context(|| format!("Cranelift failed to compile TB at 0x{pc:x}"))?;
        let cl_f: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(cl_func_ptr) };
        let next_pc = cl_f(&mut cranelift_emu as *mut _);

        if std::env::var_os("FISSION_DIFF_DEBUG").is_some() {
            let unsupported: Vec<_> = ops_flat
                .iter()
                .filter(|op| !selfjit_supports(std::iter::once(**op)))
                .map(|op| op.opcode)
                .collect();
            if !unsupported.is_empty() {
                eprintln!("  TB@0x{pc:x} skipped -- unsupported opcodes: {unsupported:?}");
            }
        }
        if selfjit_supports(ops_flat.iter().copied()) {
            let pre56 = pre_state.read_space(register_space, 56, 8).unwrap_or_default();
            let pre128 = pre_state.read_space(register_space, 128, 8).unwrap_or_default();
            let mut self_compiler = SelfJitCompiler::new().context("build SelfJitCompiler")?;
            match self_compiler.compile_translation_block(&insns, register_space) {
                Ok(self_func_ptr) => {
                    let mut self_emu = build_emulator(binary_path, pc)?;
                    self_emu.state = pre_state;
                    self_emu.pc = pc;
                    let self_f: extern "C" fn(*mut Emulator) -> u64 =
                        unsafe { std::mem::transmute(self_func_ptr) };
                    let self_next_pc = self_f(&mut self_emu as *mut _);

                    let cl_regs = cranelift_emu
                        .state
                        .read_space(register_space, 0, 512)
                        .unwrap_or_default();
                    let self_regs = self_emu
                        .state
                        .read_space(register_space, 0, 512)
                        .unwrap_or_default();

                    if self_next_pc != next_pc || cl_regs != self_regs {
                        if std::env::var_os("FISSION_DIFF_DEBUG").is_some() {
                            for op in &ops_flat {
                                eprintln!("    op: {:?} out={:?} in={:?}", op.opcode, op.output, op.inputs);
                            }
                            for (i, (a, b)) in cl_regs.iter().zip(self_regs.iter()).enumerate() {
                                if a != b {
                                    eprintln!("    reg byte offset {i}: cranelift=0x{a:02x} selfjit=0x{b:02x}");
                                }
                            }
                            eprintln!("    pre_state offset56={pre56:02x?} offset128={pre128:02x?}");
                            let cl56 = cranelift_emu.state.read_space(register_space, 56, 8).unwrap_or_default();
                            let self56 = self_emu.state.read_space(register_space, 56, 8).unwrap_or_default();
                            eprintln!("    post cl offset56={cl56:02x?} self offset56={self56:02x?}");
                        }
                        report.diverged.push(format!(
                            "TB@0x{pc:x}: next_pc cranelift=0x{next_pc:x} selfjit=0x{self_next_pc:x}, \
                             regs_match={}",
                            cl_regs == self_regs
                        ));
                    } else {
                        report.matched += 1;
                    }
                }
                Err(err) => report
                    .skipped_compile_error
                    .push(format!("TB@0x{pc:x}: {err}")),
            }
        } else {
            report.skipped_unsupported_opcode += 1;
        }

        if next_pc == pc || next_pc >= 0xFFFFFFF0_00000000 {
            break;
        }
        pc = next_pc;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_elf() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_static_printf_malloc.elf")
    }

    /// Real smoke test: walk TBs from this fixture's entry point (its real
    /// CRT startup code, straight-line-heavy and memory-touching) and
    /// confirm `SelfJitCompiler` agrees with the production Cranelift
    /// backend on every TB it claims to support -- zero divergences is the
    /// pass condition; skips (unsupported opcodes elsewhere in the binary)
    /// are expected and reported, not failures.
    ///
    /// Previously capped at 7 TBs to dodge a real divergence at `0x10067e4`
    /// (a plain register-to-register `Copy` where Cranelift's own
    /// `ensure_var!` Variable cache was keyed by `(space, offset)` only --
    /// missing `size` -- so a narrower earlier read's masked, cached value
    /// got wrongly reused by a later wider read to the same offset). Root-
    /// caused via `FISSION_JIT_DUMP_IR` tracing Cranelift's generated IR
    /// SSA value flow directly; fixed by widening the cache key to include
    /// size (`jit/compiler.rs`'s `ensure_var!`). See
    /// `known_issue_cranelift_register_copy_divergence_at_0x10067e4` below
    /// (now a normal passing regression test) and `PROJECT.md` for the full
    /// investigation trail. Cap raised now that the underlying bug is fixed.
    #[test]
    fn selfjit_matches_cranelift_on_real_entry_point_tbs() {
        let path = fixture_elf();
        assert!(path.is_file(), "missing fixture {}", path.display());

        let binary = fission_loader::loader::LoadedBinary::from_file(&path).expect("load binary");
        let entry_pc = binary.inner().entry_point;

        let report = run_differential(&path, entry_pc, 40).expect("run_differential");
        eprintln!(
            "differential: matched={} skipped_unsupported={} skipped_errors={:?} diverged={:?}",
            report.matched, report.skipped_unsupported_opcode, report.skipped_compile_error, report.diverged
        );
        assert!(
            report.is_clean(),
            "SelfJitCompiler diverged from Cranelift on a real binary: {:?}",
            report.diverged
        );
        assert!(
            report.matched > 0,
            "expected at least one TB both backends could run and agree on"
        );
    }

    /// Regression test for the register-copy divergence fixed in
    /// `jit/compiler.rs`'s `ensure_var!` (cache key widened to include
    /// `size`, not just `(space, offset)`). Previously reproduced a real
    /// Cranelift-glue bug at TB `0x10067e4`; now confirms it stays fixed.
    /// Run with `FISSION_DIFF_DEBUG=1` to see the byte-level diff and TB
    /// ops if this ever regresses.
    #[test]
    fn known_issue_cranelift_register_copy_divergence_at_0x10067e4() {
        let path = fixture_elf();
        assert!(path.is_file(), "missing fixture {}", path.display());
        let binary = fission_loader::loader::LoadedBinary::from_file(&path).expect("load binary");
        let entry_pc = binary.inner().entry_point;
        let report = run_differential(&path, entry_pc, 10).expect("run_differential");
        assert!(
            report.is_clean(),
            "regression: register-copy divergence at 0x10067e4 reappeared: {:?}",
            report.diverged
        );
    }

    fn corpus_binary(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fission-benchmark/corpus/dev/binaries/c")
            .join(name)
    }

    /// `checksum` (`control_flow_gcc_O0.exe`, this session's own corpus)
    /// does `*(uchar *)(local_10 + param_1)` inside a loop -- a real
    /// `Load` with a genuinely *computed* address, plus a real back-edge.
    /// Originally found (much earlier in this multi-phase effort) that
    /// every TB here containing a `cmp`/comparison-driven branch was
    /// skipped entirely, `matched == 0` -- not because `Load`/`IntSLess`/
    /// `CBranch` themselves were unsupported, but because x86-64 SLEIGH's
    /// own lowering of `CMP` unconditionally emits `IntCarry`/`IntSCarry`/
    /// `IntSBorrow`/`PopCount` as flag-register side effects alongside the
    /// comparison, even when the actual branch only reads one flag --
    /// exactly the kind of gap this differential harness exists to
    /// surface. Those four (and, later, `Return`, the last opcode this
    /// specific TB walk needed) are all implemented now: every reachable
    /// TB matches cleanly. Run with `FISSION_DIFF_DEBUG=1` to see the
    /// per-TB opcode/byte-level detail if this ever regresses.
    #[test]
    fn selfjit_matches_cranelift_on_real_checksum_loop() {
        let path = corpus_binary("control_flow_gcc_O0.exe");
        if !path.exists() {
            eprintln!("skipping: corpus binary not found at {}", path.display());
            return;
        }
        // `checksum @ 0x1400015b4` per `fission_cli list control_flow_gcc_O0.exe --json`.
        let report = run_differential(&path, 0x1400015b4, 60).expect("run_differential");
        eprintln!(
            "differential (checksum): matched={} skipped_unsupported={} skipped_errors={:?} \
             diverged={:?}",
            report.matched, report.skipped_unsupported_opcode, report.skipped_compile_error, report.diverged
        );
        assert!(
            report.is_clean(),
            "SelfJitCompiler diverged from Cranelift on checksum's real Load/loop: {:?}",
            report.diverged
        );
        assert!(
            report.matched > 0,
            "expected at least one TB both backends could run and agree on"
        );
    }
}
