# Timeout distribution benchmark

오프라인으로 **Ghidra oracle JSON**과 **Fission 결과 JSON**을 주소 기준으로 조인해, 성공/실패 교차표와 선택적 임계 초(threshold)별 “느리거나 실패한” 카운트, 성공 표본의 지연 분포(p50/p95/p99)를 요약합니다.

## 입력

### `--oracle`

[`benchmark/ghidra_oracle_benchmark/export_oracle.py`](../ghidra_oracle_benchmark/export_oracle.py) 출력 (`rows[].address`, `rows[].ghidra.decompile_success`, `rows[].ghidra.decompile_sec`).

주소가 `null`인 스냅샷 행은 조인에서 제외됩니다.

### `--fission`

다음 중 하나의 형태를 자동 탐지합니다.

- 최상위 `entries`: `{ "<addr>": { "success", "wall_sec" | "decomp_sec", ... } }`
- `functions[]`: 각 원소에 `address`, `success`, `wall_sec` 또는 `decomp_sec`
- `pairwise.pyghidra_vs_fission.comparisons[]`: `address`(또는 `seed_address`)와 성공 플래그·시간 필드

주소 정규화는 Grand Finale와 동일하게 [`normalize_address`](../full_benchmark/grand_finale_support/metrics.py)를 사용합니다.

## 실행 예시

```bash
python3 benchmark/timeout_distribution_benchmark/summarize_timeouts.py \
  --oracle benchmark/artifacts/ghidra_oracle/export_smoke.json \
  --fission path/to/fission_functions_or_benchmark_slice.json \
  --thresholds-sec 1,5,30,180 \
  --out benchmark/artifacts/timeout_distribution/summary.json
```

## 해석 메모

- “소프트 타임아웃” 버킷은 **실패한 행 + 주어진 초를 초과한 성공 행**을 포함합니다 (실제 디컴 엔진의 하드 타임아웃과는 다를 수 있음).
- Fission 쪽 단계별 시간은 행에 `preview_build_stats` 등이 있으면 별도 확장 스크립트로 분해할 수 있습니다 (본 스크립트는 요약치 중심).
