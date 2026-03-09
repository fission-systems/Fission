# FID 문자열 참조 스코어링(Phase 5) — 분석 결과

## 1. FID DB 문자열 데이터 구조 파악

### 1.1 현재 FidDatabase가 파싱하는 테이블

| 테이블 | 용도 | 함수-문자열 연결 |
|--------|------|------------------|
| **Strings Table** | key(8B) → 문자열 값 | 함수 **이름** 해상용 (name_id) |
| **Functions Table** | function_id, full_hash, name_id, ... | name_id로 함수 이름 조회 |
| **Superior Table** | (caller_id, callee_hash) 복합 키 | Caller-Callee 관계 |

### 1.2 결론: **함수 → 참조 문자열** 매핑 없음

- Ghidra FID 포맷(fid.xml)에는 **함수가 참조하는 데이터/문자열** 테이블이 **설명되어 있지 않음**.
- FID DB의 29,273개 문자열은 **메타데이터**(함수 이름, 라이브러리 이름 등)용.
- `Superior Table`은 **함수→함수** 관계만 저장. **함수→문자열** 관계는 없음.

→ **FID DB에 "후보 함수가 기대하는 문자열" 정보가 없으므로**, DB 구조 변경 없이는 기대 문자열과의 직접 매칭이 불가능.

---

## 2. 바이너리 측 문자열 추출 (이미 구현됨)

### 2.1 `collect_referenced_strings_near` (DecompilationPipeline.cc:74)

**시그니처:**
```cpp
static std::vector<std::string> collect_referenced_strings_near(
    const std::vector<uint8_t>& bytes,
    size_t func_off,
    uint64_t image_base,
    const std::map<uint64_t, std::string>& known_strings,
    ArchType arch = ArchType::X86_64);
```

**동작:**
- 대상 함수 바이트(`func_off`~)에서 LEA/MOV/PUSH 등으로 **참조 주소** 추출.
- `known_strings`(주소→문자열)에서 해당 주소의 **실제 문자열** 조회.
- x86/x64: RIP-relative LEA/MOV, `8D 05 imm32`, `68 imm32`, `B8..BF imm32`.
- ARM64: ADRP+ADD/LDR 패턴.

**반환:** 함수가 참조하는 **문자열 내용** 목록 (중복 제거).

### 2.2 `known_strings` 출처

- `run_preanalysis` Phase 7: `StringScanner::scan_ascii_strings`, `scan_unicode_strings`.
- `DataSectionScanner`가 스캔한 .rdata/.rodata 문자열.
- `state.enum_values`, `state.data_section_symbols` 등으로 보강 가능.

→ **대상 함수의 참조 문자열은 이미 추출 가능한 상태.**

---

## 3. 구현 전략: FID DB 확장 없이 적용

FID DB에 함수→문자열 정보가 없으므로, **후보 함수 이름 기반 휴리스틱**으로 보너스를 준다.

### 3.1 아이디어: 함수 이름 → 예상 문자열 패턴

- FID 후보의 `name`(예: `"printf"`, `"fopen"`)을 기준으로, 그 함수가 자주 쓰는 문자열 패턴을 **우리가 정의**.
- `actual_ref_strings`와 이 패턴의 **교집합**이 있으면 보너스.

### 3.2 휴리스틱 매핑 예시 (최소 집합)

| 함수 이름 패턴 | 예상 참조 문자열 패턴 |
|----------------|------------------------|
| printf, fprintf, sprintf, snprintf, vprintf | `%` 포함 (포맷 문자열) |
| fopen, _wfopen, fopen_s | `"r"`, `"w"`, `"rb"`, `"wb"`, `"a"`, `"rt"` |
| strcmp, strncmp, wcscmp | (짧은 문자열 여러 개 — 별도 규칙) |
| strstr, strchr | (문자열 인자 — 패턴 넓음) |

**구현:** `cand->name`이 위 패턴에 해당하고, `actual_ref_strings` 중 하나라도 조건을 만족하면 보너스.

### 3.3 문자열 “특이도” 기반 가산 (선택)

- `"File not found: %s"`, `"Unable to parse auth header"`처럼 **긴·특이한** 문자열은 식별력이 높음.
- 이런 문자열이 있고, 후보 이름이 `fopen`/`fprintf` 등 **문자열을 쓰는** 함수면 → 추가 가산.
- 반대로 `" "`, `"0"` 같은 흔한 문자열은 가산 없음.

---

