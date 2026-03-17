# Powder Fixed Seeds

[`samples/windows/x64/Powder.exe`](../../samples/windows/x64/Powder.exe) is an x64 single-EXE game/simulation corpus. Compared with utility-style binaries, it is useful for studying state machines, event/update loops, custom data structures, and giant-function shapes, so it is part of the x64 practical regression set.

## Seed Policy

- source: `fission_cli --list`
- filter: exclude `[import]` and zero-sized functions
- strategy: fix five seeds from the internal non-zero function list using size quantiles: `small / medium / medium-heavy / heavy / giant`
- purpose: repeatedly measure direct preview, fallback distribution, and giant-function readability on an x64 game-style single EXE

The canonical seed manifest lives in [`powder_fixed_seeds.json`](./powder_fixed_seeds.json).

## Fixed Seeds

- `0x140394a5d`
- `0x1401e6049`
- `0x1404b761c`
- `0x140161bc0`
- `0x14043f1c8`

## Baseline Summary

| Binary | Filtered Functions | Direct Preview | Fallback | Legacy | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `Powder.exe` | 16729 | 4 | 1 | 1 | 0 |

## Artifact Policy

The checked-in seed manifest is the stable benchmark reference. Large compare outputs are generated locally and should be treated as ephemeral artifacts rather than repository source of truth.

## Initial Read

1. `4/5` complete through direct `mlil_preview`.
2. The legacy path failed on all `5/5` selected seeds in that round.
3. `0x14043f1c8` is a giant-function case that currently ends in explicit assembly fallback rather than direct preview.
4. `0x140394a5d` succeeds in direct preview but still leaves `xVar` / `reg` readability residue, making it a strong quality target.

## Watchlist

1. `0x14043f1c8`
   - x64 giant fallback case
   - continue tracking whether direct preview becomes possible while preserving timeout-free explicit fallback behavior
2. `0x140394a5d`
   - direct preview readability case
   - target for temp / register / branch-surface cleanup
