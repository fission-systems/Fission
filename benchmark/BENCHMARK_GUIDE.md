# 📊 성능 벤치마킹 대시보드 가이드

## 개요

Fission은 **criterion.rs** 기반의 종합적인 성능 벤치마킹 시스템을 제공합니다.

- ✅ **자동 회귀 감지**: 성능 저하 자동 감지 및 PR 댓글
- 📈 **히스토리 추적**: 각 커밋의 성능 변화 추적
- 🎯 **다양한 바이너리**: 샘플 PE/ELF 바이너리로 실제 로딩 성능 테스트
- 📊 **그래프 생성**: ASCII 및 HTML 기반 성능 추이 시각화

---

## 🚀 빠른 시작

### 로컬에서 벤치마크 실행

```bash
# 전체 벤치마크 실행
cargo bench -p fission-analysis --bench benchmark

# 특정 벤치마크만 실행
cargo bench -p fission-analysis --bench benchmark -- cfg_analysis

# 기준점 저장 (baseline 생성)
cargo bench -p fission-analysis --bench benchmark -- --save-baseline main
```

### 결과 비교

```bash
# 현재 결과를 main 기준점과 비교
cargo bench -p fission-analysis --bench benchmark -- --baseline main
```

---

## 📁 디렉토리 구조

```
benchmark/
├── binary/                    # 벤치마크용 바이너리
│   └── x86-64/
│       └── window/           # Windows PE 바이너리
│           ├── small/        # 작은 크기 바이너리
│           ├── medium/       # 중간 크기 바이너리
│           ├── large/        # 큰 크기 바이너리
│           └── commercial_binary/
├── results/                   # 벤치마크 결과
│   └── current_*.txt         # 현재 실행 결과
└── history/                   # 성능 히스토리
    ├── timeline.jsonl        # 시간별 기록
    └── *.json                # 각 커밋의 결과
```

---

## 🧪 벤치마크 종류

### 1. CFG 분석 (cfg_analysis)

제어 흐름 그래프 분석 성능을 테스트합니다.

| 테스트 | 설명 | 예상 시간 |
|--------|------|----------|
| `cfg_analysis_16` | 16개 블록 다이아몬드 CFG | < 1ms |
| `cfg_analysis_64` | 64개 블록 다이아몬드 CFG | 1-5ms |
| `cfg_analysis_256` | 256개 블록 다이아몬드 CFG | 5-20ms |
| `complex_d2_b4` | 깊이 2, 브랜치 4 복잡 CFG | 1-3ms |
| `complex_d3_b4` | 깊이 3, 브랜치 4 복잡 CFG | 2-8ms |
| `complex_d4_b2` | 깊이 4, 브랜치 2 복잡 CFG | 3-10ms |

**예상 성능 기준:**
```
cfg_analysis_64:     time: [2.5 ms +/- 0.2 ms]
cfg_analysis_256:    time: [12.3 ms +/- 0.8 ms]
complex_d3_b4:       time: [5.2 ms +/- 0.4 ms]
```

### 2. 최적화 엔진 (optimizer)

C 코드 최적화 성능을 테스트합니다.

| 테스트 | 설명 | 예상 시간 |
|--------|------|----------|
| `simple_optimization` | 간단한 최적화 | < 0.5ms |
| `complex_optimization` | 복잡한 루프 최적화 | 1-3ms |

### 3. 바이너리 로딩 (binary_loading)

다양한 크기의 PE 바이너리 로딩 성능을 테스트합니다.

**테스트 대상:**
- `small_*.exe`: 작은 PE 바이너리 (< 1MB)
- `medium_*.exe`: 중간 크기 PE 바이너리 (1-10MB)
- `large_*.exe`: 큰 PE 바이너리 (> 10MB)
- `commercial_*`: 실제 상용 바이너리 (선택사항)

---

## 📊 CI/CD 통합

### 자동 실행

벤치마크는 다음 상황에서 자동으로 실행됩니다:

1. **Pull Request**: PR 제출 시 자동 실행
2. **Main 브랜치**: 매 커밋마다 자동 실행
3. **Nightly**: 매일 밤 종합 벤치마크

### PR 댓글 자동 생성

성능 비교 결과가 PR에 자동으로 댓글로 추가됩니다:

```
## 📊 Performance Benchmark Results

**Commit:** `a1b2c3d`
**Timestamp:** 2026-04-21T14:30:00 UTC

### CFG_ANALYSIS

| Benchmark | Current | Baseline | Change | Status |
|-----------|---------|----------|--------|--------|
| cfg_analysis_64 | 2.5 ms | 2.4 ms | +4.2% | ✓ |
| cfg_analysis_256 | 12.3 ms | 11.8 ms | +4.2% | ✓ |

### OPTIMIZER

| Benchmark | Current | Baseline | Change | Status |
|-----------|---------|----------|--------|--------|
| simple_optimization | 0.3 ms | 0.3 ms | +1.2% | ✓ |

### BINARY_LOADING

| Benchmark | Current | Baseline | Change | Status |
|-----------|---------|----------|--------|--------|
| small_app.exe | 45.2 ms | 44.8 ms | +0.9% | ✓ |

⚠️ **Performance Regressions**
- (없음)

✅ **Performance Improvements**
- complex_d3_b4: -2.1%
```

