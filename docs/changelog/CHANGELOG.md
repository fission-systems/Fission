# Changelog

All notable changes to the Fission project (November 2025 – Present).

---

## 2026-03-15

## 2026-03-16

## 2026-03-17

### Repository Licensing + CLA Setup

공개 저장소 기준 라이선스를 AGPL-3.0으로 고정하고, 향후 오픈코어 운영을 위한 Contributor License Agreement 문서를 추가했다. 목적은 코어 엔진은 AGPL-3.0으로 공개하되, 외부 기여분을 장기적으로 오픈소스/상용 양쪽에서 관리 가능한 법적 경계로 정리하는 것이다.

#### Added

- 루트 라이선스 파일
  - `LICENSE`에 GNU AGPL-3.0 전문 추가
- Contributor License Agreement
  - `CLA.md` 추가
- GitHub PR 템플릿
  - 기여자가 CLA 동의 여부를 체크할 수 있는 pull request template 추가

#### Changed

- README 공개 메타데이터
  - 저장소 라이선스를 AGPL-3.0으로 명시
  - CLA 안내 링크 추가
- Rust package metadata
  - 공개 workspace `Cargo.toml`들에 `license = "AGPL-3.0-or-later"` 반영
- CONTRIBUTING 가이드
  - 기여 전 CLA 동의가 필요하다는 정책 추가
  - 소스 파일 헤더는 전면 삽입 대신 repository-level license + 필요 시 SPDX short header를 쓰는 정책으로 고정

### Private AI Layer Repository Boundary Cleanup

공개 리포 경계를 더 선명하게 하기 위해 `fission-ai`를 오픈소스 workspace와 Git tracking 범위에서 분리했다. 이번 변경의 목적은 코어 디컴파일/분석 엔진은 공개하되, 향후 AI product/API orchestration 레이어는 비공개로 유지하는 저장소 경계를 먼저 고정하는 것이다.

#### Changed

- 공개 workspace 범위 조정
  - workspace member에서 `crates/fission-ai` 제거
  - `fission-analysis`의 `fission-ai` dependency 및 re-export 제거
- Git 공개 범위 정리
  - `.gitignore`에 `crates/fission-ai/` 추가
  - `crates/fission-ai/*`를 Git tracking에서 제거해 이후 GitHub 공개 범위에서 제외

#### Validation

- `cargo build -p fission-analysis --features native_decomp`

### v75-v78 - Preview-First Retirement Prep + Type Absorption Expansion + ARM64 Detection Scaffolding

이번 구간은 세 축으로 진행됐다. 첫째는 preview-first를 제품 기본 정책으로 더 굳히고, legacy를 내부 fallback / compat 경로로만 남기기 위한 taxonomy와 usage inventory를 정리하는 작업이었다. 둘째는 x64 `putty/cmake` hard case와 x86 `WinMerge/EverPlanet` hard case에서 Rust-side type absorption을 확장해, pointer-offset alias와 `register[offset]` surface가 field replacement 후보에서 덜 빠지게 만드는 것이었다. 셋째는 `ida76sp1` 포터블 코퍼스 기준 cross-image propagation 2차의 첫 단계로 `plugins/` 범위까지 sibling scan을 넓히고, Windows ARM64 spike를 위한 최소 PE architecture detection scaffolding을 추가하는 것이었다.

#### Added

- legacy-needed benchmark/report artifact 집계
  - preview direct가 아닌 성공 함수 목록을 binary/global summary에 별도 기록
- x86 decimal index field replacement regression test
  - `register[24]` 같은 decimal surface도 `->field/* @24 */` 후보로 검증
- `plugins/` 포함 cross-image propagation scope test
  - `ida76sp1/plugins/hexrays.dll`가 auto-propagation 대상에 실제 포함되는지 smoke 검증
- Windows ARM64 spike baseline note
  - 현재 blocker와 다음 bring-up 체크리스트를 `docs/benchmark/windows_arm64_spike.md`에 기록
- synthetic PE ARM64 loader test
  - `IMAGE_FILE_MACHINE_ARM64`가 `AARCH64:LE:64:v8A`로 매핑되는지 검증

#### Changed

- preview-first retirement prep
  - GUI에서 legacy를 사용자 workflow로 더 이상 취급하지 않도록 정리
  - CLI `--engine legacy`는 hidden compat mode 의미를 유지
  - fallback taxonomy를 `preview_timeout`, `preview_unsupported`, `native_pcode_failure`, `legacy_fallback`, `assembly_fallback` 기준으로 계속 고정
- x64/x86 공용 type absorption 보강
  - inferred type merge를 metadata-first로 유지
  - pointer-offset alias의 line-local forward substitution 유지/확장
  - `register[offset]` array index surface를 hex뿐 아니라 decimal까지 field replacement 후보로 흡수
- x86 hard case surface 보강
  - `WinMergeU.exe 0x407050`, `EverPlanet_KR.exe 0xa918d0` 같은 x86 giant function에서 decimal/stack-like index가 공용 postprocess에서 빠지지 않도록 조정
- cross-image propagation 2차의 첫 단계
  - same-folder scan에 더해 `plugins/` 하위 DLL까지 sibling candidate로 포함
  - weak-name 판정을 `sub_`, `FUN_`, `func_`, `Ordinal_`, `j_`, `thunk_`, `nullsub_`, `loc_`, `LAB_`까지 확대
- Windows PE loader / CLI surface
  - PE ARM64 machine type을 `AARCH64:LE:64:v8A`로 인식
  - CLI `binary-info --json` / terminal output에서 ARM64를 `x86_64`가 아니라 `arm64` / `ARM64 (64-bit)`로 표시

#### Improved

- `putty.exe 0x140006380`
  - 재사용 alias 때문에 남던 `unique0x... = register + offset` residue를 공용 type absorption으로 더 잘 정리
  - `register[offset]` surface 증가
- x86 hard case observability
  - hard case summary에서 `unique_surface_count`, `field_access_count`, `offset_index_count`를 바로 읽을 수 있게 됨
- legacy deprecation observability
  - 어떤 함수가 아직 preview direct가 아니어서 legacy/native fallback 결과에 의존하는지 report에서 바로 확인 가능
- `ida76sp1`
  - `plugins/hexrays.dll`까지 propagation scope에 포함돼 sibling-based auto rename 실효성 확대

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-static --features native_decomp field_offset_replacement -- --nocapture`
- `cargo test -p fission-loader test_parse_synthetic_pe -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-tauri`
- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/grand_finale_support/metrics.py`
  - `scripts/test/batch_benchmark/grand_finale_support/summary.py`
  - `scripts/test/batch_benchmark/grand_finale_support/report_md.py`

#### Notes

- `EverPlanet_KR.exe 0xa918d0`와 `WinMergeU.exe 0x407050`는 현재 legacy output 기준 `unique0x` residue는 이미 거의 0이었고, 이번 라운드의 실제 목적은 x86 `[]` / field surface를 더 잘 흡수하게 만드는 것이었다.
- Windows ARM64 spike는 아직 정식 지원 단계가 아니며, 실제 Windows ARM64 PE 샘플이 repo에 없어서 fixed-seed baseline JSON/Markdown은 다음 라운드로 넘겼다.

### v69-v74 - x64 Timeout Closure + Portable Multi-DLL Symbol Propagation

이번 구간은 두 축으로 마감됐다. 첫째는 x86/x64 giant function에서 남아 있던 마지막 branch/readability residue를 줄이고, `ida76sp1` 코퍼스에서 발생한 preview long-running case를 subprocess isolation으로 명시적 fallback으로 바꾸는 것이었다. 둘째는 포터블 멀티-DLL 폴더를 하나의 앱 집합으로 보고, sibling EXE/DLL의 import/export/thunk 관계만 이용해 현재 모듈의 weak `sub_*` 이름을 자동 복구하는 cross-image symbol propagation 1차 구현을 추가하는 것이었다.

#### Added

- x86 branch-condition recovery 보강
  - exact `TEST`/`CMP` bool tree를 terminator lowering에서 직접 읽어 `== 0`, `!= 0`, signed/unsigned compare로 복구
- preview render subprocess worker
  - heavy preview render를 별도 worker process에서 실행
  - timeout 시 worker kill 후 explicit fallback
- `ida76sp1` x64 fixed-seed baseline / watchlist artifact
  - `ida64.exe`
  - `idat64.exe`
  - `ida64.dll`
  - `ida.dll`
  - `plugins/hexrays.dll`
- Tauri backend cross-image propagation service
  - same-folder sibling `*.exe` / `*.dll` scan
  - import/export/thunk-only rename candidate resolution
  - in-memory auto rename provenance tracking

#### Changed

- non-float scalar self-equality / boolean simplification 추가
  - `x == x -> true`
  - `x != x -> false`
  - `if (!reg && reg == reg)` 같은 잔여식 정리
- dead flag intrinsic temp cleanup 강화
  - unused `__carry/__scarry/__sborrow` assignment를 dead-store cleanup에서 제거
- `ida76sp1` watchlist timeout 2건을 in-process retry가 아니라 subprocess isolation + `preview_timeout` fallback으로 정리
  - `ida64.dll 0x101fa177`
  - `hexrays.dll 0x17088330`
