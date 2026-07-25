# ADR 0014: Dual reconstruction paths — product DIR vs pre-structure substrate

**Status:** Accepted  
**Last verified:** 2026-07-25

## Context

The token **DIR** is overloaded in Fission.

### A. Code substrate (shipped today)

`fission-midend-dir` owns **pre-structuring** midend types (`DirStmt` /
`DirExpr` / `DirFunction`), the DIR-typed action pipeline, DIR VSA, and pure
DIR helpers. Pipeline shape:

```text
p-code → builder raise → Dir* (flat goto/label AST)
       → normalize
       → structure (CFG-to-AST on Dir*)
       → HirFunction
       → dual-layer print (NIR / HIR profiles; ADR 0011)
```

In this layout, “DIR” is **not** a sibling product of HIR. It is the
**static intermediate** consumed by normalize/structuring, then converted to
`HirFunction` (`dir_stmts_to_hir_stmts`). ADR 0012 describes this substrate as
the pre-structuring counterpart to `fission-midend-core`’s HIR types.

### B. Product concept (exploratory)

A separate design thread treats DIR as an **independent reconstruction
strategy** beside HIR, both rooted in the same low-level semantic IR:

```text
Machine code
    → Low-level semantic IR (foundation; see §2)
         ├── HIR path: fast static reconstruction
         └── DIR path: slower, execution- / constraint-validated reconstruction
```

In that model:

- DIR is **not** a layer above HIR (`print-NIR → HIR → DIR` is wrong).
- HIR asks: *what is the most readable reconstruction under static analysis?*
- DIR asks: *what readable reconstruction can we validate against the original
  low-level behavior (emulator, traces, solver)?*
- Full functional proof is not required; outputs carry an **assurance band**
  (e.g. Proven / Conditionally Proven / Path Validated / Trace Validated /
  Unverified) plus assumptions (path budget, external calls, timeouts).

Using one name for both A and B causes design and review failures:

- “Structure on DIR” is read as “structure on the product path” when code means
  “structure on the flat pre-HIR AST”.
- “DIR is not above HIR” is true for B and false for A’s current pipeline.
- Emulator / selfjit / solver work gets mis-owned under midend-dir crates that
  only hold static AST helpers.

A second overload is **NIR**: dual-layer **print profile** (ADR 0011), historical
midend module name, and informal “low-level IR” in product writing. Emulator and
selfjit today step **`PcodeOp` / `PcodeOpcode`**, not `Dir*` / `HirFunction` /
print-NIR text. Hybrid decompile+emu+solver designs must not treat those as
interchangeable.

## Decision

### 1. Vocabulary split (binding for new docs and designs)

| Name | Role | Layer | Status |
|------|------|--------|--------|
| **Semantic foundation** | Executable, fidelity-first ops after lift — **p-code**, or a view **isomorphic** to p-code (same op meaning, sizes, spaces, control) | Foundation | Shipped as `PcodeOp` / `PcodeFunction` |
| **Pre-structure IR** | Flat raised AST used by static normalize + structure (`Dir*` types today) | Static pipeline intermediate | Shipped under the name `Dir*` |
| **HIR product** | Fast static high-level reconstruction + dual-layer print (ADR 0011) | Default decompile product | Shipped |
| **NIR print profile** | Mechanical C **text** from the structured tree (`PrintProfile::Nir`) | Presentation / oracle **string** | Shipped; **not** the foundation |
| **Validated reconstruction product** (concept name: **DIR product**) | Optional path: candidate high-level form + behavioral checks + assurance metadata | Sibling of HIR product, not a post-HIR polish stage | Exploratory; not a shipped public contract yet |

Rules:

1. New architecture text must not use bare “DIR” without saying **substrate**
   (`Dir*` / pre-structure IR) or **product** (validated reconstruction).
2. The DIR **product** must not be described as `NIR → HIR → DIR` or as a
   presentation pass over HIR. It shares the **semantic foundation** with HIR
   and may **reuse HIR candidates** as inputs to validation, but HIR is not its
   semantic oracle, and **NIR print text is not its oracle either**.
