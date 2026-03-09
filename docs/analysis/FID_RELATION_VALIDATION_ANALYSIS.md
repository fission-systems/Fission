# FID Relation 검증 강화 — 코드 분석 및 개선 설계

## 1. 현행 로직 파악

### 1.1 RelationValidator.cc

**역할:** 해시가 일치하는 여러 FID 후보 중, Call Graph Relation이 가장 잘 맞는 후보를 선별.

**`evaluate_relations(caller_id, actual_callee_hashes)`:**
- `actual_callee_hashes`: 대상 함수가 **실제로 호출하는** 하위 함수들의 full_hash 목록.
- `db->has_relation(caller_id, callee_hash)`: FID DB의 Superior Table에 `(caller_id, callee_hash)` 관계가 있는지 확인.
- **스코어:** `matched / checked` (0.0 ~ 1.0). `checked == 0`이면 **0.5(중립)** 반환.

**`find_best_match(candidates, actual_callee_hashes)`:**
- 각 후보에 대해 `evaluate_relations` 호출.
- `score >= best.confidence`인 후보를 선택. `best` 초기값: `confidence = -0.1`.
- **문제:** `actual_callee_hashes`가 비어 있으면 모든 후보에 0.5점 → 마지막 후보가 임의로 선택됨.

### 1.2 FidDatabase.cc

**Relation 저장 구조:**
- `superior_relations`: `(caller_id * FNV_64_PRIME) ^ callee_hash` 형태의 8바이트 키 집합.
- `has_relation(caller_id, callee_hash)`: 위 식으로 키를 계산해 `superior_relations`에서 검색.

**Callelee 해시 수집 한계:**
- `addr_to_hash`에는 **프롤로그 스캔에 잡힌 함수**의 해시만 들어감.
- IAT로 호출되는 외부 함수(예: `kernel32!CreateFileA`)의 주소는 `prologue_candidates`에 없음 → `actual_callee_hashes`에 포함되지 않음.

### 1.3 DecompilationPipeline.cc (run_signature_analysis)

**FID 매칭 흐름 (762-797행):**
```
1. all_candidates.size() == 1  → 무조건 채택 (검증 없음)  ← False Positive 위험
2. all_candidates.size() > 1   → RelationValidator.find_best_match 사용
   - actual_callees: CALL rel32(E8)로 추출. target이 addr_to_hash에 있을 때만 추가.
   - result.validated 여부와 관계없이 result.name 사용
   - result.name이 비면 all_candidates[0]->name 사용
```

**문제점:**
1. **단일 후보:** Relation 검증을 수행하지 않고 바로 채택.
2. **다중 후보 + Callee 정보 없음:** 모든 후보에 0.5점 → 사실상 무작위 선택.
3. **`result.validated` 미사용:** relation 검증이 실패(0점)해도 이름을 부여함.
4. **함수 크기 미고려:** 작은 래퍼/썽크는 해시 충돌 확률이 높음.

---

## 2. 스코어링 알고리즘 개선 설계

### 2.1 조건별 가중치 (제안)

| 조건 | 가중치 | 설명 |
|------|--------|------|
| **Relation 일치률** | 기존 `matched/checked` | 이미 구현됨. `checked > 0`일 때만 의미 있음. |
| **Callee 정보 부재 패널티** | `checked == 0` → 신뢰도 0 | 검증 불가 시 채택하지 않음. |
| **함수 크기 패널티** | `code_bytes < 20` → 임계값 상향 | 작은 함수는 해시 충돌 시 FP 위험 증가. |
| **문자열 참조 보너스** | (선택) +가산점 | 함수 근처에서 참조하는 문자열이 DB와 일치하면 가산. |

### 2.2 구체적 변경 포인트

#### A. RelationValidator — `evaluate_relations` 수정

**현재:**
```cpp
if (checked == 0) return 0.5f;  // 중립 → 임의 선택 유도
```

