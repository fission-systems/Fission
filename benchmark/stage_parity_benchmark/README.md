# Stage parity benchmark

Mid-stage parity tooling belongs **only** under this directory. Do **not** wire it into default `cargo test` / workspace unit tests; fixture paths and fixed addresses live in manifests here (or sibling benchmark manifests), not in `crates/*/src/**/*.rs`.

## Oracle 조인

Ghidra 쪽 사실은 [`benchmark/ghidra_oracle_benchmark/`](../ghidra_oracle_benchmark/README.md) 출력(`rows[].ghidra`)을 단일 oracle 소스로 삼습니다. Fission 쪽은 향후 stage별 덤프(JSON) 또는 기존 벤치 행의 `preview_build_stats` 등과 조인합니다.

**조인 키**

1. 바이너리 ID (`binary_id`)가 있으면 우선 한정합니다.
2. 함수 주소는 Grand Finale와 동일하게 정규화된 문자열(`normalize_address`)로 매칭합니다.
3. 이름은 디버깅용 보조 키로만 사용합니다 (충돌 시 oracle 매니페스트의 `match_evidence`를 신뢰).

## Owner bucket 예시

디버깅 라벨은 원인 분해용 카테고리입니다 (실제 필드 이름은 도구별로 확장 가능).

| 증상 | Ghidra oracle | Fission 관측 (예시) | `owner_bucket` 후보 |
|------|----------------|---------------------|---------------------|
| 호출 타깃 불일치 | `call_targets`에 `printf` 존재 | call recovery 서브패스 폴백 카운트 증가 | `call_target_missing` |
| xref 과소 | `xref_out_count` 높음 | 내부 xref 테이블 sparse | `xref_pipeline_gap` |
| 서명 불일치 | `signature` / `param_count` 확정 | 타입 힌트 없음 | `type_facts_stage` |

Raw P-code는 일치하지만 최종 C만 어긋나는 경우, oracle의 **문자열 참조·외부 호출 수·파라미터 수**를 보면 “어느 단계에서 사실이 줄었는지”를 좁힐 수 있습니다.

## 관련 도구

- 지연·타임아웃 교차 요약: [`benchmark/timeout_distribution_benchmark/`](../timeout_distribution_benchmark/README.md)
