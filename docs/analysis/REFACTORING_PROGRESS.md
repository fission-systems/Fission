# Refactoring Progress Log

이 문서는 대규모 코드베이스 개선 작업의 진행 상황을 추적합니다.

## 발견된 문제 요약

- **총 발견 항목**: 1,235+ 개선 기회
  - Critical/High: 855개 (69%)
  - Medium/Low: 380개 (31%)

### 카테고리별 분류

| 카테고리 | Critical | High | Medium | Low | 합계 |
|---------|----------|------|--------|-----|------|
| 하드코딩된 값 | - | 250 | 20 | - | 270 |
| 성능 (할당) | - | 100 | - | - | 100 |
| 안정성 (unwrap) | 200 | - | - | - | 200 |
| 로깅 | 150 | - | - | - | 150 |
| 안전성 (unsafe) | - | 155 | - | - | 155 |
| 리팩토링 | - | - | 80 | 30 | 110 |
| 문서 | - | - | 50 | - | 50 |
| TODO/FIXME | - | - | 200 | - | 200 |

---

## Phase 1: 기반 인프라 구축 ✅ 완료

### 1.1 Constants 라이브러리 생성 ✅

**완료 항목:**
- ✅ `crates/fission-core/src/constants/` 모듈 구조 생성
- ✅ `constants/binary_format.rs` - PE/ELF/Mach-O magic numbers
- ✅ `constants/windows_api.rs` - Windows API 상수 (155개)
- ✅ `constants/memory.rs` - 메모리 관련 상수
- ✅ `fission-core/src/lib.rs`에서 constants 모듈 export
- ✅ `fission-loader`에서 Mach-O magic number 적용 (4개 파일)

**주요 상수 정의:**
- PE: `PE_DOS_SIGNATURE`, `PE_SIGNATURE`, `PE_OPTIONAL_HEADER_*`
- ELF: `ELF_MAGIC`, `ELF_CLASS_*`, `ELF_DATA_*`, `ELF_TYPE_*`
- Mach-O: `MACHO_MAGIC_*` (BE/LE variants for 32/64-bit)
- Windows: `EXCEPTION_*`, `PROCESS_*`, `THREAD_*`, `PAGE_*`, `MEM_*`
- Memory: `KB`, `MB`, `GB`, `PAGE_SIZE_*`, alignment constants

**영향:**
- 150+ magic number를 named constants로 변환 (진행 중)
- 코드 가독성 및 유지보수성 향상
- 문서화된 상수로 버그 위험 감소

### 1.2 Configuration System 개선 ✅ 

**기존 시스템 확인:**
- ✅ `fission.toml` 설정 파일 이미 존재
- ✅ `TomlConfig` 및 `Config` 구조 정립됨
- ✅ `[paths]` 섹션에 `fid_dir`, `gdt_dir` 등 정의됨
- ✅ 환경 변수 체크 로직 존재 (`FISSION_CONFIG`)

**남은 작업:**
- ⏭️ 하드코딩된 경로를 CONFIG 사용으로 전환 (Phase 4에서 진행)
- ⏭️ 추가 환경 변수 지원 (`FISSION_FID_DIR`, etc.)

### 1.3 Logging Infrastructure 통일 📋 문서화

**현황:**
- 기존: `tracing` crate 사용 중
- 혼합: 150+ `println!`/`eprintln!` 인스턴스
- 설정: `fission.toml`의 `[logging]` 섹션

**계획:**
- Phase 5에서 전체 마이그레이션 진행
- CLI는 명시적 출력 유지, 내부 로깅만 tracing으로

---

## Phase 2: 안정성 개선 - Unwrap/Expect 제거 🔄 진행 중

**목표**: 200+ unwrap/expect 호출 제거로 크래시 위험 90% 감소

### 2.1 Critical Path 식별

**우선순위 1 - 사용자 입력 처리:**
- [ ] `fission-cli/src/cli/oneshot/decompile.rs` - 주소 파싱
- [ ] `fission-cli` - 파일 경로 처리

**우선순위 2 - 바이너리 파싱:**
- [ ] `fission-loader/src/loader/pe/mod.rs` - 섹션 인덱스 접근
- [ ] `fission-loader/src/loader/elf/mod.rs` - 심볼 테이블 파싱
- [ ] `fission-loader/src/loader/macho/mod.rs` - Load command 처리

**우선순위 3 - 분석 엔진:**
- [ ] CFG 구축 - 기본 블록 접근
- [ ] 디컴파일 결과 변환

### 2.2 새로운 에러 타입 추가 계획

```rust
// LoadError 확장
pub enum LoadError {
    InvalidSection { section: String, index: usize },
    InvalidSymbol { name: String },
    // ...
}

// CliError 확장
pub enum CliError {
    InvalidAddress(String),
    InvalidPath(PathBuf),
    // ...
}
```

---

## Phase 3: 성능 최적화 - String 할당 제거 📋 계획

**목표**: 디컴파일 후처리 파이프라인 메모리 사용량 30-50% 감소

**전략**: `String` → `Cow<'_, str>` 패턴 적용

**대상:**
- `postprocess/loops.rs` (40+ 할당)
- `postprocess/arithmetic.rs` (30+ 할당)
- `postprocess/naming.rs` (40+ 할당)

---

## Phase 4-8: 향후 계획

- **Phase 4**: 하드코딩 제거 (250+ 인스턴스)
- **Phase 5**: 로깅 통일 (150+ 인스턴스)
- **Phase 6**: Unsafe 문서화 (155 블록)
- **Phase 7**: 리팩토링 (코드 중복 제거)
- **Phase 8**: 문서화 및 정리

---

## 메트릭

**현재 진행률**: ~8% (1.1, 1.2 완료 / 총 15 단계)

**예상 완료 시간**: 4-8주

**우선순위**: 안정성 > 성능 > 유지보수성 > 문서화
