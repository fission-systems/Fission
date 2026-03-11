# GRAND-FINALE Report

- Generated: 2026-03-11 15:11:23
- Binaries: 1

## Global Summary

- Shared successful functions: 0
- Fission success count: 2
- Ghidra success count: 0
- Goto reduction vs Ghidra: 0.00%
- Fission switches / Ghidra switches: 0 / 0
- Fission for-loops / Ghidra for-loops: 0 / 0
- Fission do-while / Ghidra do-while: 0 / 0
- Failure classes (Fission): {}
- Failure classes (Ghidra): {'other': 2}
- Type preservation hits (Fission): {}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 0, 'assembly_fallback': 0}
- Cast chains (Fission/Ghidra): 0 / 0
- MLIL preview success / residue / cast density: 2 / 18 / 0

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
- `test_structs_classes_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}

## Per-Binary Summary

### test_structs_classes_x64_O0
- Shared success: 0 / 2 | Goto reduction: 0.00%
- Fission/Ghidra success: 2 / 0
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {'other': 2}
- Cast chains: Fission 0 | Ghidra 0
- MLIL preview success / residue / cast density: 2 / 18 / 0
- Top residue offenders:
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
