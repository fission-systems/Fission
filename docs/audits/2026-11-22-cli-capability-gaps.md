# 라이브러리에는 있는데 CLI에는 없던 기능 (조사와 조치, 2026-11-21 ~ 22)

**조사 기준:** `origin/main` `5073a1c63`. **조치 기준:** `18f9e3d94`.

**조사 방법**
- CLI 목록은 release 바이너리의 `--help`를 재귀로 덤프해서 만들었습니다.
- 라이브러리 24개 크레이트는 코드를 읽고 호출 경로를 추적했습니다. 노출 여부는 이름이 아니라 실제 도달 여부로 판정했습니다: `crates/fission-cli/src` 또는 `crates/fission-script/src`의 코드 경로가 그 기능까지 도달해야 노출된 것으로 봤습니다.

**동적 검증 범위:** 동적 기능은 checked-in fixture와 테스트가 직접 생성한 바이너리로만 확인했습니다. DecBench/evalkit 코퍼스에는 동적 분석을 하지 않았습니다.

**범례**
- 노출: 없음 / 부분 / 전체
- 동작: 예 / 부분 / 스캐폴드 / 미검증
- 작업량: S = 플래그나 필드 추가, M = 새 명령, L = 새 라이브러리 코드

## 1. 조사 당시의 CLI

- **명령 수:** 최상위 명령은 25개, 중첩까지 펼치면 85개 경로였습니다. `crates/fission-cli/AGENTS.md`에는 9개만 적혀 있었습니다.
- **받기만 하고 아무 코드도 읽지 않던 플래그:**
  - `decomp`의 `--compiler-id`, `--engine`, 숨김 `--format`은 경고를 냈습니다.
  - `decomp --profile`은 메타데이터에 값을 되찍을 뿐이었고, **경고가 없었습니다**.
  - `inventory --compiler-id`도 아무 코드가 읽지 않았는데, 경고는 `decomp`라는 엉뚱한 이름을 가리켰습니다.
- **성공처럼 보이지만 동작하지 않던 것:**
  - `debug hw-bp`는 Windows가 아닌 OS에서 exit 0으로 끝났습니다.
  - `sandbox --snapshot-at`은 스냅샷을 디스크에 저장하지 않았습니다.
  - `--dump-trace`는 명령어 단위 트레이스를 기록하지 않았습니다.
  - `--ttd-seek`는 seek 후 PC를 로그에만 남겼습니다.
  - `--language`는 원시 펌웨어용이라고 안내했지만, 헤더 없는 파일은 로드에 실패했습니다.
- **어떤 기본 백엔드에도 구현이 없는 명령:** `debug find`, `get-flag`, `set-flag`, `dll-bp`, `ex-bp`. `mem-bp`는 `--emulator`에서만 동작합니다.
- **스키마:** `schema`의 `data`는 `x-data-schema: untyped`입니다.

## 2. 조치 결과 (PR)

