# GRAND-FINALE Report

- Generated: 2026-03-12 09:58:28
- Binaries: 1

## Global Summary

- Shared successful functions: 3
- Fission success count: 3
- Ghidra success count: 3
- Goto reduction vs Ghidra: 0.00%
- Fission switches / Ghidra switches: 0 / 0
- Fission for-loops / Ghidra for-loops: 3 / 0
- Fission do-while / Ghidra do-while: 0 / 0
- Failure classes (Fission): {}
- Failure classes (Ghidra): {}
- Type preservation hits (Fission): {}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 0, 'assembly_fallback': 0}
- Cast chains (Fission/Ghidra): 0 / 0
- MLIL preview success / residue / cast density: 3 / 0 / 0

## Preview vs Legacy

| Metric | Preview | Legacy/Fission |
| --- | ---: | ---: |
| Engine used count | 3 | 3 |
| Fallback count | 0 | n/a |
| Goto count | 0 | 3 |
| Temp surface count | 0 | 13 |
| Cast density | 0 | 0 |

## Residue Intel

### Single-Assign Temps
- `bVar16`: 3
- `xVar8`: 1
- `iVar6`: 1

### Residue Names
- `bVar16`: 7
- `iVar6`: 3
- `xVar8`: 3

### Residue Families
- `bVar`: 7
- `iVar`: 3
- `xVar`: 3
- `uVar`: 0
- `uStack`: 0
- `xStack`: 0
- `axStack`: 0
- `raw_pointer_fallback`: 0
- `assembly_fallback`: 0
- `redundant_return_temp`: 0

### Top Offenders
- `test_control_flow_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}

## Per-Binary Summary

### test_control_flow_x64_O0
- Shared success: 3 / 3 | Goto reduction: 0.00%
- Fission/Ghidra success: 3 / 3
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 0
- MLIL preview success / residue / cast density: 3 / 0 / 0
- Preview engine used / fallback / goto / temp surface: 3 / 0 / 0 / 0
- Top residue offenders:
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
