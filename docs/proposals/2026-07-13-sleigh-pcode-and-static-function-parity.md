# SLEIGH P-code And Static Function Parity

## 1. Baseline Row Anchor

- Binary: external dev/holdout PE corpus, x86 and x86-64 variants
- Functions: `count_bits` and `matrix_multiply` are representative p-code
  anchors; the full binary inventory is the discovery anchor
- Addresses: recorded by the external benchmark and not used by production
  logic
- Corpus row or benchmark command:
  `python3 -m runner.run_parity --corpus dev --limit 20 --decompilers fission,ghidra`
- Current output summary: strict raw p-code parity is 80/120. All 36 varnode
  mismatches preserve opcode count and sequence; representative x86-32
  constants differ as `0xfffffffc` versus `0xfffffffffffffffc`. Four remaining
  rows serialize conversion opcodes as `FLOATINT2FLOAT` and
  `FLOATFLOAT2FLOAT` rather than the public Ghidra mnemonics `INT2FLOAT` and
  `FLOAT2FLOAT`. Fission's requested-function boundary is direct for 10/10
  core rows, while full PE inventory misses untyped executable COFF symbols,
  including validated routines and IAT jump thunks.
- Residual after the first local full-matrix run: strict p-code is 120/120 and
  function inventory is 36/36. CFG has six mismatches where a direct recursive
  call target lies inside the reachable function but does not split the caller
  block, plus four topology fetch errors because the raw topology CLI enters
  NIR lowering for floating-point conversion opcodes.
- Residual after CFG/topology correction: assembly has 25 instruction-count
  mismatches because linear function disassembly includes unreachable alignment
  bytes and stops before reachable branch targets after an earlier return.
- Final cold-process local matrix: assembly 120/120, p-code 120/120, CFG
  120/120, function discovery 36/36, and IR invariants 120/120, with zero fetch
  errors.
- Semantic cases passed / total: not applicable; this slice measures raw lift
  and static inventory contracts
- Failure category: p-code varnode/opcode contract mismatch and static function
  discovery false negatives
- Relevant observations: x86-64 negative constants already agree because their
  varnode width is eight bytes. Missing inventory entries are executable COFF
  symbols whose type field is zero; blindly accepting all such symbols would
  also admit constructor/destructor data placed in `.text`.

## 2. Owner Proof

- [x] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation
- [x] Loader/static function facts

Evidence:

```text
x86-32 INT_ADD input:
reference: const offset=0xfffffffc size=4
candidate: const offset=0xfffffffffffffffc size=4

conversion opcode at the same sequence index and with equal varnodes:
reference: INT2FLOAT / FLOAT2FLOAT
candidate: FLOATINT2FLOAT / FLOATFLOAT2FLOAT

inventory:
x64 reference=84 candidate=71; the 13 missing executable entries consist of
one stack-probe routine and twelve six-byte IAT jump thunks.
x86 reference=71 candidate=70; the missing entry is the stack-probe routine.
```

The first wrong p-code fact is created by the canonical `Varnode::constant`
constructor and exposed by raw p-code serialization. The inventory false
negatives originate when the PE loader discards executable COFF symbols whose
type field is zero; static analysis is the owner that can safely validate those
ambiguous symbols before promotion.

## 3. Generality / Invariant Proof

Generalized rules:

```text
1. A constant-space varnode's unsigned offset is truncated to the varnode's
   declared byte width. Its signed `constant_val` remains available separately.
2. Public raw p-code mnemonics use Ghidra's documented opcode names, independent
   of Rust enum variant names.
3. A symbol in an executable COFF section with missing function type is an
   untrusted function seed, not a function fact.
4. Static analysis promotes a symbol seed only after SLEIGH decoding proves a
   valid function-shaped instruction stream or a terminal indirect jump through
   a known import-address-table slot.
5. A direct call whose target is inside the current reachable instruction set
   terminates an instruction-level basic block and emits both the call-target
   edge and, unless proven no-return, the fall-through edge.
6. Raw p-code topology is derived from the SLEIGH lift and never depends on NIR,
   HIR, structuring, or C rendering success.
7. Assembly, p-code, and CFG views consume the same SLEIGH-proven reachable
   instruction inventory. Linear byte ranges are not function membership
   evidence.
```