**개선안:**
```cpp
if (checked == 0) return 0.0f;  // 검증 불가 시 0점 (채택하지 않음)
```

#### B. RelationValidator — `find_best_match`에 임계값 도입

**추가 로직:**
- `best.confidence`가 **임계값(예: 0.3f) 미만**이면 `validated = false`, `name` 비움.
- 파이프라인에서 `validated == false`일 때는 `sub_xxx` 유지.

#### C. DecompilationPipeline — 크기 기반 임계값

**추가:**
- 함수 바이트 수 < 20 (또는 `code_unit_size` < 5)이면 임계값을 더 높게 (예: 0.6f).

#### D. DecompilationPipeline — `validated` 활용

**현재 (790-794행):**
```cpp
if (!result.name.empty()) {
    state.fid_function_names[addr] = result.name;
} else {
    state.fid_function_names[addr] = all_candidates[0]->name;  // FP 유발
}
```

**개선안:**
```cpp
if (result.validated && !result.name.empty()) {
    state.fid_function_names[addr] = result.name;
}
// else: 매칭하지 않음 → sub_xxx 유지
```

#### E. 단일 후보( all_candidates.size() == 1 ) 처리

**현재:** 무조건 채택.

**개선안:**
- 함수 크기가 작으면(`hash_len < 24` 등) RelationValidator로 한 번 검증 시도.
- `actual_callee_hashes`가 비어 있으면 채택하지 않음(또는 specific_hash 검증만 수행).

---

## 3. 구현 우선순위

| 단계 | 파일 | 변경 내용 |
|------|------|-----------|
| 1 | `RelationValidator.cc` | `checked == 0`일 때 `return 0.0f` |
| 2 | `RelationValidator.cc` | `find_best_match`에 confidence 임계값(0.3f) 및 `validated` 로직 |
| 3 | `DecompilationPipeline.cc` | `result.validated` 확인 후에만 이름 부여 |
| 4 | `DecompilationPipeline.cc` | 단일 후보에 대해 크기 체크 및 검증 조건 추가 |
| 5 | (선택) | `collect_referenced_strings_near` 활용한 문자열 기반 가산점 |

---

## 4. 문자열 참조 스코어링 (Phase 2 확장)

`DecompilationPipeline.cc`에 이미 `collect_referenced_strings_near()`가 있습니다.
- 함수 바이트에서 LEA/MOV 등으로 참조하는 문자열 주소를 추출.
- `known_strings`(DataSectionScanner 등에서 구한)와 매칭.

**활용 방안:**
- FID DB에 함수별로 기대 문자열 패턴이 있다면, 실제 참조 문자열과 비교해 가산점/감점 적용.
- 현재 FIDBF 스키마에는 함수–문자열 관계가 명시적으로 없으므로, 1차 개선(Relation 검증) 완료 후 검토 권장.

---

## 5. 요약

1. **RelationValidator:** `checked == 0` → 0.5 대신 0.0 반환, confidence 임계값 및 `validated` 플래그 도입.
2. **DecompilationPipeline:** `validated == true`일 때만 FID 이름 부여, 단일 후보도 크기·검증 조건 적용.
3. **검증:** putty.exe 등으로 이전 FP 케이스가 `sub_xxx`로 남는지, 정상 매칭은 유지되는지 확인.

---

## 6. 구현 완료 (2026-03)

### RelationValidator.cc
- `evaluate_relations`: `actual_callee_hashes.empty()` 또는 `checked == 0` → `0.0f` 반환
- `find_best_match`: `min_confidence_threshold` 인자 추가 (기본 0.3f)
- `best.confidence < min_confidence_threshold` → `validated = false`

### DecompilationPipeline.cc
- 단일/다중 후보 통합: 모든 후보가 RelationValidator 경유
- `result.validated && !result.name.empty()`일 때만 FID 이름 부여
- 함수 크기 패널티: `estimated_size < 24` 바이트 시 임계값 0.6f