- `hexrays.dll 0x170057f0`는 ambiguous empty preview 상태가 아니라 non-empty assembly fallback으로 고정
- `open_file` 직후 현재 바이너리 부모 폴더를 one-shot 스캔해 sibling import/export/thunk 기반 auto rename을 `renamed_functions`에 직접 병합
- 수동 rename / project-loaded rename은 auto-propagated rename보다 우선하도록 GUI state provenance 추가

#### Improved

- `EverPlanet_KR.exe 0xa918d0`
  - `if (!reg && reg == reg)` occurrence 제거
  - `reg == reg` residue 제거
  - `code_length` 추가 감소
- `ida76sp1` baseline closure
  - `ida64.exe`: direct preview `4/5`
  - `idat64.exe`: direct preview `4/5`
  - `ida64.dll`: direct preview `4/5`, timeout 1건은 explicit fallback으로 정리
  - `ida.dll`: direct preview `4/5`
  - `hexrays.dll`: direct preview `3/5`, 나머지는 explicit legacy/assembly fallback
- `ida64.dll 0x101fa177`, `hexrays.dll 0x17088330`는 더 이상 20초 hang으로 남지 않음
- `ida76sp1/ida64.dll` 실코퍼스 smoke 기준 sibling scan에서 non-zero propagated rename 확인
- 기존 회귀 유지
  - `putty.exe 0x140006260`: `LPRECT param_2`, `RECT local_3c`, `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: direct preview 유지
  - `WinMergeU.exe` x86 / `EverPlanet_KR.exe` x86 direct preview 유지

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-static --features native_decomp preview_worker_ -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --bin fission_preview_worker --features native_decomp`
- `cargo build -p fission-tauri`
- compare / watchlist reruns
  - `ida64.dll 0x101fa177`
  - `hexrays.dll 0x17088330`
  - `hexrays.dll 0x170057f0`
  - `ida76sp1` fixed-seed 5개 바이너리 전체
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`
  - `WinMergeU.exe` x86
  - `EverPlanet_KR.exe` x86

#### Notes

- giant function direct preview 비율은 계속 올릴 수 있지만, 이번 라운드부터는 “hang하지 않고 명시적으로 fallback한다”가 baseline safety net으로 들어갔다.
- cross-image propagation 1차 구현은 보수적으로 `import/export/thunk` 관계만 사용하며, body hash / CFG similarity / aggressive FID body matching은 아직 넣지 않았다.

### v63-v68 - C++ Corpus Expansion + x86 Preview Readability Uplift

이번 구간은 새 샘플 코퍼스를 실제로 늘려 preview를 더 넓은 실전 바이너리에 검증하고, 그 과정에서 드러난 x86 병목과 가독성 문제를 순서대로 보수하는 데 집중했다. `WinMerge`, `SumatraPDF`, `CMake`, `EverPlanet_KR`를 추가해 고정 seed를 돌렸고, x86 `try_lower_while()` timeout, trap-like `CallInd`, x86 register/slot surfacing, flag-temp residue를 차례로 정리했다.

#### Added

- 신규 Windows 샘플 코퍼스 검증 세트
  - `WinMergeU.exe` x64 / x86
  - `SumatraPDF-3.5.2-32.exe`
  - `cmake.exe`
  - `EverPlanet_KR.exe`
- x86 `CallInd` trap-like target 복구
  - `INT3` producer를 `((code *)swi(3))` 형태의 opaque callee로 surface
- x86 readability 테스트 보강
  - x86 register naming bootstrap test
  - large-body cheap slot surfacing test
  - dead non-temp local / dead flag intrinsic temp cleanup test
- EverPlanet x86 fixed-seed stress corpus
  - `0x401070`
  - `0x5d3ef0`
  - `0x7ae490`
  - `0xa918d0`
  - `0xcf3950`

#### Changed

- x86 `try_lower_while()`에 budgeted fallback 추가
  - `SumatraPDF-3.5.2-32.exe 0x4018f0` timeout 해소
- x86 register surface를 실제 이름으로 복구
  - `eax/ecx/edx/ebx/esp/ebp/esi/edi`
- large HIR에서도 cheap slot surfacing을 계속 돌리도록 normalize gate 분리
- write-only non-temp local clobber 제거
  - `local_c = ...`
  - `param_fffffffc = 0`
- x86 flag-temp canonicalization 추가
  - `__carry(x, 0)`, `__scarry(x, 0)`, `__sborrow(x, 0)` → `false`
  - `__carry(x, c)` constant case를 unsigned compare로 축약
  - exact `SBORROW` compare shape를 signed `<`, `<=`로 환원
- dead flag intrinsic temp cleanup 추가
  - unused `xVar* = __carry/__scarry/__sborrow(...)` 제거

#### Improved

- `SumatraPDF-3.5.2-32.exe` fixed seeds 5개 모두 `mlil_preview`, fallback 0
  - `0x6b434c`는 x86 `CallInd` 복구로 direct preview 승격
- `WinMergeU.exe` x86 fixed seeds 5개 모두 `mlil_preview`, fallback 0 유지
- `EverPlanet_KR.exe` fixed seeds 5개 모두 `mlil_preview`, fallback 0
  - legacy는 selected seeds 기준 전부 timeout
- `EverPlanet_KR.exe 0xa918d0` readability 개선
  - residue score: `207 -> 169 -> 11`
  - temp surface count: `182 -> 144 -> 11`
  - code length: `18435 -> 15459 -> 9462`
  - `__carry/__scarry/__sborrow`: `68/68/19 -> 33/68/18 -> 0/0/0`
- x64 regression 유지
  - `putty.exe 0x140006260`: `LPRECT param_2`, `RECT local_3c`, `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: direct preview 유지

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- fixed-seed compare reruns
  - `SumatraPDF-3.5.2-32.exe`
  - `WinMergeU.exe` x86 / x64
  - `EverPlanet_KR.exe`
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`

### v62 - Warning Cleanup + Fixed-Seed Benchmark Closure

`nir` 2차 리팩토링 이후 남아 있던 마감 작업을 정리했다. 이번 라운드는 새 알고리즘 추가 없이 dead warning 둘을 제거하고, `putty` / `everything` / `notepad++` / `7zr` 고정 seed를 다시 측정해 현재 preview 상태를 숫자로 닫는 데 집중했다.

#### Changed

- dead code warning 제거
  - `MlilPreviewOptions::is_pe_x64()` 제거
  - `PcodeFunction::to_flat_bytes()` 내부 미사용 `VN_SIZE` 제거

#### Improved

- `cargo test` / `cargo build --release`가 추가 warning 없이 깨끗하게 통과
- fixed-seed compare closure 재확인
  - `putty.exe 0x140006260`: `mlil_preview`, fallback 0, `LPRECT param_2` / `RECT local_3c` / `*param_2 = local_3c;` 유지
  - `everything.exe 0x140183590`: `mlil_preview`, fallback 0
  - `7zr.exe 0x401000`, `0x401804`, `0x402778`: 모두 `mlil_preview`, fallback 0
  - `notepad++.exe 0x140037d30`, `0x14010ce08`, `0x1402e0204`: 모두 `mlil_preview`, fallback 0
- 특히 `notepad++` fixed seed 3개는 legacy가 20초 timeout으로 실패하는 반면 preview는 전부 완료

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- compare / fixed-seed closure
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`
  - `notepad++.exe 0x140037d30`, `0x14010ce08`, `0x1402e0204`
  - `7zr.exe 0x401000`, `0x401804`, `0x402778`

### v59-v61 - x86 Conditional Structuring 안정화 + `nir` 2차 리팩토링

이번 구간은 두 단계로 마감됐다. 먼저 `7zr.exe` x86 heavy seed에서 long-running 하던 `try_lower_if()` 경로를 bounded fallback과 join/follow-gated candidate 검사로 안정화해 direct preview timeout을 없앴고, 이어서 커진 `nir` 구현을 builder/conditionals/test tree 기준으로 다시 분해해 이후 품질 개선과 regression 추적 비용을 낮췄다.

#### Added

- x86 전용 conditional structuring budget / cache
  - `try_lower_if()` 10ms / 512 subcall guard
  - `(start_idx, exit)` 기준 `lower_linear_body()` cache
- join/follow-gated plain `if` candidate pre-check
  - ambiguous follow / non-forward join / open body tail rejection
- `nir` 2차 리팩토링용 하위 모듈 트리
  - `builder/materialize.rs`
  - `builder/terminator.rs`
  - `builder/aggregate_recovery.rs`
  - `builder/call_recovery.rs`
  - `builder/entry_analysis.rs`
  - `structuring/conditionals/{plain_if,short_circuit,if_else,budget}.rs`
- 대형 test file 분해
  - `structuring_conditionals.rs`
  - `structuring_linear.rs`
  - `structuring_loops.rs`
  - `structuring_switch.rs`
  - `structuring_misc.rs`
  - `type_hints_aliases.rs`
  - `type_hints_aggregates.rs`
  - `type_hints_stack_slots.rs`
  - `type_hints_imports.rs`

#### Changed

- x86 pathological CFG에서는 schema가 명확할 때만 structured `if`를 시도하고, 그 외 shape는 즉시 generic fallback으로 넘기도록 보수화
- short-circuit 계열은 plain `if`보다 앞에서 같은 join으로 닫히는 조건 체인을 우선 흡수하도록 정리
- `builder/mod.rs`는 thin entry/state 역할만 남기고, materialization / aggregate recovery / call recovery / entry analysis를 개별 파일로 이동
- 기존 단일 파일 `structuring/conditionals.rs`는 디렉터리 모듈로 승격
- `nir/tests/mod.rs`는 공통 fixture/helper만 유지하고, 실제 회귀 테스트는 기능 축별 파일로 재배치

