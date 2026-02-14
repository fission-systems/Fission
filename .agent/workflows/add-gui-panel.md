---
description: GUI 패널 추가 워크플로우
---

# Add GUI Panel Workflow

Fission GUI에 새 패널 추가 가이드

---

## 📋 기존 패널 구조

### 패널 위치

```
crates/fission-ui/src/ui/gui/panels/
├── assembly.rs        # 어셈블리 뷰
├── decompile.rs       # 디컴파일 뷰
├── functions.rs       # 함수 목록
├── side_bar.rs        # 사이드바
├── xrefs.rs           # XRefs
├── string_xrefs.rs    # 문자열 참조
├── settings.rs        # 설정
└── bottom_tabs/       # 하단 탭들
```

### 현재 패널 확인

// turbo

```bash
ls -la crates/fission-ui/src/ui/gui/panels/
```

---

## 🏗️ 1단계: 패널 파일 생성

`panels/new_panel.rs`:

```rust
//! 새 패널 - 설명

use crate::ui::gui::core::AppState;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// 새 패널 렌더링
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading(
        egui::RichText::new("🔍 새 패널")
            .color(catppuccin::LAVENDER)
    );
    
    ui.separator();
    
    // 패널 내용
    egui::ScrollArea::vertical().show(ui, |ui| {
        // 내용 렌더링
    });
}
```

---

## 📦 2단계: 모듈 등록

`panels/mod.rs`에 추가:

```rust
mod new_panel;
pub use new_panel::render as render_new_panel;
```

---

## 🔗 3단계: 앱에 통합

### 사이드 패널인 경우

`app/mod.rs`에서 호출:

```rust
panels::render_new_panel(ui, &mut self.state);
```

### 하단 탭인 경우

1. `core/state.rs`에 탭 추가:

```rust
pub enum BottomTab {
    // 기존 탭...
    NewPanel,
}
```

1. `panels/bottom_tabs/mod.rs`에 추가:

```rust
mod new_panel;

// match 문에 추가:
BottomTab::NewPanel => {
    new_panel::render(ui, state);
}
```

---

## 🎨 4단계: 스타일링

### 테마 색상 사용

```rust
use crate::ui::gui::theme::catppuccin;

// 제목
egui::RichText::new("제목").color(catppuccin::MAUVE)

// 버튼
if ui.button(
    egui::RichText::new("버튼").color(catppuccin::BLUE)
).clicked() {
    // 액션
}
```

### 코드 하이라이팅

```rust
use crate::ui::gui::theme::code;

egui::RichText::new("keyword").color(code::KEYWORD)
egui::RichText::new("function").color(code::FUNCTION)
```

---

## ⌨️ 5단계: 키보드 단축키 (선택)

`app/mod.rs`의 `handle_navigation_actions`에 추가:

```rust
// 새 단축키
if ctx.input(|i| i.key_pressed(egui::Key::SomeKey)) {
    // 액션
}
```

---

## 🔨 6단계: 빌드 및 테스트

### 빌드

// turbo

```bash
cargo build -p fission-ui
```

### GUI 실행

// turbo

```bash
cargo run -p fission-cli -- --gui
```

---

## 📝 7단계: 문서화

`docs/FEATURES.md`에 패널 추가:

```markdown
| 새 패널 | 설명 | `new_panel.rs` |
```
