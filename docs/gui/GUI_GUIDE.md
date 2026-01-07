# GUI User Guide

## Overview

Fission features a modern, VS Code-inspired graphical interface built with egui. This guide covers all GUI features, keyboard shortcuts, and workflows.

**Interface Highlights:**
- 🎨 **Catppuccin Theme** - Easy on the eyes with multiple color variants
- 📑 **Tabbed Editor** - Multiple files and views in tabs
- 🔍 **Function Explorer** - Browse and search functions
- 🐛 **Integrated Debugger** - Debug panel with registers, memory, and call stack
- ⚡ **Fast Navigation** - Keyboard shortcuts for everything

---

## Table of Contents

- [Interface Layout](#interface-layout)
- [Getting Started](#getting-started)
- [Activity Bar](#activity-bar)
- [Side Bar](#side-bar)
- [Editor Area](#editor-area)
- [Bottom Panel](#bottom-panel)
- [Status Bar](#status-bar)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Workflows](#workflows)
- [Themes](#themes)
- [Settings](#settings)

---

## Interface Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  Menu Bar (File, Edit, View, Debug, Help)                           │
├──┬────────────────────────────────────────────────────────────┬─────┤
│  │  Editor Tabs                                                │     │
│  ├────────────────────────────────────────────────────────────┤     │
│ A│  ┌──────────────────────────────────────────────────┐      │     │
│ c│  │                                                  │      │     │
│ t│  │         Assembly / Decompiled View               │      │  S  │
│ i│  │                                                  │      │  c  │
│ v│  │                                                  │      │  r  │
│ i│  │                                                  │      │  o  │
│ t│  │                                                  │      │  l  │
│ y│  │                                                  │      │  l  │
│  │  └──────────────────────────────────────────────────┘      │     │
│ B│                                                            │  B  │
│ a│                                                            │  a  │
│ r│                                                            │  r  │
├──┼────────────────────────────────────────────────────────────┼─────┤
│  │  Bottom Panel (Console, Debug, Hex View, Timeline)        │     │
├──┴────────────────────────────────────────────────────────────┴─────┤
│  Status Bar (Binary info, memory usage, line numbers)               │
└─────────────────────────────────────────────────────────────────────┘
```

### Component Overview

| Component | Location | Purpose |
|-----------|----------|---------|
| **Menu Bar** | Top | File operations, view toggles, debug controls |
| **Activity Bar** | Left edge | Switch between Explorer, Search, Debug, Plugins, Settings |
| **Side Bar** | Left panel | Content based on active activity |
| **Editor Area** | Center | Tabbed views for assembly, decompiled code |
| **Bottom Panel** | Bottom | Console output, debug info, hex view, timeline |
| **Status Bar** | Very bottom | Binary info, performance metrics |

---

## Getting Started

### Opening a Binary

**Method 1: File Menu**
1. Click **File → Open Binary** (or `Ctrl+O` / `Cmd+O`)
2. Select a PE, ELF, or Mach-O binary
3. Wait for analysis to complete

**Method 2: Drag and Drop**
1. Drag binary file from file explorer
2. Drop onto Fission window
3. Analysis starts automatically

**Method 3: Recent Files**
1. Click **File → Recent**
2. Select from list of recently opened files

### Initial Analysis

After loading, Fission automatically:
- ✅ Parses binary format
- ✅ Discovers functions (entry point, exports, imports)
- ✅ Identifies sections
- ✅ Extracts strings

**Progress is shown in:**
- Console panel (detailed logs)
- Status bar (current operation)

---

## Activity Bar

Located on the far left, switches between different modes.

### Explorer (📁)

**Default view** - File and function explorer

**Features:**
- Function list with search/filter
- Import/export tables
- Section browser
- String view

**Shortcut:** `Ctrl+Shift+E` / `Cmd+Shift+E`

### Search (🔍)

**Global search** across all functions and data

**Features:**
- Text search in disassembly
- Regex support
- String search
- Cross-reference lookup

**Shortcut:** `Ctrl+Shift+F` / `Cmd+Shift+F`

### Debug (▶️)

**Debug control panel**

**Features:**
- Attach to process
- Breakpoint list
- Debug controls (Continue, Step Over, Step Into)
- Register view

**Shortcut:** `Ctrl+Shift+D` / `Cmd+Shift+D`

### Plugins (🧩)

**Plugin management**

**Features:**
- Installed plugins list
- Enable/disable plugins
- Plugin settings

**Shortcut:** `Ctrl+Shift+X` / `Cmd+Shift+X`

### Settings (⚙️)

**Application settings**

**Features:**
- Theme selection
- Decompiler configuration
- Font size adjustment
- Keyboard shortcuts

**Shortcut:** `Ctrl+,` / `Cmd+,`

---

## Side Bar

Content changes based on active activity.

### Explorer Mode

#### Function List

**Display:**
```
FUNCTIONS (114)
┌─────────────────────────┐
│ 🔍 Search...            │
├─────────────────────────┤
│ ▼ Entry Points          │
│   • 0x140001400  main   │
│ ▼ Imports (87)          │
│   • CreateFileA         │
│   • VirtualAlloc        │
│   • HeapAlloc           │
│ ▼ Exports (0)           │
│ ▼ Internal (27)         │
│   • 0x140001500         │
│   • 0x140001600         │
└─────────────────────────┘
```

**Actions:**
- **Click function** → Opens assembly view
- **Right-click** → Context menu (Rename, Add comment, etc.)
- **Double-click** → Opens decompiled view

#### Search Bar

**Filter functions by:**
- Name (e.g., `CreateFile`)
- Address (e.g., `0x140001000`)
- Type (entry, import, export, internal)

**Example searches:**
```
Virtual      → Shows VirtualAlloc, VirtualFree, etc.
0x14000      → Functions starting with 0x14000...
import:      → Filter imports only
```

### Debug Mode

#### Breakpoint List

```
BREAKPOINTS
┌─────────────────────────┐
│ ✓ 0x140001400  main     │
│ ✓ 0x140001500  sub_500  │
│ ✗ 0x140001600  (disabled)│
└─────────────────────────┘
```

**Actions:**
- **Click** → Jump to address
- **Checkbox** → Enable/disable
- **X button** → Remove

#### Process Attach

```
ATTACH TO PROCESS
┌─────────────────────────┐
│ 🔍 Search process...    │
├─────────────────────────┤
│ notepad.exe [1234]      │
│ chrome.exe [5678]       │
│ explorer.exe [9012]     │
└─────────────────────────┘
```

---

## Editor Area

Central workspace with tabbed interface.

### Tabs

**Tab Types:**
- 📄 **Assembly** - Disassembled instructions
- 🔤 **Decompiled** - High-level C-like code
- 📊 **Hex View** - Raw bytes with ASCII
- 📈 **Graph** - Control flow graph (future)

**Tab Controls:**
- **Click tab** → Switch to that view
- **× button** → Close tab
- **Drag tab** → Reorder (future)

### Assembly View

**Display:**
```
┌────────────────────────────────────────────────────┐
│ 0x140001400                                        │
│   push    rbp                                      │
│   mov     rbp, rsp                                 │
│   sub     rsp, 0x20                                │
│   mov     qword [rbp-0x10], rcx                    │
│   mov     dword [rbp-0x14], edx                    │
│   mov     rax, qword [rbp-0x10]                    │
│   mov     eax, dword [rax+0x10]                    │
│   add     eax, dword [rbp-0x14]                    │
│   mov     dword [rcx+0x10], eax                    │
│   leave                                            │
│   ret                                              │
└────────────────────────────────────────────────────┘
```

**Features:**
- **Syntax highlighting** - Opcodes, registers, addresses
- **Address column** - Click to jump
- **Cross-references** - Hover shows xrefs
- **Comments** - Inline and side comments

**Navigation:**
- **Click address** → Jump to location
- **Right-click** → Add comment, Set breakpoint, Follow reference
- **Double-click constant** → Show in hex view

### Decompiled View

**Display:**
```c
int process_item(Item* item, int value) {
    if (item == NULL) {
        return -1;
    }
    
    item->id = value;
    item->point.x += 10;
    item->point.y += 20;
    
    return 0;
}
```

**Features:**
- **C syntax highlighting**
- **Structure recovery** - Detected from access patterns
- **Constant substitution** - `PAGE_EXECUTE_READWRITE` instead of `0x40`
- **Type inference** - Function parameters and return types

**Context Menu:**
- **Rename variable** - Change local variable names
- **Change type** - Override inferred types
- **Add comment** - Annotate code
- **Copy as** → C, Rust, Python

### Split View (Future)

Side-by-side assembly and decompiled code:
```
┌──────────────────┬──────────────────┐
│   Assembly       │   Decompiled     │
│                  │                  │
│ 0x140001400:     │ int main() {     │
│   push rbp       │   Item* item;    │
│   mov rbp, rsp   │   item = malloc( │
│   ...            │       sizeof(...) │
└──────────────────┴──────────────────┘
```

---

## Bottom Panel

Collapsible panel at the bottom for auxiliary information.

### Console Tab

**Output:**
```
[*] Loading 'test.exe'...
[✓] Binary loaded successfully
  Format: PE (binrw)
  Architecture: 64-bit
  Entry Point: 0x140001400
  Functions: 114
[*] Decompiling function at 0x140001400...
[✓] Decompilation complete (234ms)
```

**Features:**
- Color-coded messages (info, warning, error)
- Timestamps (optional)
- Filter by level
- Clear button

### Debug Tab

**Registers:**
```
RAX: 0x0000000000000001    R8:  0x0000000000000000
RBX: 0x00007FF7A0001000    R9:  0x0000000000000000
RCX: 0x0000000000000000    R10: 0x0000000000000000
RDX: 0x0000000000000000    R11: 0x0000000000000246
RSI: 0x0000000000000000    R12: 0x0000000000000000
RDI: 0x0000000000000000    R13: 0x0000000000000000
RBP: 0x00000000001FFA50    R14: 0x0000000000000000
RSP: 0x00000000001FFA30    R15: 0x0000000000000000
RIP: 0x00007FF7A0001400
```

**Memory View:**
```
0x1FFA30: 00 00 00 00 00 00 00 00  50 FA 1F 00 00 00 00 00
0x1FFA40: 78 14 00 A0 F7 7F 00 00  00 00 00 00 00 00 00 00
```

**Call Stack:**
```
0 0x00007FF7A0001400  main
1 0x00007FF7A0001234  __tmainCRTStartup
2 0x00007FFDC5D07034  BaseThreadInitThunk
3 0x00007FFDC6932651  RtlUserThreadStart
```

### Hex View Tab

**Display:**
```
Offset    00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F   ASCII
00001000  4D 5A 90 00 03 00 00 00  04 00 00 00 FF FF 00 00   MZ..........ÿÿ..
00001010  B8 00 00 00 00 00 00 00  40 00 00 00 00 00 00 00   ¸.......@.......
00001020  00 00 00 00 00 00 00 00  00 00 00 00 00 00 00 00   ................
```

**Features:**
- **Go to address** - Jump to specific offset
- **Search** - Find byte patterns
- **Edit** - Modify bytes (if writable)
- **Export** - Save selection to file

### Timeline Tab (TTD)

**Time-travel debugging visualization:**
```
Timeline:
├─[0ms]─── Start
├─[10ms]── CreateFile
├─[25ms]── WriteFile (4 times)
├─[40ms]── VirtualAlloc
├─[55ms]── • Breakpoint at 0x140001400
├─[60ms]── CloseHandle
└─[70ms]── Exit
         ▲
      Current
```

**Controls:**
- **Scrub bar** - Drag to navigate
- **Step buttons** - Frame-by-frame
- **Bookmark** - Mark interesting points

---

## Status Bar

Bottom-most bar showing key information.

**Left Side:**
```
📄 test.exe (PE x64) | 114 functions | 18 sections
```

**Center:**
```
Decompiling: 0x140001400... (45%)
```

**Right Side:**
```
Ln 42, Col 18 | UTF-8 | Memory: 245 MB | CPU: 12%
```

**Indicators:**
- **Binary name** - Click to show full path
- **Format** - PE/ELF/Mach-O
- **Progress** - Current operation status
- **Line/Column** - Cursor position in editor
- **Memory** - Fission's memory usage
- **CPU** - CPU usage percentage

---

## Keyboard Shortcuts

### File Operations

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Open Binary | `Ctrl+O` | `Cmd+O` |
| Close Tab | `Ctrl+W` | `Cmd+W` |
| Save | `Ctrl+S` | `Cmd+S` |
| Quit | `Ctrl+Q` | `Cmd+Q` |

### Navigation

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Go to Address | `Ctrl+G` | `Cmd+G` |
| Back | `Alt+Left` | `Cmd+[` |
| Forward | `Alt+Right` | `Cmd+]` |
| Next Tab | `Ctrl+Tab` | `Cmd+Option+→` |
| Previous Tab | `Ctrl+Shift+Tab` | `Cmd+Option+←` |

### Search

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Find | `Ctrl+F` | `Cmd+F` |
| Find Next | `F3` | `Cmd+G` |
| Find Previous | `Shift+F3` | `Cmd+Shift+G` |
| Global Search | `Ctrl+Shift+F` | `Cmd+Shift+F` |

### View

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Toggle Side Bar | `Ctrl+B` | `Cmd+B` |
| Toggle Bottom Panel | `Ctrl+J` | `Cmd+J` |
| Zoom In | `Ctrl+=` | `Cmd+=` |
| Zoom Out | `Ctrl+-` | `Cmd+-` |
| Reset Zoom | `Ctrl+0` | `Cmd+0` |

### Debug

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Toggle Breakpoint | `F9` | `F9` |
| Continue | `F5` | `F5` |
| Step Over | `F10` | `F10` |
| Step Into | `F11` | `F11` |
| Step Out | `Shift+F11` | `Shift+F11` |
| Attach to Process | `Ctrl+Shift+A` | `Cmd+Shift+A` |

### Analysis

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Decompile Function | `F12` | `F12` |
| Show Cross-References | `Ctrl+R` | `Cmd+R` |
| Rename Symbol | `F2` | `F2` |
| Add Comment | `Ctrl+/` | `Cmd+/` |

---

## Workflows

### Basic Analysis Workflow

1. **Open binary** (`Ctrl+O`)
2. **Wait for analysis** (automatic)
3. **Browse functions** in Explorer
4. **Click function** to view assembly
5. **Press F12** to decompile
6. **Add comments** (`Ctrl+/`) for important findings
7. **Save notes** (`Ctrl+S`)

### Reverse Engineering Workflow

1. **Start at entry point** (0x140001400)
2. **Follow calls** (double-click call targets)
3. **Decompile interesting functions** (F12)
4. **Rename functions** (F2) for clarity
5. **Add structure definitions** (right-click → Define struct)
6. **Track cross-references** (`Ctrl+R`)
7. **Export as pseudo-C** (File → Export)

### Malware Analysis Workflow

1. **Load sample** (in VM!)
2. **Check imports** - Look for suspicious APIs
3. **Search strings** - Find URLs, IPs, registry keys
4. **Identify crypto** - Look for constants (0x67452301, etc.)
5. **Find C2 communication** - Network functions
6. **Analyze unpacking** - VirtualAlloc patterns
7. **Document IOCs** - Export findings

### Debugging Workflow

1. **Attach to process** (`Ctrl+Shift+A`)
2. **Set breakpoints** (F9 on interesting lines)
3. **Continue** (F5)
4. **Inspect registers** - Check values in Debug panel
5. **Step through** (F10 for over, F11 for into)
6. **Watch memory** - Add memory watches
7. **Record timeline** - Enable TTD for replay

---

## Themes

### Available Themes

Fission uses the Catppuccin color scheme with 4 variants:

| Theme | Background | Accent | Best For |
|-------|------------|--------|----------|
| **Latte** ☕ | Light | Blue | Daytime, bright environments |
| **Frappé** 🥤 | Medium | Rosewater | Balanced, comfortable |
| **Macchiato** 🧋 | Dark | Lavender | Low light, evening |
| **Mocha** 🍫 | Darkest | Pink | OLED, night mode |

### Changing Theme

**Method 1: Settings**
1. Click Settings (⚙️) in Activity Bar
2. Select **Appearance**
3. Choose theme from dropdown

**Method 2: Menu**
1. **View → Theme**
2. Select variant

**Method 3: Command Palette** (Future)
1. `Ctrl+Shift+P` / `Cmd+Shift+P`
2. Type "theme"
3. Select from list

### Custom Colors

Edit config file: `~/.config/fission/config.toml`

```toml
[theme]
variant = "mocha"

[theme.custom]
background = "#1e1e2e"
foreground = "#cdd6f4"
accent = "#89b4fa"
```

---

## Settings

### Accessing Settings

1. Click ⚙️ in Activity Bar, or
2. **File → Preferences**, or
3. `Ctrl+,` / `Cmd+,`

### General Settings

```
┌─ GENERAL ────────────────┐
│ Theme:         [Mocha ▼] │
│ Font Size:     [14    ▼] │
│ Font Family:   [Mono  ▼] │
│ Auto Save:     [✓]       │
│ Telemetry:     [✗]       │
└──────────────────────────┘
```

### Decompiler Settings

```
┌─ DECOMPILER ─────────────┐
│ Mode:          [Pool  ▼] │
│ Workers:       [0 (auto)]│
│ Timeout:       [30000 ms]│
│ Cache Size:    [100     ]│
│ Prefetch:      [✓]       │
└──────────────────────────┘
```

**Worker count:**
- `0` = Auto-detect (CPU cores, max 8)
- `1` = Single-threaded
- `2-8` = Manual worker count

### Debug Settings

```
┌─ DEBUG ──────────────────┐
│ TTD Enabled:   [✓]       │
│ Max Snapshots: [1000    ]│
│ Auto Break:    [✓]       │
│ Symbol Path:   [...]     │
└──────────────────────────┘
```

### Editor Settings

```
┌─ EDITOR ─────────────────┐
│ Tab Size:      [4    ▼]  │
│ Line Numbers:  [✓]       │
│ Word Wrap:     [✗]       │
│ Mini Map:      [✓]       │
│ Auto Complete: [✓]       │
└──────────────────────────┘
```

---

## Tips and Tricks

### Pro Tips

1. **Use global search** (`Ctrl+Shift+F`) to find all references to a string
2. **Double-click addresses** to jump immediately
3. **Right-click everywhere** - context menus are powerful
4. **Use TTD** for complex debugging - rewind and replay
5. **Name functions early** - Makes future analysis easier
6. **Save workspaces** - Preserve all your annotations

### Hidden Features

- **Middle-click address** → Open in new tab
- **Ctrl+scroll** → Zoom in/out
- **Shift+scroll** → Horizontal scroll
- **Alt+drag** → Column selection (future)

### Performance Tips

- **Close unused tabs** - Saves memory
- **Disable TTD** when not needed - Reduces overhead
- **Adjust cache size** - Larger = faster but more memory
- **Use worker pool** - Faster decompilation

---

## Troubleshooting

### GUI Won't Start

**Linux:** Install GTK dependencies
```bash
sudo apt install libgtk-3-dev
```

**Error: "failed to initialize display"**
- Set `DISPLAY` environment variable
- Or run with `--cli` flag instead

### Slow Performance

1. **Check CPU usage** in status bar
2. **Reduce worker count** in settings
3. **Disable prefetching** if memory-constrained
4. **Close background apps**

### Decompilation Hangs

1. **Increase timeout** in settings (default: 30s)
2. **Restart decompiler pool** (View → Restart Decompiler)
3. **Check logs** in Console panel

---

## Related Documentation

- [CLI_ONE_SHOT_MODE.md](../cli/CLI_ONE_SHOT_MODE.md) - CLI usage
- [BUILD.md](../build/BUILD.md) - Build instructions
- [PLUGIN_DEVELOPMENT.md](../plugins/PLUGIN_DEVELOPMENT.md) - Extend GUI with plugins

---

## Summary

Fission's GUI provides:
- ✅ **Modern interface** - VS Code-inspired layout
- ✅ **Powerful navigation** - Jump to anything quickly
- ✅ **Integrated debugging** - No context switching
- ✅ **Customizable** - Themes, fonts, layouts
- ✅ **Keyboard-driven** - Shortcuts for everything

**Remember:**
- `Ctrl+O` to open binaries
- `F12` to decompile
- `Ctrl+Shift+F` to search globally
- `F5` to debug
- `Ctrl+,` for settings

Happy reversing! 🔍
