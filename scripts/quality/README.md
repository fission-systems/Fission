# Local quality observation tools

## `golden_corpus_check.py`

Fast **local regression gate**: no Docker, no Ghidra reference. Batch-
decompiles a curated corpus (`scripts/quality/golden_corpus_check.py`'s
`DEFAULT_BINARIES`, ~16 binaries x 10 functions) with the local
`fission_cli` build and diffs NIR/HIR text against a checked-in golden
snapshot (`scripts/quality/golden_corpus/snapshot.json`). Also re-runs a
couple of known-heavy functions several times to catch run-to-run
nondeterminism cheaply.

| Layer | Role |
|-------|------|
| This tool | Broad-coverage local byte-diff + determinism gate, minutes not tens-of-minutes |
| `local_decomp_observe.py` | Deep single-function before/after investigation |
| `fission-benchmark` docker | Official semantic oracle / ranking / Ghidra parity (required for quality claims) |

### Workflow

```bash
# Build the fast local profile (see README.md's "Build the CLI")
cargo build -p fission-cli --profile quick-release

# Before/after a migration or perf slice:
python3 scripts/quality/golden_corpus_check.py check

# If the diff is expected (reviewed, e.g. via local_decomp_observe.py):
python3 scripts/quality/golden_corpus_check.py update
git add scripts/quality/golden_corpus/snapshot.json
```

`check` exits non-zero on any NIR/HIR mismatch, new/missing function, or
nondeterministic repeat-run. Widen coverage for a one-off deeper pass with
`--binaries ... --limit N`; the defaults are intentionally small so this
stays fast enough to run on every slice.

### Policy

Same as `local_decomp_observe.py` below: not a substitute for the Docker
ranking/parity oracle. A clean `golden_corpus_check.py check` says "this
change didn't move anything in the curated corpus," not "this change is
correct" -- it does not replace the fission-benchmark loop for quality
claims (Core Rule 10).

## `local_decomp_observe.py`

Focused **before / after** decompilation capture for one function.

| Layer | Role |
|-------|------|
| This tool | Cheap local NIR/HIR text + structural metrics + unified diff |
| `fission-benchmark` docker | Official semantic oracle / ranking (required for quality claims) |

Artifacts (gitignored): `benchmark/artifacts/local_observe/<session>/`

### Workflow

```bash
# 1) Before a semantic fix (on unclean tree is fine — records git_dirty)
python3 scripts/quality/local_decomp_observe.py baseline \
  --binary /path/to/sample.exe \
  --addr 0x4015b0 \
  --source /path/to/file.c \
  --source-symbol optional_function_name \
  --label my-case

# 2) Implement + rebuild
cargo build -p fission-cli --release

# 3) After
python3 scripts/quality/local_decomp_observe.py after --session my-case
python3 scripts/quality/local_decomp_observe.py show --session my-case
```

If you already fixed without a live baseline, import a saved snippet:

```bash
python3 scripts/quality/local_decomp_observe.py import-baseline \
  --label my-case \
  --nir-file /tmp/before_nir.c \
  --binary /path/to/sample.exe \
  --addr 0x4015b0 \
  --source /path/to/file.c \
  --source-symbol optional_function_name

python3 scripts/quality/local_decomp_observe.py after --session my-case
python3 scripts/quality/local_decomp_observe.py show --session my-case
```

### Output layout

```text
benchmark/artifacts/local_observe/<session>/
  meta.json
  source.c                 # optional
  before/{capture.json,nir.c,hir.c}
  after/{capture.json,nir.c,hir.c}
  report.md                # source + before + after + metrics
  report.json
  diff_nir.patch
  diff_hir.patch
```

### Policy

- Local observe is **not** a substitute for the docker ranking / oracle loop.
- Do not promote `local_observe` results to Pages or official latest.
- Prefer invariant language in commits; keep row identity in proposal docs only.
