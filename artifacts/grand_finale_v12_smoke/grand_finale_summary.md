# GRAND-FINALE Report

- Generated: 2026-03-11 15:35:44
- Binaries: 4

## Global Summary

- Shared successful functions: 46
- Fission success count: 46
- Ghidra success count: 48
- Goto reduction vs Ghidra: 61.11%
- Fission switches / Ghidra switches: 3 / 1
- Fission for-loops / Ghidra for-loops: 31 / 9
- Fission do-while / Ghidra do-while: 36 / 52
- Failure classes (Fission): {'type': 1, 'timeout': 1}
- Failure classes (Ghidra): {}
- Type preservation hits (Fission): {}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 13, 'assembly_fallback': 1}
- Cast chains (Fission/Ghidra): 7 / 18
- MLIL preview success / residue / cast density: 48 / 0 / 0

## Residue Intel

### Single-Assign Temps
- `iVar72`: 109
- `uVar11`: 89
- `iVar69`: 59
- `uVar4`: 52
- `iVar4`: 51
- `iVar2`: 51
- `xVar7`: 43
- `iVar6`: 41
- `iVar78`: 31
- `uVar9`: 22
- `iVar3`: 20
- `xVar5`: 18
- `iVar20`: 17
- `uVar8`: 16
- `iVar7`: 12
- `uVar64`: 10
- `uVar74`: 9
- `uVar71`: 7
- `bVar16`: 6
- `uVar13`: 6

### Residue Names
- `iVar4`: 256
- `uVar11`: 250
- `iVar72`: 222
- `uVar4`: 156
- `iVar2`: 142
- `iVar69`: 137
- `iVar6`: 127
- `iVar78`: 81
- `uVar9`: 79
- `xVar7`: 78
- `iVar3`: 55
- `iVar20`: 53
- `iVar1`: 50
- `iVar18`: 50
- `uVar8`: 37
- `xVar5`: 36
- `iVar7`: 31
- `uVar74`: 25
- `uVar7`: 22
- `uVar64`: 22

### Residue Families
- `iVar`: 1249
- `uVar`: 904
- `xVar`: 185
- `bVar`: 14
- `raw_pointer_fallback`: 13
- `uStack`: 0
- `xStack`: 0
- `axStack`: 0
- `assembly_fallback`: 0
- `redundant_return_temp`: 0

### Top Offenders
- `everything` `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
- `everything` `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
- `everything` `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
- `everything` `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}
- `cmkr` `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
- `cmkr` `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
- `test_control_flow_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
- `test_structs_classes_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
- `cmkr` `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
- `cmkr` `0x1400034a0` `FUN_0x1400034a0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}

## Per-Binary Summary

### test_control_flow_x64_O0
- Shared success: 12 / 12 | Goto reduction: 0.00%
- Fission/Ghidra success: 12 / 12
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 0
- MLIL preview success / residue / cast density: 12 / 0 / 0
- Top residue offenders:
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}

### test_structs_classes_x64_O0
- Shared success: 12 / 12 | Goto reduction: 0.00%
- Fission/Ghidra success: 12 / 12
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 0
- MLIL preview success / residue / cast density: 12 / 0 / 0
- Top residue offenders:
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}

### cmkr
- Shared success: 11 / 12 | Goto reduction: 0.00%
- Fission/Ghidra success: 11 / 12
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {'type': 1} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 7
- MLIL preview success / residue / cast density: 12 / 0 / 0
- Top residue offenders:
  - `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
  - `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
  - `0x1400034a0` `FUN_0x1400034a0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
  - `0x1400036e0` `FUN_0x1400036e0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}

### everything
- Shared success: 11 / 12 | Goto reduction: 91.67%
- Fission/Ghidra success: 11 / 12
- Struct pointer hits: Fission 0, Ghidra 19
- Type preservation: Fission {}, Ghidra {'LPSTR': 3, 'LPCWSTR': 2}
- Failure classes: Fission {'timeout': 1} | Ghidra {}
- Cast chains: Fission 7 | Ghidra 11
- MLIL preview success / residue / cast density: 12 / 0 / 0
- Top residue offenders:
  - `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
  - `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
  - `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
  - `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}
  - `0x1400012e0` `FUN_0x1400012e0`: score=12, raw_pointer_fallback=0, single_assign_temps=2, top_names={'iVar1': 6, 'uVar3': 4}
