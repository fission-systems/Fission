# Agent Guidelines for Fission

> LLM-focused working notes for contributors and coding agents.

## Project Summary

Fission is a Rust-first reverse-engineing/decompilation workspace converging on:

- **Ghidra as a lift service** (decode + p-code + CFG skeleton + failure containment)
- **Rust as the decompiler brain** (NIR/HIR normalization, structuring, rendering)
- **CLI and Tauri as product surfaces** over the same analysis core

Core direction is documented in:

- `README.md`
- `docs/architecture/ARCHITECTURE.md`

Treat this repository as one coherent analysis system. Do not solve missing semantics at the UI/surface layer when the right owner is an upstream analysis layer.

---

## Fast Path Rules

1. Find the canonical owner of a fact before editing code. Avoid policy duplication.
2. Keep lift-service logic and Rust decompiler logic separated by responsibility.
3. `fission-cli` and `fission-tauri` are orchestration/product surfaces, not semantic repair layers.
4. Prefer typed contracts over string-level patches whenever possible.
5. For NIR quality changes, wire telemetry (`NirBuildStats`) and automation outputs together.
6. For control-flow work, prioritize algorithmic invariants (dom/postdom/SCC/CFG facts), not binary-specific heuristics.
7. If architecture blocks correctness, redesign the seam at the owning layer instead of adding end-stage hacks.
8. Use current CI/BUILD commands from this file; ignore stale command snippets in older docs.
9. Keep behavior deterministic (ordering, naming, output) when test snapshots/metrics depend on stable output.
10. Large refactors are acceptable when they reduce long-term complexity and tighten ownership.

---

## Task-Start Protocol

For non-trivial changes:

1. Identify user-visible broken behavior or invariant.
2. Choose the owning layer/crate first.
3. Extend existing typed contracts before adding parallel ad-hoc payloads.
4. Push facts upstream to canonical owner; do not reconstruct downstream repeatedly.
5. Add deterministic regression tests at the layer users actually consume.
6. Validate with crate-targeted tests first, then broader suite.

---

## Change Placement Guide

- `ghidra_decompiler/` + `crates/fission-ffi/`
  - lift-service boundary (native extraction, hard-failure isolation)
- `crates/fission-pcode/`
  - p-code model, optimizer, NIR/HIR pipeline, structuring, printer
- `crates/fission-static/`
  - static analysis orchestration, preview routing, session fact handling
- `crates/fission-loader/`
  - binary parsing, symbols/sections/string facts
- `crates/fission-signatures/`
  - signature/FID-related fact source
- `crates/fission-automation/`
  - quality lanes, baseline deltas, decision gates, artifacts
- `crates/fission-cli/`, `crates/fission-tauri/`
  - user-facing orchestration and UX surfaces only

If a fix requires changing ownership between these layers, do it explicitly.

---

## Workspace Layout

Primary workspace members (`Cargo.toml`):

- `fission-automation`
- `fission-core`
- `fission-disasm`
- `fission-loader`
- `fission-pcode`
- `fission-signatures`
- `fission-static`
- `fission-dynamic`
- `fission-analysis`
- `fission-cli`
- `fission-ffi`
- `fission-tauri/src-tauri`

Other key directories:

- `ghidra_decompiler/` (native decompiler/lift side)
- `.github/workflows/` (CI/CD source of truth)
- `scripts/test/` (smoke/comparison/test helpers)

---

## Architecture Stance (Fission-specific)

Use `docs/architecture/ARCHITECTURE.md` as source of truth.

Fission has four layers:

1. Lift Service
2. Canonical IR
3. Structured IR
4. Presentation

Additional system contracts:

- Session Fact Store as conflict-resolved fact source
- Preview-first routing policy with explicit fallback taxonomy

Practical implication:

- If output is ugly but semantically correct, improve in canonical/structured layers first.
- Do not force invalid high-level constructs to avoid `goto` at all costs.

---

## Canonical Contracts

### 1) NIR Build/Quality Telemetry Contract

`NirBuildStats` is the canonical quality telemetry payload for NIR build/structuring signals.

- Add new structuring counters in `crates/fission-pcode/src/nir/types.rs`
- Wire through builder state/snapshots/merging
- Surface in automation (`crates/fission-automation/src/report.rs`) for summary/delta/decision gate coherence

