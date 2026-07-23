# Fission Dioxus GUI Guide

Generated: 2026-07-23
Scope: `crates/fission-dioxus/`

## Overview

`fission-dioxus` is the pure-Rust desktop GUI for Fission, built with [Dioxus 0.6](https://dioxuslabs.com) (desktop/WRY backend).
It is a **presentation and orchestration layer only** — it must not own semantic decompiler repair, NIR/HIR normalization, or binary analysis logic.

The GUI surfaces:
- Binary load and function discovery (via `fission-loader`)
- Single-function decompilation on demand (via `fission-decompiler` + `fission-static`)
- Pseudocode / NIR / Hex views with syntax highlighting and line numbers
- Command palette (Cmd+K) with fuzzy function search
- Import stub and import thunk classification and display
- Real-time log panel with INFO / WARN / ERROR levels

## Module Map

```
src/
├── main.rs                     App root, window config, global hotkeys, drag-resize state
├── state.rs                    AppState signal, FunctionKind, LogEntry, fuzzy_score
├── engine.rs                   Blocking load/decompile helpers (spawn_blocking targets)
└── components/
    ├── mod.rs
    ├── title_bar.rs            Menu bar, File → Open Binary, rfd file dialog
    ├── sidebar.rs              Function list, use_memo filter, virtual scroll, on-click dispatch
    ├── editor.rs               Pseudocode/NIR/Hex tabs, line numbers, syntax tokens
    ├── bottom_panel.rs         Log output panel, CFG tab placeholder
    └── command_palette.rs      Cmd+K palette, fuzzy ranking, keyboard navigation
```

## Architecture

### Threading model

All Fission core APIs (`LoadedBinary::from_file`, `decompile_with_rust_sleigh_with_facts`) are **synchronous**.
The GUI bridges them to Dioxus's async executor via:

```
UI event -> spawn(async) -> tokio::task::spawn_blocking -> core API -> Signal update
```

Components must never call blocking APIs directly from event handlers or render paths.

### State contract

All shared UI state lives in a single `AppState` struct managed as a Dioxus `Signal<AppState>` provided through context.

| Field | Type | Owner | Purpose |
|---|---|---|---|
| `binary` | `Option<Arc<LoadedBinary>>` | `title_bar` (write), all (read) | Loaded binary, cheaply cloned |
| `functions` | `Vec<FunctionInfo>` | `title_bar` (write), `sidebar` (read) | Flat sorted function list |
| `current_function_addr` | `Option<u64>` | `sidebar`, `command_palette` | Selected function |
| `current_function_kind` | `FunctionKind` | `sidebar`, `command_palette` | Code / Import / Thunk classification |
| `decompiled_code` | `Option<String>` | `sidebar`, `command_palette` | Pseudocode output |
| `decompiled_nir` | `Option<String>` | `sidebar`, `command_palette` | NIR surface output |
| `is_loading_binary` | `bool` | `title_bar` | Loading spinner guard |
| `is_decompiling` | `bool` | `sidebar`, `command_palette` | Decompiling spinner guard |
| `log_entries` | `Vec<LogEntry>` | all writers | Bounded at 500 lines |
| `is_palette_open` | `bool` | `main`, `command_palette` | Palette visibility |
| `palette_query` | `String` | `command_palette` | Fuzzy search input |
| `palette_focused` | `usize` | `command_palette` | Keyboard-focused result index |

Do not add new parallel program-fact maps here; binary metadata belongs in `fission-analysis-db` upstream.

### FunctionKind classification

Before any decompile is triggered, the clicked/selected function is classified:

```rust
FunctionKind::Import { library }  // is_import && !is_thunk_like — no body, show info panel
FunctionKind::Thunk  { target }   // is_thunk_like — JMP stub, warn + decompile
FunctionKind::Code                // regular function — decompile normally
```

This prevents the GUI from naively decompiling import stubs and displaying misleading
self-recursive pseudocode (the IAT symbol carries the same name as the stub, making
the output appear recursive — see `sqlite3_log` pattern).

### Virtual scroll

The function list uses a wheel-delta virtual list with `content-visibility: auto` on each item:

- `ITEM_H = 32.0 px`, `VISIBLE_H = 680.0 px`, `OVERSCAN = 4`
- Top and bottom `<li>` spacers maintain the total scrollable height
- `use_memo` memoises the filtered slice — recomputed only when `sidebar_search` or `functions` changes

### Command palette (Cmd+K)

- `fuzzy_score(query, target)` — consecutive-char bonus + word-boundary bonus + prefix bonus
- Results are sorted descending by score, truncated to 18
- Keyboard: `↑ ↓` move `palette_focused`; `Enter` selects and closes; `Escape` cancels
- Backdrop: `backdrop-filter: blur(6px)` with `@keyframes palette-card-in` entrance animation
- The palette reuses the same `FunctionKind` dispatch as the sidebar — no duplicate decompile logic

## Ownership Boundaries

| Area | Owned by | Must not |
|---|---|---|
| Binary parsing, symbol resolution | `fission-loader` | Patch in GUI layer |
| Decompile semantics, NIR/HIR | `fission-pcode`, `fission-decompiler` | Patch in printer or GUI |
| Static facts, FactStore | `fission-static` | Duplicate in GUI state |
| Analysis DB / typed metadata | `fission-analysis-db` | Re-implement in AppState |
| Syntax highlighting | `editor.rs` (`highlight_line`) | One Dark Pro token colours; no semantic meaning |
| Fuzzy search ranking | `state.rs` (`fuzzy_score`) | No external dependencies; keep O(n × m) |

## CSS Design System

Design tokens are declared in `assets/style.css` under `:root`.

| Token family | Prefix | Purpose |
|---|---|---|
| Surface scale | `--sf-{0..popup}` | Background layers, darkest to popup |
| Ink scale | `--ink-{0..4}` | Text hierarchy |
| Accent | `--accent`, `--accent-dim`, `--accent-border` | Primary interactive colour |
| Semantic | `--clr-{green,red,yellow,purple,orange,cyan}` | Status and badge colours |
| Border | `--bdr-{faint,default,strong}` | Border hierarchy |
| Easing | `--ease-out`, `--ease-in`, `--t-{fast,base,slow}` | Consistent motion |

Advanced CSS features in use:

- `@container sidebar` — sidebar adapts to its own width (narrow: hide addresses and badges)
- `content-visibility: auto` + `contain-intrinsic-size` — offscreen item paint skip
- `animation-timeline: view()` — scroll-driven item entrance (guarded with `@supports`)
- `backdrop-filter: blur` — command palette backdrop
- `@keyframes` with `cubic-bezier` easing — palette card entrance, shimmer skeleton loader

## Rules

1. Keep all semantic decompiler logic in the canonical owner (`fission-pcode`, `fission-decompiler`). The GUI must not repair IR, rewrite pseudocode tokens, or hide decompiler failures by substituting alternative output.
2. Classify functions before decompiling — never trigger `decompile_blocking` on `FunctionKind::Import`.
3. All blocking calls must go through `tokio::task::spawn_blocking`; never block the Dioxus render thread.
4. `AppState` is the single source of truth; do not shadow it with component-local caches that diverge.
5. The log panel is the primary diagnostic surface; push INFO/WARN/ERROR entries for every async operation.
6. Syntax highlighting in `editor.rs` is presentation-only — token colours carry no semantic information.
7. The command palette fuzzy scorer (`fuzzy_score`) must stay self-contained in `state.rs` with no external crate dependency.
8. CSS design tokens in `:root` are the single source of colour and spacing; do not hardcode values in component styles.

## Anti-patterns

- Do not call `LoadedBinary::from_file` or `decompile_with_rust_sleigh_with_facts` from synchronous event handlers.
- Do not add decompiler quality heuristics (goto removal, type inference, etc.) in the GUI rendering path.
- Do not grow `AppState` with fields that duplicate information already in `LoadedBinary` or `FunctionInfo`.
- Do not use `dangerous_inner_html` for user-controlled strings; only use it for highlighter output from trusted binary-derived data.
- Do not add ISA-specific branches in the GUI (e.g. gating on `is_x86`); ISA differences belong in cspec/CC/SLEIGH.
- Do not cache or persist decompile output between application launches; the GUI is stateless across sessions.

## Validation

```bash
# Type-check (fastest — run after every change)
cargo check -p fission-dioxus

# Launch the GUI for manual testing
cargo run -p fission-dioxus

# Release build
cargo build -p fission-dioxus --release
```

Manual verification checklist:

- Open a PE or ELF binary via File → Open Binary
- Sidebar shows function list with correct IMP / EXP / THUNK badges
- Sidebar search box filters the list in real-time (no re-render lag on large binaries)
- Clicking a regular function shows pseudocode with line numbers and syntax colouring
- Clicking an import thunk shows the yellow warning banner above the (self-recursive) code
- Clicking an import stub shows the cyan info panel and does NOT call the decompiler
- Cmd+K opens the command palette; fuzzy search ranks prefix and consecutive matches higher
- Arrow keys navigate palette results; Enter selects and closes; Escape cancels
- Sidebar drag handle resizes the panel; `@container` CSS adapts content at narrow widths
- Log panel shows INFO / WARN / ERROR entries for each load and decompile operation
- Hex tab shows raw dump with offset + hex + ASCII columns; NIR tab falls back to pseudocode when absent
