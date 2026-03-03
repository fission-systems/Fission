# Security Advisory Policy (Rust/Node)

Last updated: 2026-03-03

## 목적

본 문서는 Fission의 의존성 보안 점검 운영 기준을 정의합니다.

- Rust: `cargo deny check advisories`
- Node (Tauri frontend): `npm audit --audit-level=high`

## 현재 상태 요약

- Rollup는 `crates/fission-tauri/package.json`의 `overrides`로 `4.59.0`에 고정합니다.
- Rust는 `deny.toml`의 `advisories.ignore`에 **no-fix 생태계 이슈**를 임시 기준선으로 관리합니다.

## 왜 ignore 기준선이 필요한가

Tauri Linux 런타임 경로에서 GTK3/WebKit 계열(예: `gtk`, `gdk`, `webkit2gtk`)의
upstream 유지보수 종료(no safe upgrade) advisory가 존재합니다.

이 이슈들은 즉시 패치 가능한 안전 버전이 없어, 다음 원칙으로 운영합니다.

1. CI에서 advisory 점검은 **항상 실행**한다.
2. `deny.toml`의 ignore 목록은 문서화된 no-fix 항목으로만 제한한다.
3. 신규 advisory는 CI를 실패시켜 triage를 강제한다.
4. 분기마다(또는 tauri/wry major 업데이트 시) ignore 목록 재검토를 수행한다.

## 운영 원칙

- ignore 추가 조건
  - `cargo deny` 결과에 `Solution: No safe upgrade is available!`가 명시된 경우
  - 또는 upstream migration(예: GTK4 전환) 없이는 해소 불가한 구조적 이슈인 경우
- ignore 제거 조건
  - 안전 업그레이드 버전이 릴리즈된 경우
  - 대체 dependency/런타임으로 전환 완료된 경우

## Linux 타깃 분리 전략 (중기)

1. Tauri Linux 체인(gtk3/webkit2gtk)과 Core CLI/Analysis 체인을 분리 점검
2. Core 경로는 advisory zero-tolerance 유지
3. GUI 경로는 migration plan(GTK4/대체 런타임) 완료 전까지 기준선 + 정기 재검토

## 점검 명령

```bash
# Rust advisory
cargo deny check advisories

# Node advisory
cd crates/fission-tauri
npm ci --ignore-scripts
npm audit --audit-level=high
```

## 관련 파일

- `deny.toml`
- `.github/workflows/ci.yml`
- `crates/fission-tauri/package.json`
