# Mandatory external benchmark loop (fission-benchmark)

**Policy (2026-07-10):** Decompiler quality work that claims row-level or corpus
improvement **must** run the external multi-decompiler benchmark via
`/Users/sjkim1127/fission-benchmark/docker-compose.yml` (with the **local**
overlay for in-tree Fission builds). Do not promote “fixed” from unit tests or
one-off `fission_cli decomp` alone.

Official CI/Pages runs use **GitHub Release** Fission only. Local quality loops
use the **local** profile and must **never** be published as official latest.

## Paths

| Item | Path |
|------|------|
| Benchmark repo | `/Users/sjkim1127/fission-benchmark` |
| Compose (base) | `/Users/sjkim1127/fission-benchmark/docker-compose.yml` |
| Compose (local Fission) | `/Users/sjkim1127/fission-benchmark/docker-compose.local.yml` |
| Prepare local CLI bundle | `scripts/prepare_local_fission.sh` in the benchmark repo |
| Fission monorepo | `/Users/sjkim1127/Fission` (override with `FISSION_ROOT`) |

## Local quality loop (required for semantic fixes)

```bash
# 1) From fission-benchmark: bake current Fission tree into a Linux CLI bundle
cd /Users/sjkim1127/fission-benchmark
export FISSION_ROOT=/Users/sjkim1127/Fission
./scripts/prepare_local_fission.sh

# 2) Start local Fission service (profile local + both compose files)
docker compose -f docker-compose.yml -f docker-compose.local.yml \
  --profile local up -d --build fission

# 3) Health
curl -sf "http://localhost:${FISSION_HOST_PORT:-8007}/health" | jq .

# 4) Focused run (example: m32 control_flow / residual functions)
# Prefer fission-only for fast loops; expand decompilers for go/stop.
# Local compose exposes Fission on FISSION_HOST_PORT (default 8007), not 8000.
export FISSION_ENDPOINT="http://localhost:${FISSION_HOST_PORT:-8007}"
python runner/runner.py --corpus dev --decompilers fission \
  --limit 40 \
  --output "results/local_$(git -C "$FISSION_ROOT" rev-parse --short HEAD).json"

# Optional: full multi-decompiler comparison (slower; needs other services up)
# docker compose -f docker-compose.yml up -d
# python runner/runner.py --corpus dev --decompilers fission,ghidra
```

Notes:

- Default official `fission` service is release-backed on port **8000**.
- Local overlay typically uses **`FISSION_HOST_PORT` (default 8007)** — set
  `FISSION_ENDPOINT=http://localhost:8007` when the runner must hit local.
- Do **not** promote local docker images or local runner results to dashboard
  rankings / “latest”.
- After a semantic fix: re-run with **no stale decompilation cache** when the
  runner supports cache disable flags (see runner `--help`).

## When to run

| Change type | Minimum benchmark |
|-------------|-------------------|
| NIR materialize / return / cmov / structuring | Local fission docker + focused `runner.py` on motivating corpus rows |
| Broader quality claim | Same + smoke/holdout policy from Agents.md; no cache when measuring |
| Docs-only / pure unit debt tables | Optional |

## Agent / contributor rule

Standing rule for AI-assisted and human quality cycles:

1. Implement invariant at the owner (see ADR 0006 / 0009).
2. Unit / crate tests.
3. **Always** exercise `/Users/sjkim1127/fission-benchmark/docker-compose.yml`
   local loop above before claiming row improvement.
4. Report both CLI one-shot and runner/oracle movement.

Also linked from root `Agents.md` (Decompiler Quality Loop).
