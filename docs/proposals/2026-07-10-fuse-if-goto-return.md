# Decompiler Change Proposal: Fuse if-goto with return-bearing segments

## 1. Baseline

- saturating_add O2: `if (param_2 <= 0) goto L; … return INT_MAX; L: return eax`
- Fuse pass refused the segment because `Return` was not "fuseable linear"

## 2. Owner

- [x] Normalize cleanup `fuse_single_predecessor_boundaries` /
  `stmt_is_fuseable_linear`

## 3. Invariant

```text
Statements between `if (c) goto L;` and label `L` may include Return (and other
straight-line ops). Early returns do not make the segment non-linear for the
purpose of if-goto inversion into `if (!c) { segment }`.
```

## 4. Validation

- Unit: `fuse_if_goto_allows_returns_in_segment`
- Real: saturating_add O2 loses bare `goto` before label when segment fuses
- Docker: docs/BENCHMARK_DOCKER.md local loop
