# Issue #44: Address-only stack buffer proposal

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_030.elf`
- SHA-256: `d92d12e23c4411279c98c9e95c5454f5e948f27fa14a60d27e16942f5a1c39e8`
- Function: `sub_90ae`
- Address: `0x90ae`
- Corpus row or benchmark command:
  `target/release/fission_cli decomp <binary> --addr 0x90ae --layer hir --no-header --no-warnings --no-db`
- Current `main` output summary before the change: 220 lines / 5,966 bytes. It contains two `48 + rsp` expressions, and passes `readfds` to `select` even though the function output does not declare or define `rsp`.
- Semantic cases passed / total: N/A; this is an issue-level CLI reproduction, not a source-semantic benchmark row.
- Failure category: invalid standalone HIR expression and missing stack-object alias identity.
- Relevant observations: p-code proves a fixed `0x368`-byte frame. The escaped pointer starts at steady-state `rsp + 0x30`; repeated accesses include dynamic indexing. The independent Glaurung output models the remainder as an 824-byte local array. Direct frame accesses and the stack canary also lie in that range, so a second overlapping local would not preserve aliasing. Raw p-code also adjusts `rsp` in multiple later blocks; the current per-site table covers only the entry block. A frame backing must therefore require proven per-site stack coordinates across the CFG, including agreeing offsets at joins.
- Historical iteration note: the actual CLI output remained byte-for-byte identical to its then-current baseline (222 lines / 5,978 bytes; `48 + rsp` at lines 120 and 128). The candidate frame map was declined. The p-code explains why: x64 calls contain a same-address `IntSub rsp, 8`, a store of the return address through `rsp`, and a `Call`. The initial CFG transfer carried that transient call push into the caller's continuation; the first join then saw `-8`, `-16`, and `-24` byte coordinates. For a known caller-clean ABI, the caller resumes at its pre-call stack coordinate. Model that exact p-code sequence as a returning call effect; reject stack-changing calls whose cleanup ABI is not known.
- Rechecked after modeling the returning call effect: `rsp`/`rbp` arithmetic disappeared and the frame became one 872-byte `stack_frame`, but the `select` argument was `stack_frame + 40` while the same buffer's indexed accesses used `stack_frame + 48`. The CALL op's lowering site still inherited the transient `-8` return-address push. The push must remain visible to its p-code Store, while the CALL site and caller argument expressions must use the instruction-entry coordinate.
- Rechecked after separating the CALL site coordinate: the `select` fd-set argument now matches its indexed accesses at `stack_frame + 48`, but its timeout argument also resolves to `+48` even though the machine code builds the timeout at `rsp + 0x20` and the fd-set at `rsp + 0x30`. The raw p-code reuses one unique-space temporary for both LEAs. Resolving the first pointer through R8 -> RDX -> that temporary at the later CALL site selects the later LEA. Stack-address provenance must carry each use/definition site while walking reused unique varnodes.
- Candidate after the escape-sink refinement: 233 lines / 7,886 bytes; no `rsp` or `rbp` tokens. Both the indexed fd-set aliases and the `select` argument use the same `stack_frame + 48` base. The frame object is intentionally opaque, so other unknown bytes still print as `undefined` accesses.
- Regression probe after the escape-sink refinement: `crypto_gcc_O0.exe` at `0x1400017ca` still passes the buffer at `stack_frame + 48` to `rc4_init` and `rc4_crypt` (19 lines / 675 bytes). `data_structures_gcc_O0.exe` at `0x140001756` keeps `Pair pairs[3]` and `int values[6]` (52 lines / 1,140 bytes) and passes those named arrays to calls; it no longer promotes the whole frame to `stack_frame`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

`PreviewBuilder::stack_local_address_expr` in `crates/fission-pcode/src/midend/builder/memory/stack_slots.rs` returns no expression when the resolved offset is not covered by a known stack slot. That lets address arithmetic fall through to ordinary expression lowering, which emits `rsp` arithmetic. `run_incremental_heritage` in `crates/fission-pcode/src/midend/builder/materialize/incremental.rs` only creates slots from statically resolved Load/Store addresses; it does not create a shared object for the dynamic indexed pointer. The current Load/Store paths then turn each resolved stack slot into a scalar `Var`, so recovering only the escaped pointer would leave overlapping accesses as separate objects.

## 3. Generality / Invariant Proof

Generalized rule:

When entry analysis proves one finite fixed stack frame, a CFG dataflow analysis proves each relevant stack-base coordinate (all joins must agree after accounting for proven call-return effects), and an escaped frame-derived address is not already covered by fixed memory accesses, represent the proven frame bytes with one opaque backing object. Identify escapes at ABI integer argument registers, stored pointer values, returns, and explicit call operands; do not treat a frame-base setup or a callee-saved register spill as an escaped local pointer. Route every in-frame stack read and write through the same object using byte offsets, and express escaped pointers as offsets into it. Normalize a same-instruction return-address push only when the p-code has the matching stack store and the ABI is known caller-clean; if a stack adjustment, call cleanup, or join makes a relevant coordinate unknown, do not create the backing object. Keep incoming arguments, caller home slots and addresses outside the proven frame under their existing owners; outgoing argument slots inside the frame share the backing object. This is a conservative alias-preserving representation; it does not claim to recover the source-level buffer's exact C extent.

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] The rule is about a proven stack region and alias identity, not a function, address, or mnemonic.
- [x] ISA-specific stack-base and frame-layout facts remain in the existing ABI/register-namer and entry-layout model.
- [x] Synthetic invariant tests cover a fixed frame, an escaped pointer, and an overlapping direct access without a compiler-specific shape.

Comparable coverage:

- Similar shape 1: `fission-benchmark/corpus/dev/binaries/c/crypto_gcc_O0.exe`, function at `0x1400017ca`; frame-derived address escapes to `rc4_init` and `rc4_crypt`. The current candidate routes both call arguments through a single `stack_frame` object.
- Similar shape 2: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/data_structures_gcc_O0.exe`, `main` at `0x140001756`; prologue RBP save and two address-taken arrays passed to calls. After the change, the arrays remain named and the RBP spill does not trigger whole-frame backing.
- Synthetic invariant test: fixed-frame backing preserves reads/writes through a pointer and through a direct frame-relative access.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: builder stack-memory resolution and `run_incremental_heritage`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [x] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: entry analysis already exposes a finite frame size, and the builder already resolves stack-relative memory. Its entry-only RSP table is insufficient for this real function's later pushes/pops, so extend the existing stack-layout owner with CFG-propagated per-site coordinates and use those proven coordinates to select one shared local identity for all accesses into the frame.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: do not add a pass unless implementation proves the existing stack-memory owner cannot hold the backing-region fact.
- Possible interaction with existing normalize/structuring/materialize passes: stack loads currently become scalar local variables and stores become variable assignments. Backing-region accesses must instead remain typed dereferences with byte-offset pointers so aliases survive materialization and normalization. Aggregate-store recovery must not reinterpret the backing object as an independent local. CFG coordinate propagation must reject conflicting stack depths at joins and unknown/dynamic stack adjustments rather than guessing.
- New or changed owner-to-owner dependency:
  - [x] None intended; keep the backing-region identity inside builder stack-memory/materialization ownership.
