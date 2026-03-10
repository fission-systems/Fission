# GRAND-FINALE Report

- Generated: 2026-03-10 23:20:43
- Binaries: 3

## Global Summary

- Shared successful functions: 59
- Fission success count: 59
- Ghidra success count: 60
- Goto reduction vs Ghidra: 46.67%
- Fission switches / Ghidra switches: 2 / 2
- Fission for-loops / Ghidra for-loops: 15 / 11
- Fission do-while / Ghidra do-while: 7 / 9
- Failure classes (Fission): {'type': 1}
- Failure classes (Ghidra): {}
- Type preservation hits (Fission): {}
- Raw pointer / assembly fallbacks (Fission): {'raw_pointer_fallback': 13, 'assembly_fallback': 1}
- Cast chains (Fission/Ghidra): 0 / 7

## Residue Intel

### Single-Assign Temps
- `iVar3`: 12
- `uVar9`: 10
- `bVar16`: 6
- `xVar5`: 4
- `uVar2`: 4
- `uVar22`: 3
- `xVar8`: 2
- `iVar6`: 2
- `uVar3`: 2
- `iVar1`: 2
- `uVar10`: 2
- `uVar5`: 2
- `xVar17`: 1
- `xVar16`: 1
- `xVar4`: 1
- `iVar4`: 1

### Residue Names
- `uVar9`: 41
- `iVar3`: 31
- `xVar16`: 20
- `xVar17`: 20
- `bVar16`: 14
- `uVar22`: 13
- `uVar2`: 12
- `xVar5`: 11
- `iVar1`: 10
- `uVar5`: 10
- `uVar10`: 9
- `xVar18`: 8
- `iVar6`: 6
- `xVar8`: 6
- `uVar3`: 5
- `xVar4`: 3
- `iVar4`: 3

### Residue Families
- `uVar`: 90
- `xVar`: 68
- `iVar`: 50
- `bVar`: 14
- `raw_pointer_fallback`: 13
- `redundant_return_temp`: 2
- `uStack`: 0
- `xStack`: 0
- `axStack`: 0
- `assembly_fallback`: 0

### Top Offenders
- `cmkr` `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
- `test_structs_classes_x64_O0` `0x1400016e7` `main`: score=32, raw_pointer_fallback=0, single_assign_temps=9, top_names={'xVar5': 11, 'uVar2': 7, 'uVar3': 5}
- `cmkr` `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
- `test_control_flow_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
- `test_structs_classes_x64_O0` `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
- `cmkr` `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
- `cmkr` `0x1400034a0` `FUN_0x1400034a0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
- `cmkr` `0x1400036e0` `FUN_0x1400036e0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
- `test_structs_classes_x64_O0` `0x140001b80` `d_make_comp`: score=12, raw_pointer_fallback=0, single_assign_temps=2, top_names={'iVar1': 5, 'uVar2': 5}
- `test_structs_classes_x64_O0` `0x140001c80` `d_make_name`: score=6, raw_pointer_fallback=0, single_assign_temps=1, top_names={'iVar1': 5}

## Per-Binary Summary

### test_control_flow_x64_O0
- Shared success: 20 / 20 | Goto reduction: 0.00%
- Fission/Ghidra success: 20 / 20
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 0
- Top residue offenders:
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}

### test_structs_classes_x64_O0
- Shared success: 20 / 20 | Goto reduction: 58.33%
- Fission/Ghidra success: 20 / 20
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 0
- Top residue offenders:
  - `0x1400016e7` `main`: score=32, raw_pointer_fallback=0, single_assign_temps=9, top_names={'xVar5': 11, 'uVar2': 7, 'uVar3': 5}
  - `0x140001010` `__tmainCRTStartup`: score=18, raw_pointer_fallback=0, single_assign_temps=5, top_names={'bVar16': 7, 'iVar6': 3, 'xVar8': 3}
  - `0x140001b80` `d_make_comp`: score=12, raw_pointer_fallback=0, single_assign_temps=2, top_names={'iVar1': 5, 'uVar2': 5}
  - `0x140001c80` `d_make_name`: score=6, raw_pointer_fallback=0, single_assign_temps=1, top_names={'iVar1': 5}

### cmkr
- Shared success: 19 / 20 | Goto reduction: 0.00%
- Fission/Ghidra success: 19 / 20
- Struct pointer hits: Fission 0, Ghidra 0
- Type preservation: Fission {}, Ghidra {}
- Failure classes: Fission {'type': 1} | Ghidra {}
- Cast chains: Fission 0 | Ghidra 7
- Top residue offenders:
  - `0x140001000` `FUN_0x140001000`: score=92, raw_pointer_fallback=13, single_assign_temps=5, top_names={'xVar16': 20, 'xVar17': 20, 'uVar22': 13, 'xVar18': 8}
  - `0x140004010` `FUN_0x140004010`: score=22, raw_pointer_fallback=0, single_assign_temps=5, top_names={'uVar10': 9, 'iVar3': 5, 'xVar4': 3}
  - `0x140003270` `FUN_0x140003270`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
  - `0x1400034a0` `FUN_0x1400034a0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
  - `0x1400036e0` `FUN_0x1400036e0`: score=18, raw_pointer_fallback=0, single_assign_temps=4, top_names={'uVar9': 9, 'iVar3': 5}