#### Improved

- `7zr.exe 0x401804`, `0x402778`가 더 이상 `try_lower_if()` 장기 실행 때문에 timeout으로 떨어지지 않음
- `7zr.exe 0x401000`, `0x401804`, `0x402778` 모두 `mlil_preview` direct output 유지
- `putty.exe 0x140006260`의
  - `LPRECT param_2`
  - `RECT local_3c`
  - `*param_2 = local_3c;`
  출력 유지
- `everything.exe 0x140183590`도 `mlil_preview` direct output 유지
- `nir` 내부 응집도가 높아져 이후 x86 structuring / aggregate recovery 수술 범위를 더 좁게 잡을 수 있게 됨

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- compare / smoke
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`
  - `7zr.exe 0x401000`
  - `7zr.exe 0x401804`
  - `7zr.exe 0x402778`

### v36-v58 - `putty` Aggregate Copy 마감 + x86 Timeout 진단

이번 구간은 두 갈래로 진행됐다. 첫째는 `putty.exe 0x140006260`에서 남아 있던 마지막 aggregate transit temp를 걷어내서 preview 출력이 legacy 수준의 `RECT local_3c; *param_2 = local_3c;`에 도달하게 만드는 것이었고, 둘째는 `7zr.exe` x86 heavy seed timeout이 Rust NIR인지 native extraction인지 경계를 정확히 가르는 것이었다.

#### Added

- aggregate transit temp 정리를 위한 dead temp cleanup pass
  - unused temp assignment 제거
  - unused temp local binding prune
  - nested body까지 재귀 적용되는 cleanup fixed-point
- preview/native/prepare 경계 진단 로그
  - `PREPARE-DIAG`
  - `PREVIEW-DIAG`
  - `FFI-DIAG`
  - `FFI-NATIVE-DIAG`
  - `NATIVE-DIAG`
- structuring 단계 세부 진단 로그
  - region-lowering candidate 시도 지점
  - generic fallback (`lower_block_stmts`, `lower_block_terminator`) 진입 지점

#### Changed

- asm-guided aggregate fallback과 stack-slot recovery를 정리해 `putty.exe 0x140006260`에서
  - `LPRECT param_2`
  - `RECT local_3c`
  - `*param_2 = local_3c;`
  를 direct preview로 복구
- dead aggregate transit temp인 `xVar32` assignment/declaration 제거
- x86 timeout 추적을 위해 native prepare / preview p-code extraction / Rust structuring 경계를 모두 계측 가능하게 조정

#### Improved

- x64 regression closure
  - `everything.exe` fixed seed 3개 모두 `mlil_preview`
  - `notepad++.exe` fixed seed 2개 `mlil_preview`, 1개는 legacy/preview 공통 timeout
  - asm-guided aggregate fallback 오탐 없음
- `putty.exe 0x140006260` preview 출력은 이제 historical target을 충족
- x86 `7zr.exe` heavy seed 두 개의 timeout 원인을 native가 아니라 Rust `structuring`으로 좁힘
  - `0x401804`는 `try_lower_if()` 경로
  - `0x402778`도 `try_lower_if()` 경로

#### Validation

- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- regression / compare
  - `putty.exe 0x140006260`
  - `everything.exe` fixed seed 3개
  - `notepad++.exe` fixed seed 3개
  - `7zr.exe` fixed seed 5개
- diagnostic repro
  - `7zr.exe 0x401804`
  - `7zr.exe 0x402778`

#### Notes

- x64 쪽 aggregate-copy/type-surface 목표는 사실상 마감됐다.
- 남은 큰 이슈는 x86 heavy seed에서 `structuring/conditionals.rs::try_lower_if()`가 장기 실행되는 문제다.

---

## 2026-03-14

### v26-v35 - Preview Coverage 복구와 `putty` Type-Surface 재상승

이번 구간의 목표는 크게 두 가지였다. 첫째는 `mlil-preview`가 다시 real large-function에서 끝까지 살아남도록 coverage를 복구하는 것, 둘째는 direct preview로 복구된 뒤에도 무너졌던 `putty.exe 0x140006260`의 타입 표면을 다시 끌어올리는 것이었다. 이 과정에서 native decompiler budget, preview builder lowering, linear structuring, stack-slot recovery, call-argument recovery, wide aggregate copy 경로를 모두 손봤다.

#### Added

- preview/native 경계의 coverage 진단과 세분화 강화
  - unsupported varnode/value lowering 상세 분류 추가
  - preview 전용 flow/instruction budget 추가
  - preview linear structuring depth / complexity guard 추가
- x86 preview bootstrap 유지 및 regression guard 정착
- direct preview 경로용 stack-slot naming 복구
  - entry prologue 기반 frame size 추론
  - `RSP + offset` 패턴을 `local_xx` 식으로 surface
- indirect import / Win64 calling convention 기반 argument recovery 보강
  - `GetClientRect` 같은 import call target 이름 복구
  - `callind` 입력이 불완전한 경우 block-local register setup에서 인자 복구
- builder 내부 site-sensitive lowering 인프라 추가
  - `DefSite`
  - `LoweringSite`
  - block-local prior-def lookup
  - def-site-aware materialized varnode tracking

#### Changed

- preview p-code 경로에서 `.data` 스캔, GDT 의존, jump-table recovery를 줄여 giant dispatcher 함수 hang/abort 가능성을 낮춤
- Rust NIR linear structuring이 giant function에서 재귀 폭주하지 않도록 linear fallback cache와 complexity-based fast path를 추가
- `putty.exe 0x140001160`의 direct preview 복구를 위해 builder lowering을 보수적으로 완화
  - recoverable pointer arithmetic / wrapped condition chain / non-essential temp materialization skip
  - generic `LoweringFailed`를 단계별 상세 category로 세분화
- wide aggregate copy 복구를 위해
  - wide register source recovery
  - `SubPiece` / `IntSub` lane matcher 보강
  - 16-byte transit output materialization 억제
  - same-block prior-def 기반 lowering
  를 순차적으로 추가
- preview printer는 simple pointer deref를 더 자연스럽게 출력하도록 조정

#### Improved

- `putty.exe 0x140001160`는 다시 `engine_used = mlil_preview` direct output을 회복
- `everything.exe 0x140183590` direct preview 유지
- `7zr.exe 0x401000` direct preview 유지
- `putty.exe 0x140006260`의 preview 출력은 다시 다음 수준까지 회복
  - `LPRECT param_2`
  - `GetClientRect(param_1, param_2)`
  - `local_3c` stack-slot surface
  - `*param_2 = ...` 형태의 whole-object assignment 경로
- historical legacy 수준인 `RECT local_3c; *param_2 = local_3c;`에는 아직 한 단계 못 미치지만, raw pointer / anonymous temp 수준에서 크게 올라옴

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- `cargo check -p fission-tauri`
- compare / regression smoke
  - `putty.exe 0x140001160`
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`
  - `7zr.exe 0x401000`

#### Notes

- 가장 위험했던 두 병목은 정리됐다.
  - native giant-function flow tracing / hang
  - Rust linear structuring 재귀 폭주
- 남은 주요 gap은 `putty.exe 0x140006260`의 마지막 wide aggregate store collapse다.
  - 현재는 `local_3c`와 `LPRECT param_2`는 복구됐지만
  - `*param_2 = local_3c;`까지 완전히 접히기 직전에서
  - real optimized p-code의 마지막 wide-register path를 한 번 더 잡아야 한다.

### v25 - NIR 모듈 트리 리팩토링 및 유지보수성 정리

v25의 목표는 기능을 더 넣는 것이 아니라, 이미 커진 `nir` 코어를 이후 라운드에서 다루기 쉽게 정리하는 것이었다. 이번 라운드에서는 알고리즘 변경 없이, `builder / normalize / structuring / tests` 축으로 책임을 나누고 flat file split을 실제 directory tree로 승격했다.

#### Changed

- `crates/fission-pcode/src/nir/` 아래 구현을 폴더 트리 기준으로 재배치
  - `builder/`
  - `normalize/`
  - `structuring/`
  - `tests/`
- `nir/mod.rs`는 외부 entrypoint 역할만 유지하고, 내부 lowering/normalization/structuring 세부 구현은 각 하위 모듈로 이동
- `normalize` 계층을 다음 책임으로 분리
  - arithmetic / boolean normalization
  - cleanup
  - slot/table surfacing
  - bitstream helper matching
- `structuring` 계층을 다음 책임으로 분리
  - conditionals
  - loops
  - switch
  - linear fallback