## 4. 구체적 통합 방안

### 4.1 RelationValidator 확장

**Option A: `find_best_match`에 `actual_ref_strings` 인자 추가**

```cpp
MatchResult find_best_match(
    const std::vector<const FidFunctionRecord*>& candidates,
    const std::vector<uint64_t>& actual_callee_hashes,
    const std::vector<std::string>& actual_ref_strings,  // 추가
    float min_confidence_threshold = 0.3f);
```

- 내부에서 `evaluate_relations` 호출 후, `evaluate_string_refs(cand, actual_ref_strings)`로 보너스 계산.
- `final_score = relation_score + string_bonus` (상한 1.0).

**Option B: RelationValidator 외부에서 보너스 계산**

- `DecompilationPipeline`에서 `collect_referenced_strings_near` 호출.
- `find_best_match` 결과에 대해, `actual_ref_strings`가 있으면 별도 보너스 로직 적용.
- Validator는 기존 relation 검증만 담당.

→ **Option A**가 단일 진입점에서 처리되어 더 깔끔함.

### 4.2 `evaluate_string_refs` 설계

```cpp
float string_ref_bonus(const std::string& candidate_name,
                       const std::vector<std::string>& actual_ref_strings);
```

- `actual_ref_strings` 비어 있으면 `0.0f`.
- `candidate_name`이 format-string 함수(printf 등)이고, actual에 `%` 포함 문자열 있으면 `+0.3f`.
- `candidate_name`이 fopen 계열이고, actual에 `"r"`, `"w"` 등 있으면 `+0.3f`.
- 그 외: `+0.1f` (문자열을 쓰는 함수로 추정).

### 4.3 DecompilationPipeline 수정

- `run_signature_analysis` 내 FID 매칭 루프에서:
  1. `known_strings`: `scanned_strings`(Phase 7) + `DataSectionScanner` 결과 통합.
  2. `collect_referenced_strings_near(bin_bytes, off, image_base, known_strings, arch)` 호출.
  3. `find_best_match(all_candidates, actual_callees, actual_ref_strings, min_threshold)` 호출.

- `collect_referenced_strings_near`는 **아직 `run_signature_analysis`에서 호출되지 않음**. 여기에 통합 필요.

---

## 5. `known_strings` 사용 가능 여부

- `run_signature_analysis` 시점에 `state`에는 Phase 7 `scanned_strings`가 있음.
- `run_preanalysis`가 먼저 호출되므로 `scanned_strings`는 이미 준비됨.
- `run_signature_analysis`가 `state`를 받으므로, `state.enum_values` 또는 유사 맵을 `known_strings`로 전달 가능.
- `run_preanalysis`와 `run_signature_analysis`의 호출 순서 및 `state` 전달 경로를 한 번 더 확인 필요.

---

## 6. 구현 우선순위 요약 (✅ Phase 5 완료)

| 단계 | 작업 | 상태 |
|------|------|------|
| 1 | `run_signature_analysis`에서 `known_strings` 맵 확보 (state.enum_values) | ✅ |
| 2 | FID 매칭 루프에서 `collect_referenced_strings_near` 호출 | ✅ |
| 3 | `RelationValidator::find_best_match`에 `actual_ref_strings` 인자 추가 | ✅ |
| 4 | `string_ref_bonus` 구현 (함수 이름 패턴 → 예상 문자열 매칭) | ✅ |
| 5 | `final_confidence = relation_score + string_bonus` 적용 및 상한 1.0 처리 | ✅ |

### 구현된 휴리스틱 (RelationValidator.cc)

| 함수 이름 패턴 | 참조 문자열 조건 | 보너스 |
|----------------|------------------|--------|
| printf, sprintf, fprintf, swprintf, vprintf, vsprintf, snprintf 등 | `%` 포함 | +0.3f |
| fopen, _wfopen, popen, _popen, fopen_s, _wfopen_s | `"r"`, `"w"`, `"a"`, `"rb"`, `"wb"` 등 | +0.3f |
| _assert, _wassert, __assert_fail, __crtMessageBox, assert | `.c`, `.cpp`, `Expression:`, `Assertion failed` 등 | +0.4f |

---

## 7. 한계 및 참고

- FID DB에 함수→문자열 관계가 없어, **함수 이름 기반 휴리스틱**에 의존.
- 보너스 규칙은 초기에 보수적으로 두고, putty 등으로 검증 후 점진적으로 확대하는 것이 안전함.
