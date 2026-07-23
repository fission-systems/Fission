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
- **CFG Viewer** — Rust-native SVG renderer with Sugiyama layered layout, pan/zoom, back-edge detection
- **Drag & Drop** binary loading (drop file into window)
- **Toggle sidebar** (Cmd+B) and **toggle bottom panel** (Cmd+J)
- **Bottom panel drag resize** — vertical resize handle

## Module Map

```
src/
├── main.rs                     App root, window config, global hotkeys, drag-resize state
│                               Cmd+B/J toggles, ondrop handler, bottom panel resize
├── state.rs                    AppState signal, FunctionKind, LogEntry, fuzzy_score
│                               sidebar_visible, bottom_panel_visible, bottom_panel_height
│                               current_cfg: Option<CfgGraphData>
├── engine.rs                   Blocking load/decompile helpers (spawn_blocking targets)
│                               CfgGraphData, CfgNodeData, CfgEdgeData, CfgEdgeKind
│                               CfgGraphData::from_evidence(evidence) builds CFG from pipeline telemetry
└── components/
    ├── mod.rs
    ├── title_bar.rs            Menu bar, File → Open Binary, rfd file dialog
    ├── sidebar.rs              Function list, use_memo filter, virtual scroll, on-click dispatch
    │                           run_decompile() is pub(crate) — shared by palette
    ├── editor.rs               Pseudocode/NIR/Hex tabs, line numbers, syntax tokens
    ├── cfg_view.rs             CFG viewer — Sugiyama layout, SVG rendering, pan/zoom
    ├── bottom_panel.rs         Output / CFG / Xrefs tab bar + content routing
    └── command_palette.rs      Cmd+K palette, fuzzy ranking, keyboard navigation
```

## Architecture

### Threading model

All Fission core APIs (`LoadedBinary::from_file`, `decompile_with_rust_sleigh_with_facts`) are **synchronous**.
The GUI bridges them to Dioxus's async executor via:

```rust
spawn(async move {
    let result = tokio::task::spawn_blocking(move || { /* core API */ }).await;
    // update Signal<AppState>
});
```

State updates use `Signal<AppState>` from a global context (`use_context`).  
Never call `.write()` from within a `spawn_blocking` closure — only from async context.

### CFG Data Flow

```
decompile_blocking()
  └─ decompile_with_rust_sleigh_with_facts()
       └─ RustSleighDecompileResult.evidence (RustSleighPipelineEvidence)
             └─ raw_pcode_blocks: Vec<PcodeBlockEvidence>
                  └─ CfgGraphData::from_evidence() ─→ state.current_cfg
                                                          └─ CfgView renders SVG
```

`PcodeBlockEvidence` contains: `start_address`, `successors: Vec<u32>`, `op_count`, `terminal_opcode`.
Back edges are detected via DFS (iterative stack to avoid stack overflow on large functions).
Edge kind is inferred from `terminal_opcode`: CBranch → True/False, Branch → Unconditional, etc.

### CFG Layout (Sugiyama)

`cfg_view.rs` implements a simplified Sugiyama layered layout:

1. **BFS layer assignment** — back edges are excluded to avoid cycles
2. **Barycenter crossing reduction** — 2 passes (top-down + bottom-up), uses snapshotted adjacency to avoid borrow conflicts
3. **Coordinate assignment** — fixed `NODE_W=160`, height ∝ op_count (44–80px)
4. **SVG generation** — pure Rust string builder, no `eval()` calls

All layout constants are at the top of `cfg_view.rs`:
`NODE_W`, `LAYER_GAP`, `NODE_GAP`, `SVG_PAD`, `NODE_H_BASE/PER_OP/MAX`, `BACK_OFFSET`

### State contracts

- `current_cfg` is always cleared when a new function is selected (before decompile starts).
- `sidebar_visible` / `bottom_panel_visible` drive conditional rendering in `main.rs`.
- `bottom_panel_height` is stored in state but controlled by a local `bottom_h` signal in `App`; state is the source of truth for persistence/restore.
- `run_decompile` is `pub(crate)` in `sidebar.rs` and is reused by `command_palette.rs`.

## Validation Rules

1. **Never call `.write()` from `spawn_blocking`** — deadlocks the Dioxus runtime.
2. **SVG is generated from typed Rust data** — no user-controlled strings pass through `dangerous_inner_html`.
3. **`current_cfg` must be `None` before decompile starts** — prevents stale CFG display.
4. **Layout functions are pure** — `compute_layout` and `render_svg` take `&CfgGraphData` and return owned values; no side effects.
5. **Signals in closures** — use `signal.set(val)` pattern (read first into local, then write) to avoid Dioxus borrow conflicts.

## Build / Check Commands

```bash
cargo check -p fission-dioxus
cargo build -p fission-dioxus
# Full dev run:
cargo run -p fission-dioxus
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| Cmd+K | Open command palette |
| Cmd+B | Toggle sidebar |
| Cmd+J | Toggle bottom panel |
| Escape | Close palette |
| Arrow Up/Down | Navigate palette results |
| Enter | Select palette result |

## Anti-Patterns

- Do not add semantic decompiler logic here — fix at the canonical owner in `fission-pcode` / `fission-decompiler`.
- Do not call blocking core APIs on the async executor thread — always use `spawn_blocking`.
- Do not generate SVG from user-supplied or binary-derived strings without sanitization — use typed data structures.
- Do not grow borrow-conflicting signal patterns — read into a local `let v = *sig.read()` before writing.
- Do not hardcode `/Users/sjkim1127/Fission/utils` paths in GUI code — use `PathConfig`.