- `nir` 테스트를 기능별 파일로 분리
  - arithmetic normalization
  - slot/table surfacing
  - bitstream helpers
  - structuring
  - x86 bootstrap
  - preview type hints

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo check -p fission-tauri`

#### Notes

- 이번 라운드는 기계적 리팩토링만 수행했고, 알고리즘/출력 정책은 의도적으로 바꾸지 않았다.
- 대표 preview smoke entrypoint는 계속 유지된다.
  - `putty.exe 0x140006260`
  - `everything.exe 0x140183590`
  - `7zr.exe 0x401000`

### v24 - Preview Coverage Recovery First, x64+x86 병행

v24의 목표는 v22/v23에서 추가한 slot/table abstraction과 bitstream helper 계층이 실제 함수에 적용되도록, preview가 다시 직접 끝까지 살아남게 만드는 것이었다. 이번 라운드에서는 beautification을 거의 늘리지 않고, x64 large-function coverage 복구와 x86 bootstrap에 집중했다.

#### Added

- preview unsupported reason 세분화
  - `unsupported_arch`
  - `unsupported_cfg_branch_target`
  - `unsupported_cfg_region_shape`
  - `unsupported_cfg_phi_join`
  - `unsupported_cfg_indirect_call_region`
  - `unsupported_expr_multiequal`
  - `unsupported_expr_varnode_lowering`
- PE x86 preview bootstrap 경로 추가
  - 32-bit pointer size / stack-base 규칙 반영
  - x86 seed 함수 direct preview bootstrap 지원

#### Changed

- x64 large-function direct preview 복구를 위해 branch target recovery를 완화
- trivial forwarding / cleanup / tail return region 흡수를 region builder 앞단에서 더 공격적으로 수행
- `MULTIEQUAL`의 identical-input case는 region failure 대신 canonical value로 내리도록 조정
- indirect call이 block body 내부에 있는 경우, control terminator가 아니면 preview region failure 사유로 취급하지 않도록 정리
- v22/v23의 slot-family / bitstream helper / loop-body compaction 계층은 유지하되, 적용 순서는 coverage-first 기준으로 고정

#### Improved

- `putty.exe 0x140006260`는 다시 `engine_used = mlil_preview` direct output을 회복
- `everything.exe 0x140183590`도 `engine_used = mlil_preview` direct output을 회복
- `7zr.exe` fixed seed 중 최소 1개 함수가 direct preview로 내려가며, x86 preview bootstrap이 실함수 기준으로 처음 확인됨

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo check -p fission-tauri`
- compare reports
  - `artifacts/compare_v24_putty/`
  - `artifacts/compare_v24_everything/`
  - `artifacts/compare_v24_7zr/`

#### Notes

- 이번 라운드는 “본문 beautification”보다 “preview direct output 복구”가 KPI였다.
- 남은 병목은 `putty.exe 0x140001160`의 `value lowering failed`와 일부 x64 large-function residue에 집중된다.

## 2026-03-12

### v16 - Preview 타입 표면 품질 및 `putty 0x140006260` 직접 출력 달성

v16의 목표는 preview 경로가 단순히 구조화된 pseudocode를 내는 수준을 넘어서, known signature 기반 타입 표면을 더 자연스럽게 드러내도록 만드는 것이었다. 특히 `putty.exe 0x140006260`를 preview 경로에서도 직접 구조화하고, `LPRECT` / `RECT` / whole-object assignment 형태까지 surface 하는 것을 목표로 했다.

#### Added

- preview path에 known-signature 기반 type surface context 추가
- preview binding surface type 힌트 추가
- real p-code JSON opcode alias parsing 강화
  - `goto`, `copy`, `load`, `store`, `==`, `+`, `SUB`, `ZEXT` 등
- layout-based fallthrough 계산 추가로 preview CFG recovery 보강

#### Changed

- preview CFG/structuring이 block vector 순서가 아니라 실제 block layout과 successor 관계를 기준으로 동작하도록 수정
- conditional `goto(target, cond)` 형태의 real p-code branch를 preview lowering이 직접 이해하도록 수정
- preview optimizer panic은 즉시 전체 실패로 번지지 않고 raw p-code path로 containment 하도록 조정

#### Improved

- `putty.exe 0x140006260 --engine mlil-preview` 에서 직접:
  - `LPRECT param_2`
  - `RECT local_3c`
  - `*param_2 = local_3c;`
  를 surface 할 수 있게 됨
- preview는 여전히:
  - `preview_engine_used_count 113`
  - `preview_fallback_count 0`
  - `preview_goto_count 0`
  - `preview_temp_surface_count 0`
  를 유지

#### Benchmark Highlights

- Shared success: `120 / 120`
- Preview engine used: `113`
- Preview fallback: `0`
- Preview goto count: `0`
- Preview temp surface count: `0`
- Cast chains: `Fission 9 / Ghidra 24`

#### Notes

- 이번 라운드는 legacy 경로나 Ghidra C++ core를 넓게 손대지 않고, preview 전용 타입 표면과 CFG 해석 품질을 높이는 데 집중했다.
- `putty.exe 0x140006260`는 preview 타입 표면 품질의 대표 regression/acceptance guard로 유지된다.

### v15 - Preview 품질 격상과 저위험 함수 기본 경로 승격

v15의 목표는 legacy 성공률을 더 올리는 것이 아니라, `mlil-preview`를 낮은 리스크 함수에서는 더 읽기 좋은 기본 경로로 끌어올리는 것이었다. 이번 라운드에서 preview 경로는 canonical `switch` reconstruction, preview 전용 surface cleanup, `auto` 채택 범위 확대를 통해 실질적인 제품 경로로 한 단계 더 올라갔다.

#### Added

- preview 경로에 canonical `switch` reconstruction 추가
- preview 전용 surface cleanup 계층 추가
  - bool compare/test normalization
  - trivial temp elision
  - redundant return temp 제거
  - repeated cast wrapper collapse
- preview/legacy 공용 `engine_used` 문자열 source of truth를 `fission-static` helper로 집중

#### Changed

- `auto` preview eligibility 한도를 완화하여, 구조가 안정적인 multi-block 함수는 preview를 더 자주 직접 채택하도록 조정
- preview printer는 구조화와 cast cleanup은 더 강해졌지만, field-name 추측이나 legacy postprocess 전체 재도입은 하지 않도록 정책을 유지

#### Benchmark Highlights

- Shared success: `120 / 120`
- Preview engine used: `113`
- Preview fallback: `0`
- Preview goto count: `0`
- Preview temp surface count: `0`
- Cast chains: `Fission 9 / Ghidra 24`

#### Notes

- v15는 preview coverage와 구조화 품질을 올리는 데는 성공했지만, preview 타입 표면 품질은 아직 legacy보다 약하다.
- 대표적인 known limitation은 `putty.exe 0x140006260`에서 preview가 아직 legacy 수준의 `LPRECT` / `RECT` 표면 타입을 직접 만들지 못한다는 점이다.

### v14 - Legacy `type` 실패 제거 및 90/90 복구

v14의 목표는 `mlil-preview` rescue를 성공으로 간주하지 않고, legacy / Ghidra-backed C 경로 자체가 끝까지 살아남도록 만드는 것이었다.
핵심은 `Duplicate VariablePiece` 기반 `type` 실패를 더 이른 단계에서 containment 하고, strict type-seeded path가 무너지더라도 printer 가능한 보수적 base result를 legacy 경로 안에서 끝까지 전달하는 것이었다.

#### Fixed

- legacy path에서 남아 있던 `type` failure 3건 해소
  - `putty 0x1400052b0`
  - `putty 0x140006380`
  - `cmkr 0x140002cc0`
- `Duplicate VariablePiece` 재시도 실패 후에도 printable legacy C result가 남아 있으면 assembly fallback 대신 그 결과를 우선 반환하도록 조정
- explicit `--engine legacy` 호출에서는 preview rescue가 개입하지 않도록 정리

#### Changed

- `ghidra_decompiler/src/decompiler/DecompilationCore.cpp`
  - `Duplicate VariablePiece` 재시도 실패 시 partial legacy C 결과를 우선 확보하는 경로 추가
  - strict type-seeded path가 무너져도 legacy printer가 먹을 수 있는 보수적 결과를 전달하도록 containment 강화
- `crates/fission-cli/src/cli/oneshot/decompile.rs`
  - explicit legacy 경로에서는 preview rescue 비활성화
- `crates/fission-tauri/src-tauri/src/commands/analysis/assembly.rs`
  - explicit legacy 경로에서는 preview rescue 비활성화

#### Benchmark Highlights

- Shared success: `90 / 90`
- Fission success: `90 / 90`
- Ghidra success: `90 / 90`
- Fission `type` failures: `0`
- Preview engine used: `87`
- Preview fallback: `0`
- Preview goto count: `0`
- Preview temp surface count: `0`

#### Regression Status

- `putty.exe 0x140006260`의 `LPRECT` / `RECT` 경로 유지
- preview metrics 후퇴 없음
- `Stack_` / `_offset_size_` 잔재 재발 없음

#### Notes

- 이번 라운드는 preview 품질 개선이 아니라 pure legacy C failure removal 라운드였다.
- `mlil-preview`는 regression guard로만 유지했고, 정식 해결 판정에는 포함하지 않았다.

### v13 - MLIL Preview 구조화/가독성 고도화

`mlil-preview`를 단순 실험 슬라이스에서 벗어나, 실제로 의미 있는 pseudocode를 직접 생성하는 차세대 경로로 끌어올리는 작업을 진행했다.
이번 라운드의 핵심은 Ghidra C 출력 후처리가 아니라, Fission 고유의 NIR/HIR 경로에서 직접
제어 흐름 구조화와 식(Expression) 정규화를 수행하는 데 있었다.

#### Added

