# Architecture Isolation Scan

- Repo: `/Users/sjkim1127/Fission`
- Findings: `152`

| Kind | Location | Token | Detail |
|---|---|---|---|
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/cleanup/deindirect.rs:269` | `x86` | Architecture name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/cleanup/temp_var.rs:760` | `rsp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/cleanup/temp_var.rs:760` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/cleanup/temp_var.rs:760` | `esp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/cleanup/temp_var.rs:760` | `ebp` | Raw register name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/cleanup/utils.rs:72` | `AArch64` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/cleanup/utils.rs:72` | `x86` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/global_opt/mem_ssa.rs:266` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/global_opt/mem_ssa.rs:268` | `arm` | Architecture name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `r12` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `r13` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `r14` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:44` | `r15` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r4` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r5` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r6` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r7` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r8` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r9` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:45` | `r10` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `r11` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `lr` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `ebx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `ebp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `esi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:46` | `edi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:55` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:247` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:926` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:930` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:940` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:968` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1001` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1011` | `lr` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1027` | `r11` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1050` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1078` | `r15` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1101` | `r15` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1199` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1205` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1234` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1235` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1277` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1307` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1346` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1350` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1351` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1361` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1366` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1368` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1383` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1388` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1390` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1413` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1420` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/idioms/prologue.rs:1432` | `rbx` | Raw register name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/memory/heritage.rs:459` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/memory/heritage.rs:473` | `arm` | Architecture name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `esp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `ebp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `rsp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `eax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `ecx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `edx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `ebx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `esi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/memory/slots.rs:398` | `edi` | Raw register name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/memory/typed_facts.rs:563` | `AArch64` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/pipeline/stages.rs:72` | `x86` | Architecture name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/pipeline/stages.rs:106` | `r15` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/pipeline/stages.rs:270` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/phi_recovery.rs:154` | `eax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:637` | `rsp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:637` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:637` | `esp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:637` | `ebp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:637` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `eax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `ebx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `ecx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `edx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `esi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `edi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `esp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `ebp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `rbx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `rcx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `rdx` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1179` | `rsi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `rsp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r8` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r9` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r10` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r11` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r12` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r13` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r14` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1180` | `r15` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `al` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `bl` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `cl` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `dl` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `ah` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `bh` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `ch` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1181` | `dh` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1212` | `rsp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1212` | `rbp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1212` | `esp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1212` | `ebp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1212` | `sp` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1583` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1584` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1595` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1601` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/recovery/variable_merge.rs:1605` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/entry_param_promotion.rs:202` | `r8` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/entry_param_promotion.rs:202` | `r9` | Raw register name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:905` | `x86` | Architecture name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1581` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1594` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1610` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1620` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1630` | `rax` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1925` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1933` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1936` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1982` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1990` | `rdi` | Raw register name in architecture-independent NIR code. |
| `register_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:1993` | `rdi` | Raw register name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/normalize/types/type_infer.rs:2065` | `x86` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/cfg_analysis/postdom.rs:248` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/loops.rs:516` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/loops.rs:784` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/loops.rs:1111` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/loops.rs:1133` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/structuring/loops.rs:1141` | `arm` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:311` | `AARCH64` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:312` | `AArch64` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:313` | `ARM` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:315` | `POWERPC` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:327` | `MIPS` | Architecture name in architecture-independent NIR code. |
| `architecture_name` | `crates/fission-pcode/src/nir/types/options.rs:333` | `X86` | Architecture name in architecture-independent NIR code. |
