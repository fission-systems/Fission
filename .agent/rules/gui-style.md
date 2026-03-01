---
trigger: glob
globs: ["crates/fission-tauri/**/*.tsx", "crates/fission-tauri/**/*.ts", "crates/fission-tauri/**/*.rs"]
---

# GUI 코드 작성 규칙 (Tauri 2.x + React 19)

GUI는 `crates/fission-tauri/` 아래의 Tauri 2.x + React 19 구조로 구성됩니다.
- 프론트엔드: `src/` (TypeScript + React)
- 백엔드: `src-tauri/src/` (Rust Tauri commands)

## 프론트엔드 (React/TypeScript)

### 테마

- **항상 CSS 변수 / Catppuccin Mocha 팔레트 사용**
- `src/theme/` 내 토큰 참조
- 하드코딩된 색상값 금지

### 색상 사용 가이드

| 용도 | CSS 변수 |
|------|----------|
| 제목/강조 | `--color-lavender`, `--color-mauve` |
| 버튼 | `--color-blue`, `--color-sapphire` |
| 성공 | `--color-green`, `--color-teal` |
| 경고 | `--color-yellow`, `--color-peach` |
| 에러 | `--color-red`, `--color-maroon` |
| 텍스트 | `--color-text`, `--color-subtext1` |
| 배경 | `--color-base`, `--color-surface0` |

### 패널 컴포넌트 구조

```tsx
// src/panels/sidebar/MyPanel.tsx
import { invoke } from "@tauri-apps/api/core";

interface Props {
  // props 정의
}

export function MyPanel({ ...props }: Props) {
  return (
    <div className="panel">
      <h2 className="panel-title">패널 제목</h2>
      <div className="panel-body">
        {/* 내용 */}
      </div>
    </div>
  );
}
```

### Tauri IPC 호출 패턴

```tsx
import { invoke } from "@tauri-apps/api/core";

// 커맨드 호출
const result = await invoke<ReturnType>("command_name", {
  arg1: value1,
});
```

## 백엔드 (Rust Tauri Commands)

### 커맨드 구조

- 커맨드는 `src-tauri/src/commands/` 에 기능별로 파일 분리
- 반드시 `#[tauri::command]` 어트리뷰트 추가
- 에러는 `crate::error::AppError` 반환

```rust
// src-tauri/src/commands/my_feature.rs
#[tauri::command]
pub async fn my_command(
    state: tauri::State<'_, AppState>,
    arg: String,
) -> Result<MyDto, String> {
    // 구현
}
```

- `lib.rs`의 `invoke_handler`에 커맨드 등록 필요

## 키보드 단축키

- `src/hooks/useKeyboard.ts` 또는 컴포넌트 내 `onKeyDown` 핸들러 사용
- 기존 단축키 충돌 확인