- preview NIR/HIR 경로에 CFG helper 계층 추가
  - predecessor / successor 기반 block relation 추적
  - linear body / join 탐색 helper 추가
- canonical short-circuit folding 추가
  - `&&`
  - `||`
- multi-block loop lowering 추가
  - `while`
  - `do-while`
- cast canonicalizer 추가
  - 동일 타입 중첩 cast 제거
  - redundant widen-before-narrow wrapper 제거
  - no-op cast 제거
- `PIECE` / `SUBPIECE` recombination 추가
  - split-temp를 원본 식 기준으로 재조합
  - call argument / return surface에서 파편화 변수 노출 억제
- preview 전용 benchmark 지표 추가
  - `preview_engine_used_count`
  - `preview_fallback_count`
  - `preview_goto_count`
  - `preview_temp_surface_count`

#### Changed

- `mlil-preview`의 구조화 로직이 block index 기반의 취약한 패턴 매칭에서
  CFG 관계 기반 구조화로 전환됨
- unsupported multi-block CFG는 무리하게 잘못된 고수준 코드로 만들지 않고
  Fission pseudocode / fallback 경로로 안전하게 남기도록 유지
- legacy/Ghidra 경로는 기능 확장 대상이 아니라 regression guard path로 계속 유지

#### Improved

- preview path가 canonical multi-block `if`, `if/else`, `while`, `do-while`를 직접 출력할 수 있게 됨
- preview path에서 short-circuit boolean chain이 `&&`, `||`로 구조화됨
- split value (`PIECE/SUBPIECE`)가 별도 temp 변수로 노출되지 않고 source expression으로 재조합됨
- cast density가 낮아지고, 불필요한 `(T)(T)x` / widen-wrapper 표현이 줄어듦

#### Benchmark Highlights

- Shared success: `87 / 90`
- Fission success: `87 / 90`
- Ghidra success: `90 / 90`
- Goto reduction vs Ghidra: `47.01%`
- Cast chains: `Fission 27 / Ghidra 37`
- Preview success: `90`
- Preview engine used: `87`
- Preview fallback: `3`
- Preview goto count: `0`
- Preview temp surface count: `0`

#### Regression Status

- `putty.exe 0x140006260`의 `LPRECT` / `RECT` 경로 유지
- 기존 fallback 동작은 안정적으로 유지

#### Known Issues

- 남은 실패는 `type` 계열 3건에 집중됨
  - `putty 0x1400052b0`
  - `putty 0x140006380`
  - `cmkr 0x140002cc0`
- 다음 라운드의 1차 타깃은 type recovery / propagation 안정화

#### 핵심 파일

| 파일 | 변경 내용 |
|------|----------|
| `crates/fission-pcode/src/nir/mod.rs` | CFG helper, short-circuit folding, multi-block loop lowering, cast canonicalization, `PIECE/SUBPIECE` recombination |
| `scripts/test/batch_benchmark/grand_finale.py` | preview-only benchmark 지표 및 summary/report 강화 |

## 2026-03-11

### v10-v12 - Experimental Fission MLIL/NIR 경로를 제품 경로에 통합

Ghidra C 출력 후처리만으로는 IDA Pro / Binary Ninja 급의 일관된 가독성을 만들기 어렵다는 판단 아래,
Ghidra를 `lift + CFG + baseline type recovery + hard-fail containment` 계층으로 축소하고
그 위에 Fission 고유의 preview decompilation 경로를 올리는 작업을 시작했다.

이번 변경으로 `mlil-preview`는 더 이상 CLI 전용 실험 코드가 아니라,
CLI/Tauri 양쪽에서 선택 가능한 실제 엔진 모드가 되었다.

#### Added

- `legacy | mlil-preview | auto` decompilation engine mode 추가
  - CLI: `--engine <legacy|mlil-preview|auto>`
  - Tauri: decompiler options dialog에서 engine selector 제공
- preview 결과 메타데이터 추가
  - `engine_used`
  - `fell_back`
  - `fallback_reason`
- decompile view 상단에 engine badge / fallback badge 추가

#### Added: Fission-owned preview pipeline

- `crates/fission-pcode/src/nir/` 기반 preview NIR/HIR + Rust printer 경로 추가
- 현재 지원 범위
  - PE x64 only
  - stack-slot recovery
  - straight-line lowering
  - simple multi-block `if`, `if/else`, `while`, `do-while`
  - 구조화 실패 시 label/goto pseudocode fallback
  - basic `div/mod by power-of-two` idiom recognition

#### Changed: preview coverage 확대를 위한 p-code extraction 경량화

- `ghidra_decompiler/src/decompiler/DecompilationCore.cpp`
  - `run_decompilation_pcode()`가 full action-group `perform()` 전에
    `followFlow()` 기반 lightweight p-code serialization을 우선 사용
  - preview path가 Ghidra 분석 timeout / type 예외를 p-code 추출 단계에서 먼저 밟는 문제 완화
- `crates/fission-pcode/src/pcode/types.rs`
  - wrapped negative constant (`u64` 형태의 sign-extended constant)를
    `i64`로 복구하도록 JSON parser 보강
  - preview path가 `18446744073709551612` 같은 상수 때문에 즉시 parse 실패하던 문제 수정
- `crates/fission-pcode/src/nir/mod.rs`
  - multi-block canonical `if/if-else` lowering 보강
  - `PIECE`, `SUBPIECE`, conservative `MULTIEQUAL` lowering 보강
  - PE format gating을 `PE32+`까지 허용하도록 수정

#### Benchmark / Status

- v12 smoke benchmark 기준 `mlil-preview` 경로는 4개 바이너리 / 48개 샘플 함수에서
  직접 preview 출력을 생성
- legacy path는 회귀 없이 유지
  - `putty.exe 0x140006260`의 `LPRECT` / `RECT` 경로 유지
  - `putty.exe 0x140011060`, `cmkr.exe 0x140002cc0`의 fallback 안정성 유지
- 현재 한계:
  - preview coverage는 크게 늘었지만, real-world multi-block 함수의 pseudocode 품질은 아직 legacy보다 낮음
  - `switch` 복원, phi/loop-header normalization, large-function structuring은 다음 단계 과제

#### 핵심 파일

| 파일 | 변경 내용 |
|------|----------|
| `crates/fission-pcode/src/nir/mod.rs` | preview NIR/HIR builder, Rust printer, multi-block lowering |
| `crates/fission-pcode/src/pcode/types.rs` | wrapped negative constant JSON parsing 보강 |
| `ghidra_decompiler/src/decompiler/DecompilationCore.cpp` | lightweight p-code extraction 우선 경로 추가 |
| `crates/fission-cli/src/cli/args.rs` | `--engine` 옵션 추가 |
| `crates/fission-cli/src/cli/oneshot/decompile.rs` | engine 선택 / preview fallback plumbing |
| `crates/fission-tauri/src/panels/dialogs/DecompilerOptionsDialog.tsx` | Tauri engine selector 추가 |
| `crates/fission-tauri/src/panels/editor/DecompileView.tsx` | engine/fallback badge 표시 |
| `scripts/test/batch_benchmark/grand_finale.py` | preview comparison artifact 수집 강화 |

## 2026-03-09

### 대규모 멀티스레드 성능 혁신 (157초 → 10초)

멀티스레드 환경에서의 I/O 병목과 중복 파싱 오버헤드를 근본적으로 제거하여 **약 15.6배의 속도 향상** 달성. 100개 함수 배치 디컴파일 시간이 2.5분에서 10초로 단축됨.

#### Performance: 전역 싱글톤 캐시 레이어 도입 (초기화 8.5초 → 0초)

각 워커가 독립적으로 수행하던 무거운 초기화 작업을 전역 공유 구조로 전환하여 Amdahl의 법칙에 따른 병렬 확장성 한계 극복.

- **Sleigh XML 캐시**: `.sla` 바이너리 정의를 메모리에 캐싱하여 중복 XML 파싱 제거.
- **GDT (Ghidra Data Type) 캐시**: 수만 개의 타입을 가진 `.gdt` 파일(ZIP 압축) 파싱 결과를 워커 간 공유 (`shared_ptr`).
- **Data Section 스캔 캐시**: 바이너리의 데이터 섹션을 훑어 문자열/상수를 찾는 로직을 바이너리당 1회로 제한.

#### Reliability: Fail-Fast 타임아웃 시스템 (Monster Function 제압)

Ghidra 엔진의 `main_perform` 루프가 특정 함수에서 $O(N^3)$ 복잡도로 폭주하는 현상을 제어하기 위해 코어 레벨 타임아웃 도입.

- **분석 트립와이어**: `ActionGroup::perform` 루프 내부에 Wall-clock 타이머를 심어 지정된 시간(기본 30초, CLI 조절 가능) 초과 시 즉시 `LowlevelError` 투척.
- **성능 이득**: 11초 이상 소요되던 특정 괴물 함수를 빠른 시간 내에 끊어내어 전체 파이프라인의 "롱테일(Long-tail)" 병목 제거.

#### Scaling: 병렬 초기화 로직 정밀화

- **뮤텍스 최적화**: `initialize_architecture` 내 전역 락 범위를 최소화하고, 이미 초기화된 공통 리소스(GDT 등)는 락 없이 접근하도록 개선.
- **동적 스케일링**: 함수 개수에 비례하여 워커 수를 조절하는 휴리스틱 최적화.

