---
description: GUI 패널 추가 워크플로우
---

# Add GUI Panel Workflow

Fission GUI (Tauri 2.x + React 19)에 새 패널 추가 가이드

---

## 📋 기존 패널 구조

### 프론트엔드 패널 위치

```
crates/fission-tauri/src/panels/
├── sidebar/                # 사이드바 패널
│   ├── FunctionsList.tsx
│   ├── SearchPanel.tsx
│   ├── SettingsPanel.tsx
│   ├── PluginsPanel.tsx
│   ├── SectionsPanel.tsx
│   ├── DebugSidebar.tsx
│   └── index.ts
├── editor/                 # 에디터 뷰 (메인 영역)
│   ├── AssemblyView.tsx
│   ├── DecompileView.tsx
│   ├── HexView.tsx
│   ├── ListingView.tsx
│   └── index.ts
└── bottom/                 # 하단 탭 패널
    ├── XrefsPanel.tsx
    ├── StringXrefsPanel.tsx
    ├── CfgPanel.tsx
    ├── ExportsPanel.tsx
    ├── PatchesPanel.tsx
    ├── NotesPanel.tsx
    ├── DebugTab.tsx
    ├── TimelinePanel.tsx
    └── index.ts
```

### 백엔드 커맨드 위치

```
crates/fission-tauri/src-tauri/src/commands/
```

### 현재 패널 확인

// turbo

```bash
ls -la crates/fission-tauri/src/panels/
```

---

## 🏗️ 1단계: React 패널 컴포넌트 생성

`src/panels/<위치>/NewPanel.tsx`:

```tsx
//! 새 패널 - 설명
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface Props {
  // 필요한 props
}

export function NewPanel({ ...props }: Props) {
  const [data, setData] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("new_panel_command").then(setData).catch(console.error);
  }, []);

  return (
    <div className="panel">
      <h2 className="panel-title" style={{ color: "var(--color-lavender)" }}>
        🔍 새 패널
      </h2>
      <div className="panel-body">
        {data ?? "로딩 중..."}
      </div>
    </div>
  );
}
```

---

## 📦 2단계: index.ts에 등록

`src/panels/<위치>/index.ts`에 추가:

```ts
export { NewPanel } from "./NewPanel";
```

---

## 🔗 3단계: Tauri 백엔드 커맨드 추가 (필요한 경우)

`src-tauri/src/commands/new_feature.rs` 생성:

```rust
use crate::state::AppState;

#[tauri::command]
pub async fn new_panel_command(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // 구현
    Ok("결과".to_string())
}
```

`src-tauri/src/lib.rs`의 `invoke_handler`에 등록:

```rust
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        // 기존 커맨드들...
        commands::new_feature::new_panel_command,
    ])
```

---

## 🎨 4단계: 스타일링

Catppuccin Mocha CSS 변수 사용:

```tsx
// 제목
<h2 style={{ color: "var(--color-mauve)" }}>제목</h2>

// 버튼
<button style={{ backgroundColor: "var(--color-blue)" }}>
  버튼
</button>

// 에러 상태
<span style={{ color: "var(--color-red)" }}>에러</span>
```

---

## ⌨️ 5단계: 키보드 단축키 (선택)

`src/hooks/useKeyboard.ts` 또는 컴포넌트의 `onKeyDown` 핸들러에 추가:

```tsx
useEffect(() => {
  const handler = (e: KeyboardEvent) => {
    if (e.key === "P" && e.ctrlKey) {
      // 액션
    }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, []);
```

---

## 🔨 6단계: 빌드 및 테스트

### 개발 서버 실행

// turbo

```bash
cd crates/fission-tauri
npm run tauri dev
```

### 프로덕션 빌드

// turbo

```bash
cd crates/fission-tauri
npm run tauri build
```

---

## 📝 7단계: 문서화

`docs/FEATURES.md`에 패널 추가:

```markdown
| 새 패널 | 설명 | `NewPanel.tsx` |
```
