# Ghidra oracle export benchmark

PyGhidra + Ghidra headless에서 **디컴 C 문자열 외의 oracle**(xref, 호출 타깃, 문자열 참조, 서명·파라미터 요약, 디컴 성공/실패 이유 등)을 매니페스트 기준으로 JSON으로 덤프합니다.  
Rust 크레이트 유닛 테스트와 분리되어 있으며, 픽스처 경로는 **매니페스트에만** 둡니다.

## 요구 사항

- Ghidra 설치 디렉터리 (`GHIDRA_INSTALL_DIR` 또는 `--ghidra-dir`)
- Python 패키지 `pyghidra` (`pip install pyghidra`)
- 파일 기반 매니페스트의 경우 바이너리 파일이 로컬에 존재해야 함 (예: CI에서 MinGW로 빌드한 `.exe`)

## 매니페스트 스키마

최상위:

- `binaries` (배열): 각 원소는 하나의 로드 단위입니다.

바이너리 원소:

| 필드 | 의미 |
|------|------|
| `id` | 안정적인 문자열 ID |
| `path` | 저장소 루트 기준 상대 경로 (파일 로드 시 필수) |
| `hex_bytes` | 공백 없는 hex 문자열 (합성 로드 시 `path` 대신 사용) |
| `program_name` | 합성 로드 시 프로그램 이름 (기본 `synthetic`) |
| `language` | Ghidra Language ID (합성 시 기본 `DATA:LE:64:default`) |
| `loader` | 로더 이름 (기본 `BinaryLoader`) |
| `rows` | 함수별 타깃 행 배열 (`addr`, 선택 `name`, 선택 `feature_group`/`feature`). 비어 있으면 바이너리 스냅샷만 한 줄 출력합니다. |

행 원소:

- `addr`: 시드 주소 (`0x...`)
- `name`: 선택적 이름 힌트 (시드 해석에 사용, [`grand_finale_support/runners.py`](../../full_benchmark/grand_finale_support/runners.py) 와 동일 규칙)

## 출력 스키마

- `_meta`: 도구 이름, 매니페스트 경로/sha256, Ghidra 경로, 벽시계 시간, 행 수
- `rows[]`: 각 행에 `binary_snapshot`(바이너리 단위 요약), `ghidra`(함수 oracle 페이로드), 시드 매칭 메타데이터

필드 일부는 Ghidra 버전/API에 따라 채워지지 않을 수 있으며 `collector_warnings`에 이유가 남습니다.

## 실행 예시

```bash
export GHIDRA_INSTALL_DIR=/path/to/ghidra_12.0.4_PUBLIC
python3 benchmark/ghidra_oracle_benchmark/export_oracle.py \
  --manifest benchmark/ghidra_oracle_benchmark/examples/smoke_manifest.json \
  --ghidra-dir "$GHIDRA_INSTALL_DIR" \
  --out benchmark/artifacts/ghidra_oracle/export_smoke.json \
  --per-function-timeout-sec 180
```

합성 바이트만 (프로젝트 임시 파일은 `benchmark/artifacts/ghidra_oracle_micro/`, gitignore 됨):

```bash
python3 benchmark/ghidra_oracle_benchmark/export_oracle.py \
  --manifest benchmark/ghidra_oracle_benchmark/examples/micro_manifest.json \
  --ghidra-dir "$GHIDRA_INSTALL_DIR" \
  --out benchmark/artifacts/ghidra_oracle/export_micro.json
```

## 메모

- 초기 버전은 Grand Finale와 동일하게 레거시 `pyghidra.open_program()` 경로를 사용합니다 (파일 입력).
- 신형 `program_loader()` 경로는 합성 `hex_bytes` 입력에 사용합니다.
- 대규모 `call_targets` / 문자열 목록은 상한으로 잘립니다 ([`collectors.py`](collectors.py)).