ISA-agnostic check:

- [x] Constant-width and opcode-name rules are p-code contracts, not ISA gates.
- [x] COFF section/storage/type facts remain in the loader; instruction meaning
      remains in SLEIGH/static analysis.
- [x] Tests describe width and symbol-seed validation without function names or
      benchmark addresses.

Comparable coverage:

- Similar shape 1: negative displacement constants at 1, 2, 4, and 8-byte widths
- Similar shape 2: untyped executable symbols representing both code and data
- Similar shape 3: direct self/mutual-recursive calls versus external calls
- Synthetic invariant tests: width-masked constant offsets, canonical opcode
  mnemonics, reject/accept cases for ambiguous COFF symbol seeds, internal-call
  CFG splitting, raw-topology serialization without NIR lowering, and exclusion
  of unreachable alignment bytes from the lifted instruction inventory

## 4. Risk And Ownership Check

- Existing owners: `pcode::Varnode`, the SLEIGH reachable-instruction/CFG lift,
  raw p-code CLI serialization, PE COFF symbol parsing, and
  `fission-static::function_discovery`
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
- Why extending those owners is sufficient: no new NIR pass or output cleanup is
  required. The loader preserves ambiguous evidence and static analysis proves
  or rejects it using the existing SLEIGH frontend.
- New helper justification: a typed loader-to-static seed record is needed so
  ambiguous symbols do not leak into the canonical function inventory before
  validation.
- Possible interactions: constant offsets are serialized and compared as
  unsigned values, while signed expression lowering continues to use
  `constant_val`. Function discovery precision must remain 100% on the focused
  binaries.
- New owner dependency: none; loader exports facts and static already consumes
  loader facts and SLEIGH.
- Telemetry impact: add accepted/rejected symbol-seed counts to the existing
  discovery report only if it can be done without a parallel schema.
- Known cases that must not change: x86-64 p-code rows already matching, existing
  typed COFF functions, and data symbols located in executable sections.

## 5. Validation Matrix

- [x] Targeted invariant tests
  - Command: `cargo nextest run -p fission-pcode -E 'test(pcode)'`
  - Command: `cargo nextest run -p fission-sleigh -E 'test(compiled_table)'`
  - Command: `cargo nextest run -p fission-loader -E 'test(coff)'`
  - Command: `cargo nextest run -p fission-static -E 'test(function_discovery)'`
  - Expected signal: width/opcode contracts and seed validation pass
- [x] Crate-level gates
  - Command: `cargo nextest run -p fission-pcode`
  - Command: `cargo nextest run -p fission-sleigh`
  - Command: `cargo nextest run -p fission-loader`
  - Command: `cargo nextest run -p fission-static`
- [x] Focused benchmark rows
  - Command: external local-Docker p-code and function-discovery stages
  - Result: p-code strict 120/120 and Fission inventory exact match on all 36
    measured PE variants
- [x] Smoke or automation sample
  - Command: `python3 -m runner.run_parity --corpus dev --decompilers fission,ghidra`
  - Result: 516/516 structural rows match; assembly, p-code, CFG, function
    discovery, and IR invariants each report 100% coverage and match rate
- [x] Boundary audit
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: no changed file introduces a NIR owner dependency; the scanner
    still reports the five pre-existing violations and 47 migration-debt edges

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] Yes, the local coding agent is implementing this proposal
- Information used locally:
  - [x] Structural failure pattern
  - [x] Owner evidence
  - [x] Invariant candidates
  - [x] Validation matrix
- Redaction confirmed for production code:
  - [x] No function-name condition
  - [x] No address condition
  - [x] No binary-path condition
  - [x] No corpus-row condition
  - [x] No compiler-tuple condition
- Ghidra guidance confirmed:
  - [x] Public p-code and inventory contracts only; no pseudocode style mimicry
- Unseen or synthetic validation evidence: synthetic invariant tests plus the
  external parity matrix; benchmark identities remain documentation-only

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard-only edits:
  - [x] Confirmed
- Any new helper is an owner-crossing fact contract rather than a new semantic
  pass:
  - [x] Confirmed
