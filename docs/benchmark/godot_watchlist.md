# Godot Watchlist

[`samples/windows/x64/Godot_v4.6.1-stable_win64.exe`](../../samples/windows/x64/Godot_v4.6.1-stable_win64.exe) is a 164 MB x64 single-EXE engine sample. It is better treated as a **secondary stress/watchlist corpus** than as part of the routine fixed-seed regression set. Even `fission_cli --list` does not finish quickly on this binary, so it is handled as a one-time long-scan seed source rather than a normal repeatedly scanned corpus.

## Watchlist Role

- role: x64 large-engine / giant-function / timeout-closure stress corpus
- purpose: strengthen worst-case large-engine coverage rather than increase routine regression counts
- rerun policy: do not re-run full `--list` as part of routine regression; only re-run the three targeted addresses below

The canonical watchlist manifest lives in [`godot_watchlist.json`](./godot_watchlist.json).

## Seed Policy

- source: one-time x64 `.pdata` exception-table scan
- scan result: `209650` function ranges from `.pdata`
- strategy: manual curation instead of quantile-based fixed seeds
- fixed roles:
  - `smaller-heavy`
  - `heavy`
  - `giant`

This round originally tried to identify direct-preview readability candidates as well, but the representative cases did not finish even within a 120-second budget. The current baseline therefore functions primarily as a **timeout watchlist**.

## Fixed Watchlist

- `0x144574a30`
- `0x14435bac0`
- `0x141e17130`

## Baseline Summary

| Binary | Seeds | Direct Preview | Explicit Fallback | Legacy Success | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `Godot_v4.6.1-stable_win64.exe` | 3 | 0 | 0 | 0 | 3 |

## Artifact Policy

The reproducible seed manifest is checked into the repository. Large compare outputs are generated locally and are not treated as checked-in source-of-truth artifacts.

## Why These Three

1. `0x144574a30`
   - `smaller-heavy` lower-bound case
   - times out for both legacy and preview even under a 20-second compare budget despite a `.pdata` size of only `121` bytes
2. `0x14435bac0`
   - `heavy` timeout case
   - `.pdata` size `5713` bytes, representative of engine-scale heavy bodies
3. `0x141e17130`
   - `giant` worst-case case
   - `.pdata` size `296491` bytes, one of the largest giant-function timeout cases in the binary

## Initial Read

1. This sample is more accurately treated as a **timeout-closure / large-engine resilience** watchlist than as part of general fixed-seed regression.
2. All three seeds ended in the same timeout outcome for both legacy and preview under the 20-second budget.
3. It is better used as a targeted giant-function timeout watchlist, and later as a stress corpus for ARM64-scale large-engine work, than as a routine regression input.