### 회귀 감지 & 알림

**회귀 기준:**
- 🔴 5% 이상: 성능 저하 (Critical)
- 🟡 2-5% 사이: 경미한 저하 (Warning)
- 🟢 -5% 이상: 성능 개선 (Good)

**알림:**
- Slack 메시지 (선택사항)
- GitHub 리뷰 자동 요청
- Issue 자동 생성 (Critical 회귀만)

---

## 🔧 성능 분석 스크립트

### 벤치마크 분석

```bash
python3 scripts/benchmark/analyze_benchmark.py \
    --current benchmark/results/current_abc123.txt \
    --baseline benchmark/history/main_latest.json \
    --commit abc123 \
    --output benchmark/results/report.md
```

**옵션:**
- `--current`: 현재 벤치마크 결과 파일
- `--baseline`: 기준점 파일 (생략 시 main 최신 사용)
- `--commit`: 커밋 해시
- `--history-dir`: 히스토리 디렉토리 (기본: `benchmark/history`)
- `--output`: 출력 리포트 파일

**출력:**
- 마크다운 리포트 생성
- 성능 비교 테이블
- 회귀/개선 사항 강조

### 히스토리 업데이트

```bash
python3 scripts/benchmark/update_history.py \
    --result benchmark/results/current_abc123.txt \
    --commit abc123 \
    --branch main \
    --report-file benchmark/results/history_report.md
```

**옵션:**
- `--result`: 벤치마크 결과 파일
- `--commit`: 커밋 해시
- `--branch`: 브랜치 이름
- `--timestamp`: 타임스탬프 (기본: 현재 시간)
- `--history-dir`: 히스토리 저장 위치
- `--report-file`: 리포트 파일 출력

**생성물:**
- `benchmark/history/timeline.jsonl`: 시간 기반 모든 결과
- `benchmark/history/main_latest.json`: main 브랜치 최신 결과
- ASCII 기반 성능 추이 그래프

---

## 📈 성능 해석 가이드

### 정상 범위

```
cfg_analysis_64:  ±2-3% 변동은 정상
  ✓ 2.4 ms (baseline)
  ✓ 2.5 ms (current) → 2% 증가
  ⚠️  2.6 ms (current) → 8% 증가 (조사 필요)
```

### 회귀 조사 체크리스트

성능 저하가 감지되었을 때:

1. **로컬에서 재현**
   ```bash
   git checkout <problematic-commit>
   cargo bench -p fission-analysis --bench benchmark -- --baseline main
   ```

2. **원인 파악**
   - 알고리즘 변경: 코드 리뷰
   - 의존성 업데이트: Cargo.lock 확인
   - 컴파일 플래그: `Cargo.toml` 프로필 확인

3. **성능 프로파일링**
   ```bash
   cargo install flamegraph
   cargo flamegraph -p fission-analysis --bin benchmark
   ```

4. **최적화**
   - 핫스팟 식별 및 최적화
   - 알고리즘 개선
   - 캐싱 추가

---

## 🎯 권장 성능 기준

### CFG 분석 (제어 흐름 그래프)

```
Small (< 64 blocks):    < 5 ms
Medium (64-256 blocks): < 20 ms
Large (> 256 blocks):   < 100 ms
```

### 바이너리 로딩

```
Small PE (< 1MB):    < 100 ms
Medium PE (1-10MB):  < 500 ms
Large PE (> 10MB):   < 2000 ms
```

### 최적화 엔진

```
Simple patterns:     < 1 ms
Complex patterns:    < 5 ms
```

---

## 🚨 트러블슈팅

### 벤치마크 파일 찾을 수 없음

```
Error: binary_loading benchmark failed: No such file
```

**해결:**
```bash
# benchmark/binary/x86-64/window/ 디렉토리 존재 확인
ls -la benchmark/binary/x86-64/window/small/

# 없으면 생성
mkdir -p benchmark/binary/x86-64/window/{small,medium,large,commercial_binary}

# Git LFS로 추적되는 바이너리 풀다운
git lfs pull
```

### 불안정한 결과

벤치마크 결과가 매번 다른 경우:

```bash
# 샘플 수 증가
cargo bench -p fission-analysis --bench benchmark -- --sample-size 200

# 워밍업 실행 횟수 증가
cargo bench -p fission-analysis --bench benchmark -- --warm-up-time 3
```

### 메모리 부족

큰 바이너리 벤치마크 시 메모리 부족:

```bash
# 샘플 수 감소
cargo bench -p fission-analysis --bench benchmark -- --sample-size 20

# 한 번에 하나씩만 실행
cargo bench -p fission-analysis --bench benchmark -- small_
```

---

## 📚 추가 리소스

- [criterion.rs 문서](https://bheisler.github.io/criterion.rs/book/)
- [Rust 성능 튜닝](https://nnethercote.github.io/perf-book/)
- [Fission 벤치마크 히스토리](./history/)
- [성능 분석 스크립트](../scripts/benchmark/)