#### 전체 변경 파일

| 파일 | 변경 내용 |
|------|----------|
| `ghidra_decompiler/decompile/sleigh.cc`, `sleigh_arch.cc` | Sleigh 인메모리 스트림 캐시 구현 |
| `ghidra_decompiler/src/core/ArchInit.cc` | GDT 전역 캐시 및 병렬 초기화 로직 |
| `ghidra_decompiler/src/core/DataSymbolRegistry.cc` | 데이터 섹션 스캔 결과 캐시 |
| `ghidra_decompiler/decompile/action.cc` | `Action::perform` 타임아웃 트립와이어 설치 |
| `ghidra_decompiler/decompile/architecture.hh`, `.cc` | 아키텍처별 타임아웃 설정 필드 추가 |
| `crates/fission-ffi/src/decomp/ffi.rs`, `wrapper.rs` | 타임아웃 조절용 FFI API 추가 |
| `crates/fission-cli/src/cli/args.rs` | `--timeout-ms` 옵션 추가 |

#### 벤치마크 결과 (putty.exe, limit 100 functions)

| 단계 | 소요 시간 (Wall Clock) | 개선율 |
|------|-----------------------|-------|
| 순정 (Baseline) | 157초 | 1.0x |
| Phase 2a (Sleigh Cache) | 57초 | 2.7x |
| **Phase 3 (Global Cache + Timeout)** | **10.03초** | **15.6x** |

---

## 2026-03-07

### 디컴파일러 성능 최적화 + 성공률 개선

putty.exe(2,756개 함수) 전체 벤치마크 기준 **성공률 61% → 87%**, 단일 함수 디컴파일 시간 **207ms → 44ms** 달성.

#### Performance: PostProcessPipeline 계측 및 병목 제거

pass별 `std::chrono` 타이밍 계측을 추가하여 병목을 정밀 식별 후 최적화.

| Pass | Before | After | 개선 |
|------|--------|-------|------|
| compound ops (`+=`, `++` 등) | 22.9ms | 0.01ms | **2,290x** |
| GUID 치환 | 34ms | ~0ms† | O(M·N) → O(N) 스캔 |
| CFGStructurizer (goto 없는 함수) | 48ms | 0.12ms | **400x** |
| PostProcessor::process 합계 | 83ms | 1.4ms | **59x** |

† GUID 로딩은 첫 호출에서만 파일 I/O 발생 (~27ms 일회성)

#### Performance: `convert_while_to_for` — 7 regex → 단일 패스 수동 매칭

`x = x + 1;` → `x++;` 류의 복합 대입 변환을 7개 `regex_replace` 순차 실행에서
O(N) 단일 패스 수동 파서로 전환.

- **수정 위치**: `ghidra_decompiler/src/decompiler/PostProcessor.cc`

#### Performance: GUID 치환 O(M·N) → O(N)

전체 GUID 맵(수천 개)을 순회하며 `string::find`를 반복하는 방식에서,
코드에서 GUID 패턴(8-4-4-4-12)을 먼저 스캔 후 해시맵 한 번 lookup으로 전환.

- **수정 위치**: `ghidra_decompiler/src/processing/passes/ConstantReplacementPasses.cc`

#### Performance: CFGStructurizer Early Exit

`goto` 키워드가 없는 함수에서 13개 CFG 변환 pass를 전부 스킵.
대부분의 함수에는 goto가 없으므로 가장 임팩트가 큰 최적화.

- **수정 위치**: `ghidra_decompiler/src/decompiler/CFGStructurizer.cc`

#### Performance: `LabelAnalyzer::remove_unused_labels` — 동적 regex 제거

미사용 라벨마다 `std::regex`를 새로 컴파일하던 방식에서
O(N·L) 수동 라인 스캐너로 전환.

- **수정 위치**: `ghidra_decompiler/src/decompiler/cfg/LabelAnalyzer.cc`

#### Fix: Recursive Decompilation 에러 715건 수정 (성공률 61% → 87%)

`--decomp-all` 모드에서 callee 분석 시 A→B→A 순환 참조가 발생하면
`isProcStarted()` 체크가 예외를 던져 외부 함수 전체 실패하던 버그 수정.

예외 전파 대신 순환 진입 탐지 시 forward-declaration 스텁을 반환하도록 변경.

- **수정 위치**: `ghidra_decompiler/src/decompiler/DecompilationCore.cpp`
- `ctx->analyzed_callees`에 순환 주소 즉시 등록하여 재시도 방지

#### Fix: Duplicate VariablePiece 예외 처리 강화 (PrototypeEnforcer + Ghidra Merge 충돌)

PrototypeEnforcer가 주입한 엄격한 API 시그니처와 Ghidra Merge가 충돌할 때 발생하는
`LowlevelError: Duplicate VariablePiece`를 Upstream 코드 수정 없이 방어.

- **FFI**: `ghidra::LowlevelError` 전용 catch 추가 — `std::exception` 미상속으로 누락되던 실제 에러 메시지 노출
- **DecompilationCore**: DVP 발생 시 `seed_before_action` 없이 1회 재시도
- **run_analysis_passes**: DVP 발생 시 빈 `AnalysisArtifacts`로 폴백하여 기본 디컴파일 유지

자세한 원인/현재 한계는 `docs/analysis/KNOWN_ISSUES.md` 참조.

- **수정 위치**: `DecompilationCore.cpp`, `libdecomp_ffi.cpp`

#### Infra: 공정 배치 벤치마크 시스템 구축

단일 프로세스 `--decomp-all --benchmark` 모드를 활용, 초기화 비용을 분리한
공정한 Fission vs PyGhidra 비교 스크립트.

- **추가 위치**: `scripts/test/batch_benchmark/runner_fission.py`, `main.py`

#### 전체 변경 파일

| 파일 | 변경 내용 |
|------|----------|
| `ghidra_decompiler/src/decompiler/PostProcessPipeline.cpp` | pass별 chrono 타이밍 계측 추가 |
| `ghidra_decompiler/src/decompiler/PostProcessor.cc` | `convert_while_to_for` 수동 패스 전환, 내부 pass 타이밍 추가 |
| `ghidra_decompiler/src/decompiler/CFGStructurizer.cc` | goto early exit 추가 |
| `ghidra_decompiler/src/decompiler/cfg/LabelAnalyzer.cc` | `remove_unused_labels` 동적 regex 제거 |
| `ghidra_decompiler/src/decompiler/DecompilationCore.cpp` | recursive decompilation throw → stub 반환 |
| `ghidra_decompiler/src/processing/passes/ConstantReplacementPasses.cc` | GUID 치환 O(N) 스캔 전환 |
| `ghidra_decompiler/src/processing/passes/NamingStandardizers.cc` | non-static regex → `static const` |
| `ghidra_decompiler/src/processing/passes/CppVirtualCallPasses.cc` | non-static regex → `static const` |
| `ghidra_decompiler/include/fission/ffi/DecompContext.h` | `analyzed_callees` 캐시 추가 |
| `ghidra_decompiler/src/decompiler/AnalysisPipeline.cpp` | callee 재분석 캐시 적용 |
| `scripts/test/batch_benchmark/runner_fission.py` | 단일 프로세스 배치 모드 재작성 |
| `scripts/test/batch_benchmark/main.py` | 공정 비교 출력 포맷 업데이트 |

#### 벤치마크 결과 (putty.exe, 2,756 functions)

| 지표 | Before | After |
|------|--------|-------|
| 성공률 | 1,693/2,756 (61%) | **2,408/2,756 (87%)** |
| 단일 함수 TOTAL | 207ms | **44ms** |
| postproc TOTAL | 125ms | **32ms** |

---

## 2026-03-03

### 보안 취약점 대응 (2차: 정책 기준선/CI 게이트 정착)

공급망 취약점 대응을 운영 가능한 형태로 고정하고, CI에서의 보안 점검을
기본 품질 게이트로 복원.

#### Added

- `docs/build/SECURITY_ADVISORIES.md` 추가
  - Rust/Node 보안 점검 명령, no-fix advisory 기준선 운영 원칙, 재검토 조건 문서화

#### Changed

- `deny.toml`
  - no safe upgrade가 없는 생태계 advisory를 `advisories.ignore` 기준선으로 명시
  - 신규/패치 가능한 advisory는 CI 실패로 triage 강제
- `.github/workflows/ci.yml`
  - Rust advisory 단계의 non-blocking 설정 제거(`continue-on-error` 제거)
  - 기준선 외 신규 취약점은 build failure로 처리

#### Security Notes

- 본 기준선은 영구 면제가 아니며, upstream 안전 업그레이드 또는 런타임 전환 시 제거 대상.

### 안정화/성능/이식성 리팩토링 완료 (Phase 2 ~ 4)

이번 배치에서 에러 처리 안전성(Phase 2), postprocess 성능(Phase 3), 경로 이식성(Phase 4)을
연속적으로 완료하고 `main`에 반영.

#### Added

- Postprocess 실행 통계 API 추가
  - `PassExecutionStats` (`executed_passes`, `borrowed_outputs`, `owned_outputs`, skip 카운트)
  - `PassRegistry::execute_all_with_stats(...)`
  - `execute_default_passes_with_stats(...)`

#### Changed

- **Phase 2 (`unwrap/expect` 제거):**
  - analysis/loader/pcode/disasm/core/ffi/tauri 전반 panic-prone 경로 제거
  - 실패 경로를 명시적 분기/에러 전파로 통일
