# Full Rust Decompiler Roadmap

## Vision

Build a production-grade decompiler pipeline where semantic lifting, IR normalization, structuring, and C rendering are Rust-native, with C++ reduced to (and eventually removed from) critical runtime paths.

## Target Pipeline

1. Assembly / bytes input
2. Sleigh decode + semantics
3. Raw p-code
4. P-code canonicalization / cleanup
5. NIR build
6. NIR normalize + structuring
7. C printer

This architecture is correct for long-term Rust ownership, as long as unsupported semantics are explicit and measurable.

## Guiding Principles

- Fix behavior at canonical semantic layers, not UI or printer-only layers.
- Prefer deterministic CFG/dom/postdom/SCC-based transforms over binary-specific heuristics.
- Keep metrics and telemetry contracts centralized (single source of truth).
- Roll out with shadow mode and parity gates before switching defaults.
- Preserve a safe fallback path until parity and reliability targets are met.

## Phased Plan

### Phase 0: Contracts and Baselines

**Goal:** Freeze interfaces and establish objective parity measurement.

**Work:**
- Define stable boundaries between:
  - native lift input
  - p-code payload
  - NIR build input
  - structuring output
  - C printer output
- Snapshot current outputs for representative corpora.
- Add automated diff tooling for:
  - p-code shape parity
  - NIR shape parity
  - C output stability

**Exit Criteria:**
- Reproducible baseline artifacts for each corpus.
- CI job that reports drift (not just pass/fail).

---

### Phase 1: Semantic Coverage in Rust (Sleigh -> p-code)

**Goal:** Close high-impact semantic gaps in Rust conversion.

**Work:**
- Prioritize unsupported statements/expressions by frequency and blast radius.
- Complete dynamic attach resolution paths (token/context-driven).
- Add explicit diagnostics for unsupported operations with stable error taxonomy.
- Enforce deterministic op ordering and temp allocation behavior.

**Exit Criteria:**
- Unsupported ratio reduced below agreed threshold on target corpora.
- No unknown-failure class in top-N frequent instructions.
- Determinism checks pass across repeated runs.

---

### Phase 2: NIR Reliability and Structuring Quality

**Goal:** Ensure Rust-native NIR path is robust enough for default usage.

**Work:**
- Strengthen canonicalization and normalization passes.
- Improve structuring pass quality for difficult CFGs (irreducible/guarded tails/etc.).
- Expand validation lanes for:
  - structured ratio
  - regression deltas
  - printer-friendly shape invariants

**Exit Criteria:**
- NIR quality metrics meet or exceed current production baseline.
- No critical regressions in designated benchmark suites.

---

### Phase 3: Operational Migration (Shadow -> Partial Default)

**Goal:** Make Rust path production-visible with controlled risk.

**Work:**
- Run Rust path in shadow mode for all supported workloads.
- Introduce feature-flag rollout by architecture / binary family.
- Keep C++ as fallback only, with telemetry on fallback cause.

**Exit Criteria:**
- Shadow-mode parity within tolerance over sustained period.
- Fallback rate decreases release-over-release.
- No P0 incidents attributable to Rust default cohorts.

---

### Phase 4: Rust-First by Default

**Goal:** Flip default execution path to Rust.

**Work:**
- Enable Rust path by default for stable cohorts.
- Keep guarded emergency fallback for unstable segments.
- Tighten regression gates to prevent semantic backsliding.

**Exit Criteria:**
- Rust default handles majority of production workload.
- Fallback retained only for explicitly tracked edge cases.

---

### Phase 5: C++ Dependency Elimination

**Goal:** Remove critical C++ dependency from runtime decompilation path.

**Work:**
- Eliminate remaining fallback calls.
- Remove obsolete bridge code and build targets.
- Keep optional compatibility mode only if product policy requires it.

**Exit Criteria:**
- Production decompilation path is Rust-only.
- CI, release, and quality lanes pass without C++ runtime dependency.

## Metrics to Track Continuously

- Semantic unsupported ratio
- Fallback invocation rate and reason distribution
- Structured function ratio
- Output determinism rate
- Regression delta count per release
- End-to-end throughput and latency

## Risk Register (Top Items)

- **Semantic parity drift**
  - Mitigation: mandatory parity reports per PR on hot paths.
- **Performance regressions**
  - Mitigation: benchmark gates for representative binary sets.
- **Heuristic overfitting to specific binaries**
  - Mitigation: require invariant-based justifications for new logic.
- **Printer masking semantic defects**
  - Mitigation: fail early in semantic/NIR validation layers.

## Practical Milestones

1. Week 0-2: Contracts + baseline CI artifacts
2. Week 3-6: Top semantic gap closures
3. Week 7-10: NIR/structuring reliability hardening
4. Week 11-14: Shadow mode at scale
5. Week 15+: Progressive default flip and fallback retirement

(Adjust schedule based on corpus complexity and team capacity.)

## Definition of Done (Program Level)

- Rust path is default and stable for production workloads.
- Quality metrics are equal or better than prior baseline.
- C++ is no longer required for critical decompilation runtime paths.
- Regression governance is automated and enforced in CI.
