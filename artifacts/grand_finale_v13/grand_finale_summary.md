# GRAND-FINALE Report

- Generated: 2026-03-12 10:28:01
- Binaries: 3

## Global Summary

- Shared successful functions: 87
- Fission success count: 87
- Ghidra success count: 90
- Goto reduction vs Ghidra: 47.01%
- Fission switches / Ghidra switches: 14 / 11
- Fission for-loops / Ghidra for-loops: 33 / 7
- Fission do-while / Ghidra do-while: 75 / 98
- Failure classes (Fission): {'type': 3}
- Failure classes (Ghidra): {}
- Type preservation hits (Fission): {'LPRECT': 1}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 29, 'assembly_fallback': 3}
- Cast chains (Fission/Ghidra): 27 / 37
- MLIL preview success / residue / cast density: 90 / 0 / 0

## Preview vs Legacy

| Metric | Preview | Legacy/Fission |
| --- | ---: | ---: |
| Engine used count | 87 | 87 |
| Fallback count | 3 | n/a |
| Goto count | 0 | 142 |
| Temp surface count | 0 | 5151 |
| Cast density | 0 | 27 |

## Residue Intel

### Single-Assign Temps
- `iVar7`: 180
- `iVar72`: 109
- `iVar6`: 101
- `uVar11`: 91
- `iVar4`: 70
- `iVar69`: 59
- `xVar30`: 59
- `iVar18`: 59
- `uVar4`: 55
- `iVar2`: 53
- `xVar7`: 43
- `iVar3`: 42
- `iVar21`: 38
- `iVar5`: 37
- `uVar9`: 37
- `iVar8`: 36
- `iVar78`: 31
- `xVar11`: 31
- `xVar58`: 29
- `uVar55`: 27

### Residue Names
- `iVar7`: 436
- `iVar4`: 323
- `uVar11`: 254
- `iVar6`: 250
- `iVar72`: 222
- `iVar18`: 217
- `xVar13`: 214
- `uVar4`: 165
- `iVar2`: 149
- `iVar69`: 137
- `iVar3`: 129
- `uVar9`: 121
- `xVar30`: 107
- `iVar5`: 97
- `iVar21`: 96
- `iVar1`: 93
- `iVar8`: 82
- `iVar78`: 81
- `xVar7`: 78
- `uVar44`: 58

### Residue Families
- `iVar`: 2600
- `uVar`: 1654
- `xVar`: 816
- `bVar`: 81
- `raw_pointer_fallback`: 29
- `redundant_return_temp`: 3
- `uStack`: 0
- `xStack`: 0
- `axStack`: 0
- `assembly_fallback`: 0

### Top Offenders
- `putty` `0x140001160` `FUN_0x140001160`: score=1658, raw_pointer_fallback=16, single_assign_temps=394, top_names={'iVar18': 167, 'xVar30': 107, 'iVar21': 96, 'uVar44': 58, 'xVar58': 47}
- `everything` `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
- `everything` `0x14014df40` `FUN_0x14014df40`: score=1135, raw_pointer_fallback=0, single_assign_temps=303, top_names={'iVar7': 353, 'xVar13': 214, 'iVar6': 65, 'xVar11': 45, 'iVar8': 30}
- `everything` `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
- `everything` `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
- `everything` `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}
- `cmkr` `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
- `cmkr` `0x1400048b0` `FUN_0x1400048b0`: score=56, raw_pointer_fallback=0, single_assign_temps=17, top_names={'uVar2': 9, 'uVar8': 9, 'iVar3': 7, 'xVar6': 7, 'uVar9': 7}
- `putty` `0x140007710` `FUN_0x140007710`: score=44, raw_pointer_fallback=0, single_assign_temps=12, top_names={'iVar1': 8, 'uVar7': 7, 'iVar11': 5, 'xVar6': 4, 'uVar9': 4}
- `putty` `0x140006cf0` `FUN_0x140006cf0`: score=30, raw_pointer_fallback=0, single_assign_temps=8, top_names={'xVar9': 7, 'xVar8': 6, 'xVar3': 3, 'xVar4': 3, 'xVar5': 3}

## Per-Binary Summary

### everything
- Shared success: 30 / 30 | Goto reduction: 39.76%
- Fission/Ghidra success: 30 / 30
- Struct pointer hits: Fission 0, Ghidra 19
- Type preservation: Fission {}, Ghidra {'LPSTR': 3, 'LPCWSTR': 2}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 7 | Ghidra 14
- MLIL preview success / residue / cast density: 30 / 0 / 0
- Preview engine used / fallback / goto / temp surface: 29 / 1 / 0 / 0
- Top residue offenders:
  - `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
  - `0x14014df40` `FUN_0x14014df40`: score=1135, raw_pointer_fallback=0, single_assign_temps=303, top_names={'iVar7': 353, 'xVar13': 214, 'iVar6': 65, 'xVar11': 45, 'iVar8': 30}
  - `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
  - `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
  - `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}

### putty
- Shared success: 28 / 30 | Goto reduction: 50.00%
- Fission/Ghidra success: 28 / 30
- Struct pointer hits: Fission 1, Ghidra 83
- Type preservation: Fission {'LPRECT': 1}, Ghidra {'LPCSTR': 2, 'LPCWSTR': 1, 'LPRECT': 1}
- Failure classes: Fission {'type': 2} | Ghidra {}
- Cast chains: Fission 20 | Ghidra 16
- MLIL preview success / residue / cast density: 30 / 0 / 0
- Preview engine used / fallback / goto / temp surface: 28 / 2 / 0 / 0
- Top residue offenders:
  - `0x140001160` `FUN_0x140001160`: score=1658, raw_pointer_fallback=16, single_assign_temps=394, top_names={'iVar18': 167, 'xVar30': 107, 'iVar21': 96, 'uVar44': 58, 'xVar58': 47}
  - `0x140007710` `FUN_0x140007710`: score=44, raw_pointer_fallback=0, single_assign_temps=12, top_names={'iVar1': 8, 'uVar7': 7, 'iVar11': 5, 'xVar6': 4, 'uVar9': 4}
  - `0x140006cf0` `FUN_0x140006cf0`: score=30, raw_pointer_fallback=0, single_assign_temps=8, top_names={'xVar9': 7, 'xVar8': 6, 'xVar3': 3, 'xVar4': 3, 'xVar5': 3}
  - `0x1400073d0` `FUN_0x1400073d0`: score=23, raw_pointer_fallback=0, single_assign_temps=7, top_names={'bVar4': 5, 'xVar2': 4, 'xVar3': 4, 'uVar1': 3}
  - `0x1400062f0` `FUN_0x1400062f0`: score=16, raw_pointer_fallback=0, single_assign_temps=4, top_names={'xVar2': 4, 'xVar3': 4, 'xVar4': 4}

### cmkr
- Shared success: 29 / 30 | Goto reduction: 100.00%
- Fission/Ghidra success: 29 / 30
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {'type': 1} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 7
- MLIL preview success / residue / cast density: 30 / 0 / 0
- Preview engine used / fallback / goto / temp surface: 30 / 0 / 0 / 0
- Top residue offenders:
  - `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
  - `0x1400048b0` `FUN_0x1400048b0`: score=56, raw_pointer_fallback=0, single_assign_temps=17, top_names={'uVar2': 9, 'uVar8': 9, 'iVar3': 7, 'xVar6': 7, 'uVar9': 7}
  - `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140004240` `FUN_0x140004240`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
