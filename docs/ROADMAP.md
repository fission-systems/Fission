## Fission Roadmap (High‑Level)

이 문서는 여러 아이디어/분석 문서에 흩어져 있던 TODO와 계획을 **한 곳에서 개요 수준으로** 정리한 것입니다.  
세부 설계·노트는 기존 문서(`docs/analysis/*`, `docs/idea/*`)를 그대로 참조하되, 우선순위와 현재 방향은 이 문서를 기준으로 봅니다.

---

## 1. Decompiler & FID

- **FID 정확도 향상 (Call‑graph 기반)**  
  - `docs/idea/ESSENTIAL_IMPROVEMENTS_2026-01-11.md` 1번 항목.  
  - 작업 개요:
    - Call graph builder를 `fission-analysis` 쪽에 추가.
    - `fission-signatures`에서 Parent/Child relation 제약 지원.
    - Relation을 만족하지 않는 FID 매치는 버리기.

- **FID DB 확장 / Ghidra FIDB 활용**  
  - Ghidra `.fidbf` → Rust 측에서 직접 읽거나, 변환 파이프라인 정리.  
  - `utils/signatures/fid`에 있는 기존 자원을 일관된 방식으로 로딩.

- **지속적인 타입/상수 후처리 개선**  
  - 관련 배경: `docs/analysis/CONSTANT_SUBSTITUTION.md`, `STRING_INLINING.md`,
    `TYPE_PROPAGATION_ANALYSIS.md`, `TYPE_PROPAGATION_STATUS.md`.
  - 방향:
    - 이미 구현된 최적화 규칙 정리 + 누락된 케이스 추가.
    - 분석 품질과 성능(시간/메모리) 밸런스를 유지하는 선에서 점진 개선.

---

## 2. Dynamic Debug & Timeline (Tauri 중심)

- **Windows 디버거 백엔드와 Tauri 연결**  
  - 현재 상태:
    - `crates/fission-tauri/src-tauri/src/commands.rs`의 `debug_*`는
      `DebugStateDto`만 조작하는 스텁 수준.
  - 목표:
    - `fission-analysis::debug::platform::windows`를 Tauri 커맨드에서 직접 호출.
    - attach / continue / step / breakpoint가 실제 프로세스에 반영되도록 연결.
    - 이벤트/레지스터 상태를 `DebugStateDto`로 노출하여 `DebugTab`/`TimelinePanel`에서 사용.

- **플랫폼 지원 정책 명시**  
  - macOS: 현재 `fission-analysis` 수준에서 stub 구현.  
  - 계획:
    - 단기: “Dynamic Debug = Windows 전용”임을 문서·UI에 명시.
    - 장기: macOS/Linux 지원 여부와 범위를 별도 결정.

---

## 3. GUI 전환: egui → Tauri

- **Tauri를 1급 시민으로**  
  - 정적 분석 기능(디컴파일, 어셈블리, CFG, Strings/Hex, Imports/Exports, 검색 등)은
    Tauri UI에 이미 대부분 이식됨.
  - 남은 작업:
    - Dynamic Debug/Timeline 완성(위 2번).
    - String XRefs 같이 “클릭 → 해당 주소로 이동”하는 워크플로우 연결 보강.

- **egui GUI의 역할 정리**  
  - 단기: “레거시/백업 UI”로 유지, 테스트와 디버깅 루트로 활용.  
  - 장기:
    - Tauri에서 모든 주요 플로우가 안정화되면 egui 코드를 제거하거나,
      개발용 최소 뷰만 남기는 방향 검토.
  - 관련 문서: `docs/gui/GUI_GUIDE.md` (egui 기준), 이 문서에서 Tauri 전환 계획을 상위 레벨로 관리.

---

## 4. 문서 및 설정 정리

- **문서 중앙화**  
  - `README.md` + `docs/architecture/ARCHITECTURE.md` + 이 `docs/ROADMAP.md`를
    상위 3대 문서로 두고, 나머지를 보조 자료로 위치시킴.
  - `docs/idea/*`, `docs/analysis/IMPROVEMENT_LOG.md`, `FUTURE_IMPROVEMENTS.md`,
    `MISSING_FEATURES_ANALYSIS.md` 등은
    - 여기에 언급된 항목과 일치하는 내용만 유지,
    - 나머지는 필요시 참고용(archive)으로 사용.

- **하드코딩/로깅/에러 정리**  
  - 하드코딩된 경로/타임아웃/버퍼 크기 → `fission-core::constants` 혹은 설정으로 이전.
  - `logging::info(&format!(...))` + `eprintln!` 패턴 → `tracing` 기반 구조화 로깅으로 점진 전환.
  - `Result<T, String>` → `FissionError`/`Result<T>`로 수렴 후, Tauri 경계에서만 직렬화.

---

