# Local quality observation tools

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