- Telemetry impact, if any: none expected.
- Known cases that must not change: caller-owned parameters, Windows home slots and outgoing arguments; inlined dynamic stack-probe loops; frame-pointer definitions that are not proven entry-frame bases; any frame address outside the established local frame.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command/result: `cargo nextest run -p fission-pcode frame --no-fail-fast`; 26 passed. This includes fixed-frame aliasing, an ABI register argument, an RBP save spill, and call-only pointer cases.
  - Signal: the frame-base setup and callee-saved spill do not trigger backing; an uncovered address copied into an ABI argument register does.
- [x] Crate-level gate:
  - Command/result: `cargo nextest run -p fission-pcode --no-fail-fast`; 1,153 passed, 1 skipped, 0 failed.
  - Stack-slot/materialization regressions: none observed among the crate tests.
- [x] Focused binary reproduction:
  - Command/result: release `fission_cli decomp ... --addr 0x90ae --layer hir --no-header --no-warnings --no-db`; 233 lines / 7,886 bytes. Output has no `rsp`/`rbp`; the dynamic fd-set accesses and `select` argument share `stack_frame + 48`.
  - This is a correctness/alias-representation result, not a measured readability gain. The opaque frame still contains `undefined` accesses for bytes whose types are unknown.
- [x] Smoke samples:
  - `crypto_gcc_O0.exe` at `0x1400017ca`: 19 lines / 675 bytes; `rc4_init` and `rc4_crypt` both receive the same `stack_frame + 48` buffer.
  - `data_structures_gcc_O0.exe` at `0x140001756`: 52 lines / 1,150 bytes; `Pair pairs[3]` and `int values[6]` remain named and are passed to their calls. The saved RBP spill does not create opaque frame backing.
- [x] Related checks:
  - `CARGO_BUILD_JOBS=1 cargo build -p fission-cli --release --bin fission_cli` passed.
  - `CARGO_BUILD_JOBS=1 cargo check -p fission-pcode -p fission-decompiler` passed.
- [x] Boundary audit:
  - `python3 scripts/audit/nir_boundary_scan.py --root .` passed with zero findings, violations, or migration debt.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt:
  - [ ] Structural failure pattern only
  - [ ] Owner evidence only
  - [ ] Invariant candidates only
  - [ ] Validation matrix only
- Redaction confirmed:
  - [ ] Function names removed
  - [ ] Addresses removed
  - [ ] Binary paths removed
  - [ ] Corpus row ids removed
  - [ ] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [ ] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run.
  - Synthetic invariant test command/result: `cargo nextest run -p fission-pcode frame --no-fail-fast`; 26 passed.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Proposal invariant is identity-independent.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] The acceptance claim is correctness and alias representation, not quality score or readability.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Extend the existing stack-memory owner unless implementation evidence disproves that boundary.