3. Until a rename lands, **code identifiers** (`DirStmt`, `fission-midend-dir`,
   `DirFunction`) continue to mean **pre-structure IR only**. Do not overload
   those types with assurance metadata or emulator coupling “because the product
   is also called DIR”.
4. Bare “NIR” in new dual-path / hybrid docs is forbidden unless qualified:
   **foundation** (only if p-code-isomorphic — prefer saying **p-code**),
   **print profile**, or **legacy midend name**. Default: say **p-code** for the
   shared executable IR.

### 2. Semantic foundation: p-code-isomorphic IR (not NIR print)

Decompiler, emulator, and solver form one hybrid system only if they share a
**single executable semantic foundation**. In Fission that foundation is:

> **p-code** (`PcodeOp` / `PcodeOpcode` / varnode spaces), or a derived IR that
> is **operationally isomorphic** to p-code (same observable step semantics).

**Isomorphic NIR (allowed as API/name only):** a normalized or SSA-shaped view
may be called “NIR” in product language **if and only if**:

1. op set and meaning match p-code (or a documented, tested refinement),
2. emulator (or a proven lowerer to p-code) can **execute** that view,
3. solver interprets the **same** op/memory/ABI model,
4. DIR-product equivalence compares observations against **that** foundation,
   not against HIR/print-NIR strings alone.

**Not the foundation (forbidden as sole shared IR for hybrid validation):**

| Surface | Why insufficient alone |
|---------|-------------------------|
| Pre-structure `Dir*` | Already raised; materialize choices blur lift vs structure bugs |
| `HirFunction` | Static product; presentation and recovery policy embedded |
| NIR **print** text (`PrintProfile::Nir`) | String surface; not an execution IR (ADR 0011 oracle is for **quality ranking of decompile text**, not emu step semantics) |
| HIR **print** text | Readability surface only |

Shipped reality (normative for ownership):

```text
        ┌── emulator / selfjit   (step / JIT PcodeOp)
p-code ─┼── solver experiments   (same op / BV / memory model)
        └── decompiler builder   (raise → pre-structure → structure → HIR)
                  ↑
         DIR product: candidates may come from static path;
         behavioral check is against foundation observations
```

**“Share NIR instead of p-code”** is therefore:

- **OK** only when “NIR” means a p-code-isomorphic foundation view (§2 conditions);
- **Not OK** when “NIR” means print profile, `Dir*`, or HIR.

Prefer documenting the foundation as **p-code** to match `fission-emulator` and
`fission-pcode` today; do not invent a second executable IR without a parity
contract and tests against p-code.

### 3. Dual-path product shape (accepted intent)

```text
Semantic foundation (p-code or p-code-isomorphic view)
    ├── Static path     → (pre-structure IR) → structure → HIR product
    │                        └→ dual-layer print (NIR / HIR profiles)
    └── Validated path  → candidates (+ optional static HIR) → execute / symbolic
                          check vs foundation → DIR product + assurance
```

- **HIR** remains the default, scalable decompile surface.
- **DIR product** is selective (regions, critical functions, research lane),
  not a mandatory whole-binary replacement for HIR.
- Decompiler, emulator, and solver improve as **shared consumers of the
  foundation**, not as a stack where only HIR or print-NIR is trusted.

### 4. Shared structuring substrate: foundation CFG facts

For both products, **control-structure candidate generation** should be owned
from **foundation CFG facts** (p-code basic blocks, BRANCH/CBRANCH edges,
dominance, post-dominance, SCC / SESE), not from a late HIR-only rewrite.

| Shared | Diverges by path |
|--------|------------------|
| Lift / p-code meaning | Accept policy (static vs validated) |
| CFG / region candidate discovery | Assurance attachment and failure bands |
| Optional block/region expression raise | Whole-function proof ambition |
| Small equivalence helpers (arith, cond) | Default on vs opt-in expensive checks |

Explicit non-goals of this ADR:

- Replacing the shipped static pipeline overnight.
- Implementing full translation validation for arbitrary binaries.
- Forking two complete structuring codebases; prefer **one candidate core**,
  two accept/assurance policies.
- Binding production decompile quality claims to DIR-product experiments without
  ADR 0006 measurement.