## 5. 참고: 세부 문서들

이 로드맵에서 언급한 항목의 세부 내용은 다음 문서들을 참고하세요:

- 분석/최적화: `docs/analysis/CONSTANT_SUBSTITUTION.md`, `STRING_INLINING.md`,
  `TYPE_PROPAGATION_ANALYSIS.md`, `TYPE_PROPAGATION_STATUS.md`
- FID 및 시그니처: `docs/analysis/GCC_FID_IMPLEMENTATION.md`,
  `docs/idea/ESSENTIAL_IMPROVEMENTS_2026-01-11.md`,
  `docs/idea/fission_signature_tech_ideas.md`
- 디버거/다이내믹 분석: `docs/idea/debugger.md`
- GUI 전환/UX: `docs/gui/GUI_GUIDE.md`, `docs/idea/gui.md`

---

## 6. 아키텍처 개선 프로젝트 (완료)

### Phase 1-4: 모듈화 및 중앙화 ✅ (완료 - 2024)

- ✅ **Phase 1**: PostProcessors.cc 모듈화 (C++ → Rust 이전 준비)
- ✅ **Phase 2.1**: CFG 분석 모듈화 (핵심 로직 분리)
- ✅ **Phase 2.2**: FFI 레이어 단순화 (안정적인 경계)
- ✅ **Phase 3.1**: DWARF 파서 모듈화 (타입 정보 처리 개선)
- ✅ **Phase 3.2**: FFI crate 모듈화 (관심사 분리)
- ✅ **Phase 4**: Regex 패턴 라이브러리 중앙화 (일관성 및 재사용성)

**Commit**: `0217f44` - 167 files changed, 9112 insertions(+), 5382 deletions(-)

### Phase 5: 확장 가능한 인터페이스 ✅ (완료 - 2024)

- ✅ **Phase 5.1**: Tauri 커맨드 그룹화
  - 17개 명령 파일 → 5개 도메인으로 정리:
    - `binary/` (4 files): binary, metadata, hex, listing
    - `analysis/` (5 files): assembly, cfg, xrefs, annotations, analysis
    - `debugging/` (2 files): debug, ttd
    - `workspace/` (3 files): project, search, settings
    - `extensions/` (2 files): plugins, devtools
  - 51개 커맨드를 도메인별로 체계화
  
- ✅ **Phase 5.2**: Pass Trait 시스템
  - **핵심 구현**:
    - `PostProcessPass` trait: 모든 후처리 패스의 공통 인터페이스
    - `PassRegistry`: 의존성 해결 및 실행 순서 관리 (Topological Sort)
    - `PassContext`: 타입 정보, DWARF 데이터 등 공유 상태 관리
    - `PassCategory`: 6가지 카테고리 (Arithmetic, ControlFlow, Naming, Cleanup, LanguageSpecific, TypeBased)
  - **22개 Concrete Pass 구현**:
    - Language-Specific (3): Rust/Go boilerplate 제거, Swift 디맹글링
    - Type-Based (3): 필드 오프셋 교체, 타입 캐스트 삽입, DWARF 이름 적용
    - Arithmetic (2): 산술 관용구 인식, 2의 거듭제곱 곱셈 → 쉬프트
    - Cleanup (4): 배열 인덱싱, 비트 연산 → 논리 연산, 상수 조건 제거, 데드 코드 제거
    - Control Flow (8): while → for 변환, switch 재구성, if 구조 단순화
    - Naming (3): 귀납 변수명 변경, 의미론적 변수명 변경, 루프 관용구 인식
  - **통합**:
    - `PostProcessor::process_with_registry()`: 새로운 trait 기반 실행 (권장)
    - `PostProcessor::process()`: 레거시 호환성 유지
    - 런타임 pass 활성화/비활성화 지원
    - 자동 의존성 해결 및 실행 순서 최적화
  - **문서화**: [`docs/analysis/PASS_SYSTEM.md`](analysis/PASS_SYSTEM.md) 추가
  - **테스트**: 모든 유닛 테스트 통과 (pass, registry 모듈)

**향후 가능성**:
- 플러그인 아키텍처: 동적 라이브러리에서 패스 로드
- 설정 파일: TOML/JSON 기반 pass 구성
- 메트릭 수집: 패스 실행 시간 통계
- 병렬 실행: 독립적인 패스를 병렬로 실행
- 캐싱: 변경되지 않은 코드에 대한 패스 결과 캐싱

### Phase 6-7: 품질 및 문서화 (예정)

- ⬜ **Phase 6**: 테스트 인프라
  - 통합 테스트 확장
  - 벤치마크 및 성능 테스트
  - 회귀 테스트 자동화
  
- ⬜ **Phase 7**: 문서 및 메트릭
  - API 문서 완성
  - 아키텍처 다이어그램
  - 성능 메트릭 수집

