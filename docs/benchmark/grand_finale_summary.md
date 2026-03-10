# GRAND-FINALE Report

- Generated: 2026-03-10 20:09:11
- Binaries: 3

## Global Summary

- Shared successful functions: 60
- Fission success count: 60
- Ghidra success count: 60
- Goto reduction vs Ghidra: 50.50%
- Fission switches / Ghidra switches: 9 / 9
- Fission for-loops / Ghidra for-loops: 16 / 11
- Fission do-while / Ghidra do-while: 16 / 22

## Residue Intel

### Single-Assign Temps
- `xVar30`: 59
- `iVar18`: 59
- `iVar21`: 38
- `xVar58`: 29
- `uVar55`: 27
- `xStack_948`: 23
- `iVar57`: 21
- `xStack_940`: 20
- `xVar20`: 20
- `uVar44`: 16
- `uVar17`: 14
- `bVar60`: 14
- `uVar47`: 12
- `uStack_914`: 10
- `uVar14`: 10
- `uVar36`: 10
- `uVar24`: 9
- `uStack_915`: 9
- `bVar61`: 8
- `xVar11`: 7

### Residue Names
- `iVar18`: 167
- `xVar30`: 107
- `iVar21`: 96
- `axStack_968`: 64
- `uStack_48`: 64
- `axStack_848`: 61
- `uVar44`: 58
- `xVar58`: 47
- `iVar57`: 46
- `uVar17`: 45
- `uVar55`: 45
- `xStack_948`: 45
- `axStack_8b0`: 42
- `uVar24`: 38
- `xStack_940`: 38
- `axStack_8b8`: 38
- `uStack_914`: 35
- `axStack_838`: 34
- `xVar20`: 32
- `uVar16`: 27

### Residue Families
- `uVar`: 608
- `iVar`: 425
- `xVar`: 306
- `axStack`: 290
- `xStack`: 188
- `uStack`: 135
- `bVar`: 55

## Per-Binary Summary

### putty
- Shared success: 20 / 20 | Goto reduction: 50.80%
- Fission/Ghidra success: 20 / 20
- Struct pointer hits: Fission 1, Ghidra 71

### test_control_flow_x64_O0
- Shared success: 20 / 20 | Goto reduction: 0.00%
- Fission/Ghidra success: 20 / 20
- Struct pointer hits: Fission 0, Ghidra 0

### test_structs_classes_x64_O0
- Shared success: 20 / 20 | Goto reduction: 58.33%
- Fission/Ghidra success: 20 / 20
- Struct pointer hits: Fission 0, Ghidra 0