- Replacing p-code execution with print-NIR or HIR interpretation as the hybrid
  oracle.

### 5. Rename policy for the code substrate (planned, not blocking)

The long-term goal is to stop calling pre-structure IR “DIR” in product docs.

| Phase | Action |
|-------|--------|
| Now | Docs/ADRs use **pre-structure IR** / `Dir*` for substrate; **DIR product** for the validated path; **p-code** for foundation |
| Next | Prefer new APIs and comments that say pre-structure / flat IR; avoid expanding `Dir*` semantics toward validation |
| Later (optional mechanical rename) | `Dir*` → e.g. `Flat*` / `PreHir*` / `GotoIr*`, crate `fission-midend-dir` → name matching pre-structure role; free the token **DIR** for the validated product — only with a dedicated migration PR, not drive-by |

This ADR **accepts the dual-path vocabulary, foundation rule, and owner intent**.
It does **not** require an immediate identifier rename to stay consistent.

### 6. Owner boundaries (relative to existing ADRs)

| Concern | Owner | Notes |
|---------|--------|------|
| Semantic foundation (lift / p-code meaning) | `fission-pcode` (+ sleigh) | ADR 0002 / 0008; hybrid shared IR |
| Pre-structure IR types & static normalize helpers | `fission-midend-dir` / normalize | Code “DIR” = substrate only |
| Static structuring free functions | `fission-midend-structuring` | Prefer CFG facts from foundation over HIR-only repair |
| HIR presentation / dual-layer print (incl. NIR profile) | `render` + ADR 0011 | **Not** foundation; **not** DIR product |
| Emulator / selfjit differential | `fission-emulator` | Executes foundation (`PcodeOp`); DIR-product oracle |
| Constraint / solver experiments | dedicated solver surface (existing or future) | Same foundation model; must not silently alter print-NIR oracle text |
| DIR-product assurance schema / lane | **TBD** under a future ADR when first public contract ships | Until then: research / internal only |

ADR 0006 still gates **semantic quality** claims for the static path. DIR-product
work may land mechanical scaffolding and focused differential tests, but must
not claim decompiler quality wins without measured evidence.

## Consequences

**Positive**

- Stops treating “DIR above HIR” and “DIR before HIR” as the same design.
- Aligns dual-path exploration with a single **executable** foundation (p-code).
- Separates print-NIR / HIR oracle text from emu/solver step semantics.
- Gives rename and crate work a stable target language without blocking current
  static decompile.

**Costs / risks**

- Temporary dual meaning of “DIR” in oral discussion until rename or habit
  settles; mitigate with “substrate” vs “product” every time.
- Temporary dual meaning of “NIR”; mitigate with §1 rule 4 and “p-code” default.
- Risk of smuggling validation logic into `DirStmt` or normalize “for DIR”;
  reject in review using §1 rule 3.
- Risk of dual structuring stacks; reject unless a proposal proves the shared
  candidate core is insufficient.
- Risk of a second “semantic NIR” IR that drifts from p-code; require isomorphism
  contract + differential tests (§2) before any hybrid path depends on it.

**Follow-ups**

1. Architecture / PROJECT docs: one diagram with dual paths + foundation =
   p-code + current static pipeline (pre-structure IR labeled).
2. First DIR-product spike design: region-scoped Path/Trace Validated check
   **against p-code / foundation observations** (no full-function proof).
3. Optional rename RFC when `Dir*` churn cost is justified.
4. When a public DIR-product contract exists, add ADR 001x for assurance bands,
   CLI/API surface, and non-goals (syscalls, concurrency, SMC, etc.).

## Related

- [ADR 0002](0002-fission-pcode-canonical-semantics.md) — p-code crate owns
  shipped semantics
- [ADR 0006](0006-decompiler-quality-change-gate.md) — measurement gate
- [ADR 0008](0008-nir-substrate-and-owner-boundaries.md) — substrate vs owners
- [ADR 0011](0011-hir-presentation-contract.md) — HIR presentation / NIR print ≠
  executable foundation
- [ADR 0012](0012-midend-rename-and-crate-extraction.md) — midend crate split;
  `fission-midend-dir` as pre-structure substrate
