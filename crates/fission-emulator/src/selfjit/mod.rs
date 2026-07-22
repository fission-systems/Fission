//! Self-implemented JIT compiler -- skeleton, replacing the Cranelift
//! dependency (`crate::jit`) is the end goal, **not done yet**.
//!
//! # Why this exists
//!
//! `crate::jit::JitCompiler` (Cranelift) is the emulator's *only*
//! execution engine -- there is no interpreter fallback (see
//! `crate::jit::callbacks`'s own doc comment: "There is no interpreter
//! fallback path"). Removing the external dependency means replacing it
//! with something that reaches full correctness parity first; this module
//! is the scaffold for that, not a working replacement.
//!
//! # Status
//!
//! [`compiler::SelfJitCompiler`] implements [`crate::jit::TbBackend`] (the
//! same seam `crate::jit::JitCompiler` implements) and genuinely compiles
//! and executes real translation blocks -- see its module doc for the
//! exact covered-opcode list and its own integration test, which builds a
//! real `Emulator`, compiles a hand-built p-code sequence with
//! `SelfJitCompiler` (no Cranelift involved anywhere in the call path),
//! executes the result as real mapped AArch64 machine code, and checks
//! the guest-visible register state came out correct. That test passing
//! is the actual proof-of-concept this skeleton delivers: the pipeline
//! (p-code -> hand-emitted host machine code -> mmap RX -> call -> correct
//! guest state) works end to end without Cranelift, for the ops it covers.
//!
//! It is **not** wired in anywhere as the active backend --
//! `crate::core::Emulator` still only ever constructs a
//! `crate::jit::JitCompiler`. Flipping that default is deliberately left
//! undone until the coverage/correctness gap below is closed; shipping a
//! partial JIT as the *only* engine (remember: no interpreter fallback)
//! would silently break every guest program that touches an unimplemented
//! opcode.
//!
//! # The real gap vs. Cranelift, and the recommended path to closing it
//!
//! `crate::jit::compiler` (Cranelift) covers effectively the full
//! `PcodeOpcode` surface, all host register allocation, and intra-
//! instruction relative-branch loops (the TZCNT bug this session just
//! fixed lives in exactly that machinery). `selfjit::compiler` covers
//! ~25 integer/boolean/comparison opcodes (see that module's own doc for
//! the exact list), no register allocation (every operand round-trips
//! through a host callback call -- correct, but slow), no loops, and one
//! host architecture (AArch64; `emit::x86_64` is an unimplemented stub,
//! see its module doc for why).
//!
//! Closing that gap by writing a second Cranelift (full instruction
//! selection + register allocation + multi-ISA emission) would be a
//! multi-year undertaking on its own. The realistic path -- the one this
//! skeleton is actually scaffolded for -- is **copy-and-patch** style
//! codegen (the technique CPython 3.13's JIT and early LuaJIT/WebKit
//! baseline-JIT tiers used): keep the current call-per-operand approach
//! architecturally (no register allocator needed, since every value's
//! home is always "wherever `jit_read_space`/`jit_write_space` says it
//! is"), but instead of hand-writing each opcode's AArch64 sequence
//! inline in Rust (what `compiler.rs` does today), compile small,
//! reusable machine-code *stencils* per opcode once (e.g. via a build
//! script that runs a tiny C/Rust snippet through the host's own
//! optimizing compiler and extracts the resulting bytes + relocation
//! sites), then stitch stencils together at TB-compile time by `memcpy`
//! + patching immediate operands. This keeps codegen fast (no per-call
//!   instruction-selection logic, just copy+patch) and correctness-
//!   reviewable (each stencil is small, testable in isolation, and can be
//!   diffed against what a real compiler emits) without needing a real
//!   register allocator or an instruction scheduler.
//!
//! Concretely, the remaining work (roughly the recommended order):
//! 1. Add the ~39 missing `PcodeOpcode` variants to `compiler.rs`'s match
//!    (integer arithmetic/shifts/comparisons, zero/sign extension,
//!    `Load`/`Store` -- narrow, ≤8-byte path only -- and now `IntCarry`/
//!    `IntSCarry`/`IntSBorrow`/`PopCount` are covered; the latter four were
//!    re-prioritized *ahead* of this list's original ordering after
//!    `selfjit::differential` (item 4, done early -- see below) confirmed
//!    x86-64 SLEIGH's own lowering of `CMP` unconditionally emits all four
//!    as flag-register side effects alongside *any* comparison, even when
//!    the actual branch only reads one flag -- validated against real
//!    corpus code, not just synthetic unit tests: `checksum`'s real
//!    `Load`-in-a-loop, previously 0 matchable TBs, now replays cleanly).
//!    `Piece`/`SubPiece`/`PtrAdd`/`PtrSub`/`LzCount` are the next tier;
//!    `Float*`/`Call*`/`MultiEqual` are larger, later.
//!    Also close the two documented-but-real correctness gaps in what's
//!    already implemented: results aren't truncated to the varnode's
//!    declared bit width, and shift amounts aren't clamped to that width
//!    before shifting (see `compiler.rs`'s own doc for both), and port
//!    the >8-byte `Load`/`Store` path (needs a stack-slot allocator this
//!    backend doesn't have yet -- see `compiler.rs`'s `Load`/`Store` doc).
//! 2. Support intra-instruction relative BRANCH/CBRANCH (the TZCNT-style
//!    loop construct) -- `compiler.rs` currently refuses to compile any TB
//!    containing one, matching `crate::jit::compiler::remap_relative_
//!    branches`'s own logic once ported over.
//! 3. Implement `emit::x86_64` (this session's own dev machine is Apple
//!    Silicon, so it could not be built *and verified* here -- see that
//!    module's doc for encoding references).
//! 4. **Started** (`selfjit::differential`, `#[cfg(test)]`-only): captures
//!    real, SLEIGH-decoded translation blocks from a real binary (via
//!    `Emulator::collect_translation_block`) and replays each one both
//!    backends can run through `JitCompiler` (trusted pathfinder for real,
//!    data-dependent control flow) and `SelfJitCompiler` independently,
//!    diffing final register state. Not "done": only exercises whatever
//!    opcode subset is currently implemented (which is exactly why item 1
//!    above was refined the way it was), doesn't yet cross-check memory-
//!    space bytes beyond registers, and hasn't been run at real scale
//!    across the corpus. Re-run (and extend) it after every future opcode
//!    addition, not just once at the end, before ever considering flipping
//!    the default.
//!
//!    **A real, precisely-located but unconfirmed lead this harness
//!    already found**: `selfjit::differential::tests::
//!    known_issue_cranelift_register_copy_divergence_at_0x10067e4`
//!    (`#[ignore]`d, not blocking) -- a plain register-to-register `Copy`
//!    in `testdata/x64_static_printf_malloc.elf`'s real CRT startup code
//!    where `SelfJitCompiler`'s result matches the copy's real source
//!    bytes exactly and Cranelift's does not (2 of 8 bytes come out `0`).
//!    Read through `crate::jit::compiler`'s `host_reg_file` fast path,
//!    `dirty`-entry tracking, and `jit_reg_bulk_flush` end-to-end without
//!    finding the mechanism -- everything inspected looks individually
//!    correct in isolation. Flagged here rather than fixed blind; a real
//!    fix needs either reproducing it through production's normal
//!    `run_instruction` orchestration first (this harness calls a compiled
//!    TB's function pointer directly, the same call shape `run_instruction`
//!    itself uses, but hasn't been proven identical in every other respect)
//!    or bisecting Cranelift's register-caching path with the same rigor
//!    this session's signed-comparison JIT fix used.
//! 5. Only then: migrate to copy-and-patch stencils for performance parity,
//!    and retire the `cranelift-*` dependencies from `Cargo.toml`.

pub mod codebuf;
pub mod compiler;
#[cfg(test)]
pub(crate) mod differential;
pub mod emit;

pub use compiler::SelfJitCompiler;