| PR | 내용 |
|---|---|
| [#987](https://github.com/fission-systems/Fission/pull/987) | `fission-analysis-protocol`의 HTTP 세션 계약 삭제. 서버가 없었습니다. CLI가 출력하는 JSON 형태는 유지했습니다. |
| [#988](https://github.com/fission-systems/Fission/pull/988) | `decomp --json`에 `globals`, `frame`, `declared_callees` 추가. `verify --seed`와 반례 필드. `--profile` 경고, `inventory` 경고의 명령 이름, `hw-bp` 실패 처리 |
| [#991](https://github.com/fission-systems/Fission/pull/991) | 전역 `--base`/`--entry`로 원시 이미지 로드. `languages` 명령. `disasm --json`의 DWARF `source` |
| [#996](https://github.com/fission-systems/Fission/pull/996) | `sandbox --sym-explore`의 `--find`/`--avoid`/`--sym-max-steps`와, 경로별로 풀어낸 입력값. `--ttd-seek` 상태와 `--ttd-read`. `--snapshot-out`. `--interp`, `--timeout-ms`, `--dynlink`/`--ld-so`/`--lib-path`/`--lazy-bind`. `--dump-trace` 도움말 정정 |
| [#1006](https://github.com/fission-systems/Fission/pull/1006) | `call` 명령(함수 하나를 원하는 인자로 실행, `--stub-calls`). 디버그 세션의 `--record`, `back-step`, `seek` |
| [#1008](https://github.com/fission-systems/Fission/pull/1008) | `types`(출처와 verified 표시), `whatis`, `api`. `identify` 매치 실패 시 해시. `db watch`/`rm-watch`. `info --identity`의 TLS 콜백 주소, CodeView, Rich 레코드 |
| 이 문서의 PR | README, `docs/CLI.md`, `crates/fission-cli/AGENTS.md`, `skills/fission/SKILL.md` 갱신 |

**병합 순서:** #988 → #991 → #996 → #1006 → #1008 → 문서 PR은 앞 PR 위에 차례로 쌓여 있습니다. #987은 독립적입니다.

**품질 주장 여부:** 모든 변경은 이미 계산되던 값을 내보내거나 CLI와 라이브러리를 연결하는 기계적 변경입니다. 디컴파일 결과의 의미를 바꾸지 않으므로 품질 개선이라고 주장하지 않습니다.

### 조치 과정에서 함께 고친 결함
- **역방향 seek가 불가능했던 문제:** `Emulator`는 run 루프의 블록 경계에서만 TTD 스냅샷을 기록했습니다. 그래서 single-step만 하는 세션에는 돌아갈 스냅샷이 없었습니다. 기록 코드를 `Emulator::record_ttd_snapshot`으로 분리하고, 디버그 백엔드가 launch 시점에 step 0 스냅샷을 남기도록 했습니다.
- **`--ttd-seek` 동작 범위:** 첫 스냅샷보다 앞의 step으로는 seek가 실패합니다. 추측하지 않고 실패하는 것이 맞는 동작이라 그대로 두고, 문서에 적었습니다.
- **타입 출처를 구분할 수 없던 문제:** `InferredTypeInfo`에 출처가 없었고, Go와 Swift가 둘 다 kind를 `Struct`로 냈습니다. `source` 필드를 추가하고, 로더가 타입을 만드는 모든 지점에서 채우도록 했습니다.

## 3. 남은 갭

| 항목 | 이유 |
|---|---|
| 솔버 단독 질의(`eval`, `min`, `max`), SMT-LIB 내보내기, 충돌 예산 | 사용자가 식을 입력할 문법(파서)이 없습니다(L). `verify`에서 SMT-LIB을 내보내는 형태라면 S–M입니다. |
| P-code 기반 DIR 보증 등급(`DirPipeline`) | 아직 "first slice"로, 연속되고 부작용 없는 정수 영역만 다룹니다. |
| 구조화 엔진과 selection axis 플래그 | DecBench 수치를 바꿀 수 있는 설정이라, 노출 여부는 사용자가 결정해야 합니다(지시에 따라 제외). |
| 사용자 정의 타입과 헤더 적용, Rhai의 decomp/xrefs/db 바인딩 | `fission-project` 스키마와 스크립트 API를 새로 설계해야 합니다(L). |
| 어셈블러, 자기 바이너리로 FID 시그니처 생성, HIR AST JSON | 라이브러리에 아직 없는 기능입니다. |
| 콜그래프 DOT 내보내기, 지배자 트리와 루프 보고, VSA 질의, 함수 발견 근거 | 동작하는 라이브러리는 있지만 이번 범위(우선순위 1~10)에 포함되지 않았습니다. |
| 실행 기록 질의("이 주소에 누가 썼나"), PE에 argv 전달 | 질의 API가 없거나(L), 이번 범위에 포함되지 않았습니다. |
| `debug find`, `get-flag`, `set-flag`, `dll-bp`, `ex-bp` | 백엔드 구현이 없고, 이미 명시적인 오류를 냅니다. 구현은 별도 작업입니다. |
| 디컴파일러 옵션 `--compiler-id`, `--engine`, `--format`, `--profile` | 연결할 실제 설정이 없습니다. 지금은 모두 경고를 냅니다. 제거할지 연결할지는 1.0 결정 사항입니다. |

## 4. 노출하지 않기로 한 것

| 항목 | 이유 |
|---|---|
| 플러그인 시스템(`fission-plugin`) | `PluginAPI` 구현체가 없고, 검색 경로는 저장만 되고 읽히지 않습니다. 이슈 #557, #451 참고 |
| Windows 프로세스 덤프, IAT 재구성, 안티디버그 우회 | 모듈 주석이 "unported"라고 적고 있습니다. |
| `PatchManager`, `DecompilerCache`, `string_xrefs` | 죽은 코드이거나 기존 명령과 겹칩니다. |
| CRT 패턴의 바이너리 전체 스캔 | 파일 오프셋을 `addr - image_base`로 계산하는 버그가 있습니다. |
| GUI 설정 저장소(`config_store`) | GUI를 제거하면서 남은 코드입니다. |

## 5. 다음에 볼 만한 것

1. **명령별 `data` JSON 스키마.** 이번에 추가한 필드가 많아서 타입이 있는 스키마의 가치가 커졌습니다.
2. **`verify --emit-smt <dir>`.** 솔버를 외부 검증기와 연결하는 가장 작은 경로입니다.
3. **`list --json`의 함수 발견 근거.** 이미 계산되는 `FunctionDiscoveryReport`와 provenance 인덱스를 내보내는 작업입니다.
