# GRAND-FINALE Report

- Generated: 2026-03-11 14:16:22
- Binaries: 4

## Global Summary

- Shared successful functions: 119
- Fission success count: 119
- Ghidra success count: 120
- Goto reduction vs Ghidra: 34.62%
- Fission switches / Ghidra switches: 11 / 6
- Fission for-loops / Ghidra for-loops: 50 / 20
- Fission do-while / Ghidra do-while: 67 / 91
- Failure classes (Fission): {'type': 1}
- Failure classes (Ghidra): {}
- Type preservation hits (Fission): {}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 13, 'assembly_fallback': 1}
- Cast chains (Fission/Ghidra): 9 / 24

## Residue Intel

### Single-Assign Temps
- `iVar7`: 180
- `iVar72`: 109
- `iVar6`: 103
- `uVar11`: 91
- `iVar4`: 73
- `iVar69`: 59
- `iVar2`: 55
- `uVar4`: 55
- `xVar7`: 43
- `iVar3`: 42
- `uVar9`: 37
- `iVar5`: 37
- `iVar8`: 36
- `iVar78`: 31
- `xVar5`: 24
- `xVar11`: 24
- `uVar8`: 20
- `uVar2`: 18
- `iVar20`: 17
- `uVar1`: 15

### Residue Names
- `iVar7`: 436
- `iVar4`: 331
- `iVar6`: 256
- `uVar11`: 254
- `iVar72`: 222
- `xVar13`: 214
- `uVar4`: 165
- `iVar2`: 155
- `iVar69`: 137
- `iVar3`: 129
- `uVar9`: 121
- `iVar5`: 97
- `iVar8`: 82
- `iVar1`: 81
- `iVar78`: 81
- `xVar7`: 78
- `uVar2`: 61
- `xVar5`: 53
- `iVar20`: 53
- `uVar1`: 51

### Residue Families
- `iVar`: 2199
- `uVar`: 1176
- `xVar`: 501
- `bVar`: 58
- `raw_pointer_fallback`: 13
- `redundant_return_temp`: 2
- `uStack`: 0
- `xStack`: 0
- `axStack`: 0
- `assembly_fallback`: 0

### Top Offenders
- `everything` `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
- `everything` `0x14014df40` `FUN_0x14014df40`: score=1135, raw_pointer_fallback=0, single_assign_temps=303, top_names={'iVar7': 353, 'xVar13': 214, 'iVar6': 65, 'xVar11': 45, 'iVar8': 30}
- `everything` `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
- `everything` `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
- `everything` `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}
- `cmkr` `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
- `cmkr` `0x1400048b0` `FUN_0x1400048b0`: score=56, raw_pointer_fallback=0, single_assign_temps=17, top_names={'uVar2': 9, 'uVar8': 9, 'iVar3': 7, 'xVar6': 7, 'uVar9': 7}
- `test_control_flow_x64_O0` `0x140001a00` `main`: score=38, raw_pointer_fallback=0, single_assign_temps=12, top_names={'uVar1': 23, 'xVar2': 3}
- `test_structs_classes_x64_O0` `0x140001e40` `d_ref_qualifier`: score=35, raw_pointer_fallback=0, single_assign_temps=7, top_names={'uVar5': 12, 'uVar2': 8, 'iVar4': 8}
- `test_structs_classes_x64_O0` `0x1400016e7` `main`: score=32, raw_pointer_fallback=0, single_assign_temps=9, top_names={'xVar5': 11, 'uVar2': 7, 'uVar3': 5}

## Per-Binary Summary

### test_control_flow_x64_O0
- Shared success: 30 / 30 | Goto reduction: 0.00%
- Fission/Ghidra success: 30 / 30
- Struct pointer hits: Fission 0, Ghidra 2
- Type preservation: Fission {}, Ghidra {'LPVOID': 2}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 1
- Top residue offenders:
  - `0x140001a00` `main`: score=38, raw_pointer_fallback=0, single_assign_temps=12, top_names={'uVar1': 23, 'xVar2': 3}
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
  - `0x140001c80` `__do_global_ctors`: score=15, raw_pointer_fallback=0, single_assign_temps=3, top_names={'uVar2': 9, 'uVar3': 3}
  - `0x140001d00` `__main`: score=15, raw_pointer_fallback=0, single_assign_temps=3, top_names={'uVar2': 9, 'uVar3': 3}
  - `0x1400019b3` `fibonacci(int)`: score=4, raw_pointer_fallback=0, single_assign_temps=1, top_names={'iVar1': 3}

### test_structs_classes_x64_O0
- Shared success: 30 / 30 | Goto reduction: 11.76%
- Fission/Ghidra success: 30 / 30
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 2 | Ghidra 2
- Top residue offenders:
  - `0x140001e40` `d_ref_qualifier`: score=35, raw_pointer_fallback=0, single_assign_temps=7, top_names={'uVar5': 12, 'uVar2': 8, 'iVar4': 8}
  - `0x1400016e7` `main`: score=32, raw_pointer_fallback=0, single_assign_temps=9, top_names={'xVar5': 11, 'uVar2': 7, 'uVar3': 5}
  - `0x140001f10` `d_count_templates_scopes`: score=24, raw_pointer_fallback=0, single_assign_temps=3, top_names={'uVar1': 13, 'uVar2': 8}
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
  - `0x1400020e0` `d_growable_string_callback_adapter`: score=15, raw_pointer_fallback=0, single_assign_temps=2, top_names={'uVar3': 8, 'uVar1': 5}

### cmkr
- Shared success: 29 / 30 | Goto reduction: 100.00%
- Fission/Ghidra success: 29 / 30
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {'type': 1} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 7
- Top residue offenders:
  - `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
  - `0x1400048b0` `FUN_0x1400048b0`: score=56, raw_pointer_fallback=0, single_assign_temps=17, top_names={'uVar2': 9, 'uVar8': 9, 'iVar3': 7, 'xVar6': 7, 'uVar9': 7}
  - `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140004240` `FUN_0x140004240`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}

### everything
- Shared success: 30 / 30 | Goto reduction: 39.76%
- Fission/Ghidra success: 30 / 30
- Struct pointer hits: Fission 0, Ghidra 19
- Type preservation: Fission {}, Ghidra {'LPSTR': 3, 'LPCWSTR': 2}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 7 | Ghidra 14
- Top residue offenders:
  - `0x140183590` `FUN_0x140183590`: score=1574, raw_pointer_fallback=0, single_assign_temps=382, top_names={'iVar72': 222, 'uVar11': 174, 'uVar4': 156, 'iVar69': 137, 'iVar78': 81}
  - `0x14014df40` `FUN_0x14014df40`: score=1135, raw_pointer_fallback=0, single_assign_temps=303, top_names={'iVar7': 353, 'xVar13': 214, 'iVar6': 65, 'xVar11': 45, 'iVar8': 30}
  - `0x140123c80` `FUN_0x140123c80`: score=637, raw_pointer_fallback=0, single_assign_temps=159, top_names={'iVar2': 142, 'iVar6': 82, 'iVar4': 58, 'iVar20': 53, 'iVar18': 50}
  - `0x14011d840` `FUN_0x14011d840`: score=417, raw_pointer_fallback=0, single_assign_temps=130, top_names={'xVar7': 78, 'uVar11': 76, 'iVar4': 70, 'iVar3': 22, 'iVar9': 10}
  - `0x1401120c0` `FUN_0x1401120c0`: score=245, raw_pointer_fallback=0, single_assign_temps=20, top_names={'iVar4': 128, 'iVar1': 44, 'iVar6': 39, 'iVar3': 8, 'uVar2': 3}