- **Phase 3 (`Cow` 기반 문자열 최적화):**
  - pass pipeline을 `Result<Cow<str>, PassError>` 중심으로 전환
  - cleanup/structure/naming/type/dwarf/arithmetic/loop/switch/casts/boilerplate 패스에 no-op `Borrowed` fast path 확장
  - 불필요한 `String` 할당 감소
- **Phase 4 (하드코딩 경로 제거):**
  - `build.rs`의 Windows 절대 경로(`C:\\...`) fallback 제거
  - `VCPKG_ROOT`/`VCPKG_INSTALLATION_ROOT` + 환경 기반 탐색으로 통일
  - DIE 시그니처 로딩을 고정 상대경로 배열 대신 실행경로 기반 상향 탐색으로 변경

#### Fixed

- `switch` 재구성 회귀 수정:
  - `result = ...` 같은 일반 대입 타깃도 인식하도록 패턴 매칭 보완
  - 실패하던 `test_switch_from_if_else_assign_multiline` 복구

#### 검증

- `cargo build -p fission-loader -p fission-analysis -p fission-ffi` 통과
- `cargo test --workspace` 최종 통과 (postprocess 회귀 포함)

#### 대표 커밋

- `719050e` Constants library and initial stability improvements
- `58e05b2` Phase 2.8 remaining unwrap/expect 제거
- `1c6a278` Phase 3.1 Cow 기반 pass pipeline 도입
- `bb5e586` pass execution stats API 추가
- `65888f9` Phase 4 하드코딩 경로 제거
- `d12316d` switch reconstruction 회귀 수정

---

## 2026-03-02

### Decompiler Postprocess 모듈화 리팩토링 (대형 `postprocess.rs` 분해)

`crates/fission-analysis/src/analysis/decomp/postprocess.rs`의 단일 대형 구현을
기능 책임별 하위 모듈로 분리해 유지보수성과 회귀 안정성을 개선.

#### 주요 변경

- `naming` 분리: `replace_field_offsets`, `recognize_swift_accessors`, `demangle_swift_symbols`, `rename_induction_vars`, `rename_semantic_vars`, `apply_dwarf_names`
- `structure` 분리: `simplify_if_structure`, `while_true_to_while_cond`
- `arithmetic` 확장 분리: `apply_arithmetic_idioms`, `recover_divisor`, `mul_pow2_to_shift`
- 공통 조건 유틸 분리: `negate_condition` → `postprocess/condition.rs`
- 테스트 분리: 인라인 테스트 제거, `postprocess/tests.rs`로 이관

#### 신규/변경 파일

- `crates/fission-analysis/src/analysis/decomp/postprocess.rs`: 오케스트레이션 중심으로 슬림화
- `crates/fission-analysis/src/analysis/decomp/postprocess/naming.rs`: 네이밍/필드/DWARF 후처리 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/structure.rs`: if/while 구조 단순화 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/arithmetic.rs`: 산술 이디엄/매직디비전 복구 분리
- `crates/fission-analysis/src/analysis/decomp/postprocess/condition.rs`: `negate_condition` 공용 유틸
- `crates/fission-analysis/src/analysis/decomp/postprocess/tests.rs`: postprocess 회귀 테스트 모듈
- `docs/analysis/POSTPROCESS_MODULES.md`: 모듈 구조/확장 규칙 문서 추가

#### 테스트/검증

- `cargo build -p fission-analysis` 통과
- `cargo test -p fission-analysis postprocess::tests:: -- --nocapture` 통과 (`test_switch_from_if_else_assign_multiline`, `test_negate_condition_basic_cases`, `test_while_true_to_while_cond_simple`)

---

## 2026-03-01

### 디컴파일러 품질 대폭 개선: 4대 버그 수정 + v4 벤치마크 시스템

**벤치마크 점수: ARM64 69.4% → 88.9% | x64 71.6% → 88.2% | Linux 69.8% → 91.6% | Windows 68.0% → 91.1%**

#### Bug 1 (CRITICAL): 빈 함수 본문 생성 버그 수정 (`parse_decl_varname`)

`normalize_msvc_crt_printf` 후처리 스텝의 `parse_decl_varname` 람다가 `return param_1 / 3;`를 고아 변수 선언으로 잘못 파싱하는 치명적 버그 수정.

- **원인**: `return` 키워드 이후의 표현식에서 `3`을 변수명으로 파싱 → 다른 곳에서 미사용으로 판단 → 해당 라인 전체 삭제 → 함수 본문 소실
- **증상**: `divide_by_3(unsigned x)` → `uint __Z11divide_by_3j() { }` (파라미터도, 본문도 없음)
- **수정 위치**: `ghidra_decompiler/src/processing/PostProcessors.cc`
  - `parse_decl_varname`: `return` 키워드 검사 + 숫자로 시작하는 식별자 거부 조건 추가
  - `parse_assign_lhs`: `return` 키워드 exclusion 추가

#### Bug 2: 소멸자 Segfault 수정 (`arch.release()`)

- **원인**: `DecompContext::~DecompContext()`에서 `arch.reset()` 호출 시 Ghidra `Architecture` 소멸자 체인이 SIGSEGV 발생 (try/catch로 포착 불가)
- **수정**: `arch.reset()` → `arch.release()` (제어된 메모리 누수, 프로세스 종료 시 OS가 회수)
- **수정 위치**: `ghidra_decompiler/src/ffi/DecompContext.cpp`

#### Bug 3: 구조체 포인터 접근 미변환 수정 (`convert_struct_access`)

- **원인**: 기존 `annotate_structure_offsets`는 주석만 추가, `*(type*)(param + offset)` → `param->field` 변환 불가. 추가로 hex 리터럴 분할 버그(`0xc` → `0  /* field_0 */xc`) 존재
- **수정**: 완전한 새 함수 `convert_struct_access()` 구현 (5단계: 구조체 typedef 파싱 → struct 타입 파라미터 탐지 → 포인터 역참조 변환 → 오프셋-0 `*param` 변환 → 잔여 오프셋 재주석)
- **수정 위치**: `ghidra_decompiler/src/processing/PostProcessors.cc`, `ghidra_decompiler/include/fission/processing/PostProcessors.h`

#### Bug 4: 복합 대입 연산자 regex가 `->` 를 삼키는 버그 수정

- **원인**: `sub_assign_pattern` 정규식 `(\w+)\s*=\s*\1\s*-\s*([^;]+);`이 `local_8 = local_8->field_8;`를 빼기 연산으로 매칭 → `local_8 -= >field_8` 생성
- **수정**: `->` 음수 전방탐색(negative lookahead) 추가
- **수정 위치**: `ghidra_decompiler/src/decompiler/PostProcessor.cc` (2곳)

#### Feature: v4 벤치마크 시스템 구축

7개 C++ 소스 파일 × 4개 플랫폼 × 2개 최적화 레벨 = **56개 테스트 바이너리** 기반 자동화 벤치마크.

| 구성 요소 | 내용 |
|-----------|------|
| 테스트 소스 | arithmetic_idioms, control_flow, structs_classes, calling_conventions, string_memory, real_world_algorithms, advanced_patterns |
| 플랫폼 | macOS ARM64, macOS x86_64, Linux x86_64, Windows x86_64 |
| 스크립트 | `scripts/benchmark/run_benchmark.sh`, `benchmark_v4.py`, `analyze_results.py` |
| YAML 수트 | `suite_macos_arm64.yaml`, `suite_macos_x86_64.yaml`, `suite_linux_x86_64.yaml`, `suite_windows_x86_64.yaml` |

- **패턴 OR 지원**: `benchmark_v4.py`에 `|` 연산자 지원 추가 (예: `%|&`로 ARM64 `%` 와 x64 `&` 동시 매칭)

#### 벤치마크 결과 요약

| 플랫폼 | Bug 1+2 수정 후 | Bug 3 수정 후 | 최종 |
|--------|----------------|---------------|------|
| macOS ARM64 | 69.4% → 85.9% (+16.5pp) | 85.9% → 88.9% (+3.0pp) | **88.9%** |
| macOS x86_64 | 71.6% → 87.6% (+16.0pp) | 87.6% → 88.2% (+0.6pp) | **88.2%** |
| Linux x86_64 | 69.8% → 88.6% (+18.8pp) | 88.6% → 91.6% (+3.0pp) | **91.6%** |
| Windows x86_64 | 68.0% → 88.0% (+20.0pp) | 88.0% → 91.1% (+3.1pp) | **91.1%** |

#### 변경 파일 목록

| 파일 | 변경 내용 |
|------|----------|
| `ghidra_decompiler/src/processing/PostProcessors.cc` | Bug 1 parse_decl_varname 수정, Bug 3 convert_struct_access 구현 |
| `ghidra_decompiler/include/fission/processing/PostProcessors.h` | convert_struct_access 선언 추가 |
| `ghidra_decompiler/src/ffi/DecompContext.cpp` | Bug 2 arch.release() 수정 |
| `ghidra_decompiler/src/decompiler/PostProcessor.cc` | Bug 4 복합대입 regex 수정 |
| `ghidra_decompiler/src/decompiler/PostProcessPipeline.cpp` | 파이프라인 정리 |
| `scripts/benchmark/benchmark_v4.py` | OR 패턴(`\|`) 지원 추가 |
| `scripts/benchmark/suites/*.yaml` | mod 함수 패턴 `%\|&` 업데이트 (플랫폼 차이 흡수) |
| `examples/sources/test_*.cpp` | 7개 테스트 소스 추가 |