### 2) Preview Routing Contract

Preview-first policy and fallback reasons belong to the static orchestration layer (not UI polish layer).

### 3) Structuring Safety Contract

When uncertain, prefer conservative fallback over incorrect structuring.

- Good fallback > wrong high-level reconstruction

---

## Rewrite Bias

When architecture blocks correctness:

- Rewrite seams/contracts at owning layer.
- Move logic to canonical owner instead of piling adapters downstream.
- Avoid one-off per-binary fixes unless explicitly experimental and isolated.

---

## Rust Typing Rules

- Prefer enums/newtypes/typed structs over loose maps.
- Keep JSON boundary types at I/O edges; keep internal analysis typed.
- Use deterministic collections/order where output stability matters.
- Use `TryFrom`/`From` for boundary conversions.

---

## Build and Run (Current)

From repository root:

```bash
# Build native decompiler
cmake -S ghidra_decompiler -B ghidra_decompiler/build -DCMAKE_BUILD_TYPE=Release
cmake --build ghidra_decompiler/build --config Release

# Build CLI (native integration)
cargo build -p fission-cli --features native_decomp

# Run core tests used in CI fast gate
cargo test -p fission-pcode -p fission-automation -p fission-loader --verbose
```

Automation loop:

```bash
cargo run -p fission-automation -- nir-check --lane nir
```

Useful quality loops:

```bash
cargo run -p fission-automation -- nir-check \
  --lane nir \
  --run-profile fast \
  --focus-top-mismatch 5 \
  --no-build \
  --fission-bin ./target/debug/fission_cli \
  --baseline artifacts/fission-automation/latest/nir/summary.json
```

---

## CI/CD Source of Truth

Use workflow files directly:

- `.github/workflows/ci.yml` (fast gate)
- `.github/workflows/ci-heavy.yml` (heavy validation + automation artifacts)
- `.github/workflows/cd.yml` (release)

Current CI expectations include:

- native decompiler build on Linux/macOS/Windows
- rustfmt + clippy (workspace, tauri excluded in fast gate)
- focused crate test suites
- `fission-automation` `nir-check` lanes in heavy CI

---

## Testing Policy

### Default sequence for NIR/structuring work

1. Targeted tests (`structuring_*`, focused unit tests)
2. `cargo test -p fission-pcode`
3. `cargo check -p fission-pcode`
4. If telemetry/report touched:
   - `cargo test -p fission-automation`
   - `cargo check -p fission-automation`
5. If static wiring touched:
   - `cargo check -p fission-static --features native_decomp`

### Rule

Do not claim completion unless modified crates are tested/typechecked.

---

## NIR Structuring Workflow Guidance

For control-flow changes under `crates/fission-pcode/src/nir/structuring/`:

1. Prefer algorithmic facts first:
   - edge classes
   - dominator/postdominator
   - SCC/irreducible signals
2. Keep reducers conservative under uncertainty.
3. Add both:
   - positive regression (should structure)
   - negative regression (must NOT over-structure)
4. Wire telemetry when behavior class changes.

Reference anchors:

- `structuring/driver.rs`
- `structuring/linear.rs`
- `structuring/loops.rs`
- `structuring/switch.rs`
- `structuring/recovery.rs`
- `structuring/cfg_analysis.rs`

---

## Automation Ownership Rules

`crates/fission-automation` owns run orchestration and quality reporting surfaces.

If you add NIR quality signals:

1. Add fields to `NirBuildStats` (pcode side)
2. Merge/snapshot fields correctly
3. Expose in `build_stats_pairs()`
4. Include in baseline delta and summary markdown
5. Decide whether gate logic should consume the new signal

Do not add divergent metric definitions in multiple places.

---

## AI Agent Anti-Patterns (Do Not Do)

- Fixing semantic gaps only in printer/UI layer
- Adding binary-specific heuristic without invariant-based guard
- Introducing parallel “temporary” telemetry payloads outside `NirBuildStats`
- Modifying CI behavior without reflecting intended validation policy
- Declaring success from a single targeted test while full crate regresses

---

## References

- `README.md`
- `docs/architecture/ARCHITECTURE.md`
- `docs/build/BUILD.md`
- `CONTRIBUTING.md`
- `crates/fission-automation/README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
