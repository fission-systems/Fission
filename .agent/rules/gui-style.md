---
trigger: glob
globs: ["crates/fission-ui/**/*.rs"]
---

# GUI 코드 작성 규칙

## 테마

- **항상 Catppuccin 팔레트 사용**
- `crate::ui::gui::theme::catppuccin::*` import
- 하드코딩된 Color32 금지

### 색상 사용 가이드

| 용도 | 색상 |
|------|------|
| 제목/강조 | `LAVENDER`, `MAUVE` |
| 버튼 | `BLUE`, `SAPPHIRE` |
| 성공 | `GREEN`, `TEAL` |
| 경고 | `YELLOW`, `PEACH` |
| 에러 | `RED`, `MAROON` |
| 텍스트 | `TEXT`, `SUBTEXT1` |
| 배경 | `BASE`, `SURFACE0` |

## 코드 하이라이팅

- `crate::ui::gui::theme::code::*` 사용
- 키워드: `code::KEYWORD`
- 함수: `code::FUNCTION`
- 문자열: `code::STRING`

## 패널 구조

```rust
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // 제목
    ui.heading(egui::RichText::new("패널").color(catppuccin::LAVENDER));
    
    ui.separator();
    
    // 내용
    egui::ScrollArea::vertical().show(ui, |ui| {
        // ...
    });
}
```

## 키보드 단축키

- 새 단축키 추가 시 `app/mod.rs`의 `handle_navigation_actions` 수정
- 기존 단축키와 충돌 확인