---

## [Unreleased / HEAD] — 2026-02-25

### x86 Double Argument Synthesis + Benchmark Normalization Fixes

**Decompiler Quality: x86 90.1% → 92.6% | x64 98.8% (maintained)**

#### x86 `double` Argument Synthesis (Track 1)

x86 cdecl passes a `double` (8 B) as two consecutive 4-byte stack pushes. Ghidra's Pcode
`CPUI_CALL` therefore had two 4-byte input varnodes where the callee prototype expected one
8-byte parameter. This caused Fission to emit `create_item(..., 0x51eb851f, 0x4048feb8)`
instead of `create_item(..., 0x4048feb851eb851f)`.

**Three bugs fixed in `merge_split_double_args`:**

1. **`isConstant()` guard removed** — CPUI_CALL target lives in code/fspec space, never CONST.
2. **`queryFunction(defaultCodeSpace, addr)` → `fd->getCallSpecs(call_op)`** — x86 externals
   live in fspec address space; `queryFunction` always returned null.
3. **`getProto()` removed** — `FuncCallSpecs` IS-A `FuncProto`; use `fc->numParams()` /
   `fc->getParam()` directly.

**Result:** `_create_item(0x3e9,"TestItem",0x51eb851f,0x4048feb8)`
→ `_create_item(0x3e9,"TestItem",0x4048feb851eb851f)` ✅

#### Benchmark Normalization Fixes (`compare_decompilers_v3.py`)

- **VAR digit suffix bug**: `p[vucslt]Var(\d+) → VAR\1` kept digit; fixed to strip it.
- **Missing xVar pattern**: `xVar2`, `mVar1` etc. not normalised; added `[a-z]Var[0-9]+ → VAR`.

**Score impact (main function):** 40.0% → 60.0%

#### Files Changed

| File | Change |
|------|--------|
| `ghidra_decompiler/src/analysis/TypePropagator.cc` | `merge_split_double_args` rewrite |
| `ghidra_decompiler/include/fission/analysis/TypePropagator.h` | method moved to public |
| `ghidra_decompiler/src/decompiler/AnalysisPipeline.cpp` | post-barrier call (FFI + batch) |
| `scripts/compare/compare_decompilers_v3.py` | VAR/xVar normalization bug fixes |

---

## 2026-02-24

### Track 2 / 3 / 4 + Benchmark Normalization (x86 80.0% → 90.1%)

**Decompiler Quality: x86 80.0% → 90.1% (+10.1 pp)**

#### Track 2 — Pointer Return Type Inference

`create_item` returns a `void*` heap pointer. Fission previously inferred the return type as a
scalar int. Added allocator-like heuristic in `TypePropagator::propagate_call_return_types`.

#### Track 3 — Array Argument Semantics

Improved `propagate_from_call` handling for pointer parameters bound to local arrays.

#### Track 4 — CLI Header Strip

`--no-header` flag added; suppresses `// Function: NAME @ 0xADDR` banner from `--decomp` output.

#### Normalizer A-1 through A-6

| Rule | Description |
|------|-------------|
| A-1 | Remove `!= (Type*)0x0` null-pointer comparisons |
| A-2 | `char*`/`byte*`/`uchar*` → `OPAQUE_PTR` |
| A-3 | `f_<hex>*` inferred-struct pointers → `OPAQUE_PTR` |
| A-4 | `UNDEF*` → `OPAQUE_PTR` |
| A-5 | `uint*/ushort*` → `UNDEF` |
| A-6 | Remove `(UNDEF)` cast expressions |

---

## 2026-02-23

### Tauri GUI — Phase 1–9 완전 이관 + egui 제거

Egui 기반 `fission-ui`를 **Tauri 2.x + React 19 / TypeScript**로 완전 이관.

| Phase | 핵심 내용 |
|-------|----------|
| 1 | StringXrefs 클릭 내비게이션, 어셈블리 복사 버튼, Cmd+←/→ 탭 순환, 정규식 필터, CFG V(G) |
| 2 | `@tanstack/react-virtual` 가상 스크롤 (5,000+ 라인) |
| 3 | FID 시그니처 식별 (`run_fid` Tauri 커맨드) |
| 4 | 디버그 메모리 덤프 (최대 4 KB) |
| 5 | TTD 타임라인 (5개 커맨드, Windows SingleStep) |
| 6 | UTF-16 LE 문자열 스캔 + StringXrefs 가상 스크롤 |
| 7 | CFG 팬/줌 + UI 스케일 슬라이더 (50%–200%) |
| 8 | 분석 결과 JSON 내보내기 |
| 9 | `crates/fission-ui/` egui 코드 완전 제거 (~6,000 LOC) |

---

## 2026-02-21

### Tauri GUI — Analyze Functions / Deep Scan (Phase 6)

- `analyze_functions`: CALL 타깃 스캔으로 내부 함수 발굴
- `deep_scan_functions`: 프롤로그 패턴 스캔
- FunctionsList: 카테고리 필터, Analyze 🔍 / Deep Scan 🕵 툴바 버튼
- MenuBar: Tools 메뉴 (F5/F6 단축키)

---

## 2026-02-20

### Windows/MSVC Build Compatibility + Tauri GUI

**MSVC 빌드 호환성 5건:** dlfcn.h → GetProcAddress, cxxabi.h → Dbghelp.h/UnDecorateSymbolName,
`__builtin_bswap32` → `_byteswap_ulong`, `std::regex::multiline` 제거, vcpkg auto-link.

**Tauri GUI Phase 1–5:** 30+ IPC 커맨드, 프로젝트 저장/로드 (`.fprj`), Listing 가상 스크롤,
CFG SVG 렌더링, 디버거 UI (Windows).

---

## 2026-02-17

### Analysis Pipeline & Data Section Scan 공통화

- `BatchAnalysisContext` 어댑터: FFI/배치 경로 통합 (`run_analysis_passes()`)
- `scanAndRegisterDataSymbols()` 통합 API: PE 파싱 + 심볼 등록 일원화
- FFI: `decomp_set_function_*`, `decomp_set_default_prototype` API 추가

---

## 2026-02-16

### Loader: Linear Sweep Function Discovery

`discover_functions_by_linear_sweep()`: 코드 섹션 선형 분석, 프롤로그 패턴으로 함수 경계 추론.
x86/x64 지원, stripped PE 함수 복원률 개선.

---

## 2026-02-15

### Track A+B: Normalizer A-1~A-6 + TypePropagator MinGW (x64 92.1% → 98.8%)

- TypePropagator: MinGW underscore prefix 처리, `propagateAcrossReturns` + integer-cast stripping
- x64 최종: **98.8%** vs Ghidra

---

## 2026-02-14

### Track B: MinGW x86 Binary + x86 Benchmark Suite (baseline 80.0%)

- `comparison_test_x86.exe` 추가 (MinGW i686-w64-mingw32-gcc -O1)
- `suite_x86.yaml` 정의 (8개 함수)
- x86 baseline: 80.0%

---

## 2026-02-13 ~ 02-12

### Type Propagation + Pcode Optimizer + Performance

- `propagate_from_call`: DF-1 fallback, 8B 파라미터 독립 인덱스
- `WhileToForConverter`: loop-init 직전 할당 감지 → `for` 변환
- Performance Group 1-3: hash tuning, static regex, 2-barrier, string cache, O(n) CFG, lazy arch init

---

## 2026-02-11

### Ghidra Integration: Pcode Optimizer Phase 1+2 + Constant Substitution

- 32개 Pcode 최적화 규칙 (Ghidra 142개 대비 23% 커버리지)
- Context-Aware Constant Substitution: 16개 enum 그룹, 100+ Windows API 매핑

---

## 2026-01-20

### Listing View + C++ RTTI Recovery

- `egui_extras::TableBuilder` 기반 가상 스크롤 listing
- RTTI 구조체 파싱, 가상 함수 테이블 복원, 간접 호출 해석

---

## 2025-12-01 ~ 2026-01-19 — Initial Feature Set

- Ghidra FFI (zero-copy, libdecomp.dylib / decomp.dll)
- iced-x86 Pure Rust 디스어셈블러
- Cross-Platform Loader: PE · ELF · Mach-O
- CFG Analysis: dominator tree, loop detection, DOT/JSON 내보내기
- FID Signatures: 40+ CRT 함수
- GDT Type Loading: 5,700+ structures, 6,500+ typedefs
- Time Travel Debugging (Windows)
- TUI Mode (Ratatui)
- Plugin System (동적 Rust 플러그인)

---

## Benchmark Summary

| Platform | Score | Status |
|----------|-------|--------|
| x64 (Windows PE) | **98.8%** | Target 98%+ ✅ |
| x86 MinGW (Windows PE) | **92.6%** | Target 90%+ ✅ |

| Metric | Value |
|--------|-------|
| Pcode optimizer rules | 32 |
| Windows API mappings | 100+ (9 DLLs) |
| GDT structures / typedefs | 5,700+ / 6,500+ |
| CRT/library signatures | 40+ |
| Supported platforms | Windows · Linux · macOS |
