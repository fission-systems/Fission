# CLI One-Shot Analysis Mode

## Overview

Fission's CLI supports two operational modes:

1. **REPL Mode** - Interactive shell for exploratory analysis
2. **One-Shot Mode** - Single analysis execution with immediate exit (this document)

One-shot mode is designed for automation, CI/CD pipelines, and quick single-purpose analysis without entering the interactive shell.

---

## Usage

### Basic Syntax

```bash
fission --cli <binary_path> <analysis_flag> [options]
fission --cli <binary_path> <address> [--asm] [--count N]
```

The `--cli` flag is **required** to enter CLI mode. Additional flags determine whether to enter REPL or execute one-shot analysis.

### Behavior

- **Without additional flags**: Enters REPL mode
  ```bash
  fission --cli binary.exe
  # Enters interactive shell, waits for commands
  ```

- **With analysis flags**: Executes one-shot analysis
  ```bash
  fission --cli binary.exe --info
  # Displays binary info and exits immediately
  ```

---

## Available One-Shot Flags

### `--info`
Display comprehensive binary information.

**Output includes**:
- File format (PE, ELF, Mach-O)
- Architecture (x86, x86_64, ARM, etc.)
- Bitness (32-bit or 64-bit)
- Entry point address
- Image base address
- File size
- Number of sections
- Number of discovered functions

**Example**:
```bash
$ fission --cli examples/winapi_test.exe --info

Binary Information
  Path: examples/winapi_test.exe
  Format: PE (binrw)
  Architecture: x86:LE:64:default
  Bitness: 64-bit
  Entry Point: 0x0000000140001400
  Image Base: 0x0000000140000000
  File Size: 158041 bytes
  Sections: 18
  Functions: 114
```

---

### `--sections`
Display section table with permissions.

**Output includes**:
- Section name
- Virtual address
- Size in bytes
- Permissions (R=Read, W=Write, X=Execute)

**Example**:
```bash
$ fission --cli examples/winapi_test.exe --sections

Section Information
  Name         Virtual Addr       Size         Flags
  ────────────────────────────────────────────────────────────
  .text        0x0000000140001000       7968 R-X
  .data        0x0000000140003000        160 RW-
  .rdata       0x0000000140004000       3528 R--
  .pdata       0x0000000140005000        600 R--
  .idata       0x0000000140008000       3140 R--
```

---

### `--strings [min_len]`
Extract printable ASCII and Unicode strings.

**Characteristics**:
- Minimum string length: 4 characters (default)
- Optional argument to set minimum length
- Shows address offset and string content
- Useful for finding hardcoded paths, error messages, URLs

**Example**:
```bash
$ fission --cli binary.exe --strings
$ fission --cli binary.exe --strings 8
```

**Filtering output**:
```bash
# Find specific strings
fission --cli binary.exe --strings | grep "config"
fission --cli binary.exe --strings | grep -i "error"

# Count strings
fission --cli binary.exe --strings | wc -l
```

---

### `--xrefs <address>`
Show cross-references to a specific address.

**Output includes**:
- Addresses that reference the target
- Instruction type (call, jmp, mov, etc.)
- Disassembly of referencing instruction

**Example**:
```bash
$ fission --cli binary.exe --xrefs 0x140001234

References to 0x140001234:
  0x140001100: call 0x140001234
  0x140001500: jmp 0x140001234
  0x140002000: lea rax, [0x140001234]
```

**Use cases**:
- Find all callers of a function
- Locate data references
- Understand code flow
- Identify dead code (zero xrefs)

---

### `--count <N>`
Set the number of instructions for disassembly output.

**Use with**:
- `--asm` + `<address>` (one-shot disassembly)

**Example**:
```bash
$ fission --cli binary.exe 0x140001450 --asm --count 100
```

---

## Use Cases

### 1. CI/CD Integration

```bash
#!/bin/bash
# Automated binary analysis in CI pipeline

BINARY="build/output.exe"

# Extract metadata
fission --cli $BINARY --info > analysis/info.txt
fission --cli $BINARY --sections > analysis/sections.txt
fission --cli $BINARY --strings > analysis/strings.txt

# Check for suspicious strings
if fission --cli $BINARY --strings | grep -qi "malware\|keylog"; then
    echo "Suspicious strings detected!"
    exit 1
fi
```

---

### 2. Batch Analysis

```bash
#!/bin/bash
# Analyze multiple binaries

for binary in binaries/*.exe; do
    echo "Analyzing: $binary"
    fission --cli "$binary" --info >> report.txt
    echo "---" >> report.txt
done
```

---

### 3. Quick Triage

```bash
# Rapid binary triage during incident response

# 1. Check file type and architecture
fission --cli suspicious.exe --info

# 2. Look for sections (packed binaries have unusual sections)
fission --cli suspicious.exe --sections

# 3. Extract strings for IOCs
fission --cli suspicious.exe --strings | grep -E "(http|C:\\|\.dll)"

# 4. Get function count (low count = packed/obfuscated)
fission --cli suspicious.exe --info
```

---

### 4. Scripting and Automation

```python
#!/usr/bin/env python3
import subprocess
import json

def analyze_binary(path):
    """Extract binary info using Fission one-shot mode"""
    result = subprocess.run(
        ['fission', '--cli', path, '--info'],
        capture_output=True,
        text=True
    )
    # Parse output and return structured data
    return parse_fission_output(result.stdout)

def find_references(binary, address):
    """Find all references to an address"""
    result = subprocess.run(
        ['fission', '--cli', binary, '--xrefs', hex(address)],
        capture_output=True,
        text=True
    )
    return parse_xrefs(result.stdout)
```

---

## Address Format Support

All flags that accept addresses support multiple formats:

| Format | Example | Description |
|--------|---------|-------------|
| Hexadecimal (0x prefix) | `0x140001000` | Standard hex notation |
| Hexadecimal (no prefix) | `140001000` | Interpreted as hex if valid |
| Decimal | `5368713216` | Plain decimal number |

**Examples**:
```bash
fission --cli binary.exe --xrefs 0x140001000
fission --cli binary.exe --xrefs 140001000      # Same as above
fission --cli binary.exe --xrefs 5368713216     # Decimal
```

---

## Comparison: REPL vs One-Shot

| Feature | REPL Mode | One-Shot Mode |
|---------|-----------|---------------|
| **Invocation** | `--cli <binary>` | `--cli <binary> <flag>` |
| **Behavior** | Interactive shell | Execute and exit |
| **Use Case** | Exploratory analysis | Automation |
| **Multiple Commands** | Yes | No (single command only) |
| **Scripting** | Difficult | Easy |
| **Output** | Formatted, colorized | Parseable |

---

## Error Handling

### File Not Found
```bash
$ fission --cli nonexistent.exe --info
Error: Failed to open binary: No such file or directory
```

### Invalid Address Format
```bash
$ fission --cli binary.exe --xrefs invalid_addr
Error: Invalid address format: invalid_addr
```

### Unsupported Format
```bash
$ fission --cli image.png --info
Error: Unsupported binary format
```

---

## Performance Considerations

### Speed
One-shot mode is optimized for speed:
- No REPL initialization overhead
- Minimal analysis (only what's requested)
- No interactive prompt delays

### Resource Usage
- **Memory**: Loads only necessary data
- **CPU**: Single analysis pass
- **Disk I/O**: Reads binary once

### Benchmarks
```bash
# Typical execution times (varies by binary size)
time fission --cli binary.exe --info      # ~0.2s
time fission --cli binary.exe --sections  # ~0.2s
time fission --cli binary.exe --strings   # ~0.5s (scans entire binary)
time fission --cli binary.exe --xrefs 0x  # ~0.3s
```

---

## Implementation Details

### Code Location
- Main CLI handler: `src/main.rs`
- One-shot implementations: `src/ui/cli/handlers.rs`
- Flag parsing: `src/main.rs` (Args struct with clap)

### Flow
1. Parse arguments with `clap`
2. Load binary with `analysis::loader`
3. Check for one-shot flags
4. Execute requested analysis
5. Print results
6. Exit immediately (skip REPL)

### Adding New One-Shot Flags

To add a new one-shot flag:

1. **Add flag to Args struct** (`src/main.rs`):
```rust
#[arg(long, help = "New analysis flag")]
pub new_flag: bool,
```

2. **Add handler** (`src/ui/cli/handlers.rs`):
```rust
pub fn cmd_new_analysis(binary: &BinaryInfo) -> Result<()> {
    println!("New Analysis Results");
    // Implementation
    Ok(())
}
```

3. **Add dispatch logic** (`src/main.rs`):
```rust
if args.new_flag {
    cmd_new_analysis(&binary_info)?;
    return Ok(());
}
```

---

## Related Documentation

- [ARCHITECTURE.md](../architecture/ARCHITECTURE.md) - Overall system architecture
- [examples/README.md](../examples/README.md) - Test cases and validation

---

## Future Enhancements

Potential improvements to one-shot mode:

- [ ] JSON output format (`--json`) for easier parsing
- [ ] Batch mode (`--batch file_list.txt`)
- [ ] Output redirection (`--output file.txt`)
- [ ] Filtering options (`--strings-min-length 8`)
- [ ] Export formats (`--export-csv`, `--export-html`)
- [ ] Parallel processing for multiple binaries
- [ ] Progress indicators for long operations

---

## Examples Gallery

### Malware Analysis Workflow
```bash
# 1. Quick triage
fission --cli malware.exe --info

# 2. Check for packing (unusual sections, low function count)
fission --cli malware.exe --sections
fission --cli malware.exe --info

# 3. Extract IOCs
fission --cli malware.exe --strings | grep -E "(http|ftp|\.com|\.exe)"

# 4. Find suspicious API usage
fission --cli malware.exe --strings | grep -iE "(virtualalloc|createthread|writeprocessmemory)"
```

### Vulnerability Research
```bash
# Find all references to a potentially vulnerable function
fission --cli app.exe --xrefs 0x140001234

# Extract strings to find format string vulnerabilities
fission --cli app.exe --strings | grep "%"
```

### Reverse Engineering
```bash
# Understand binary structure
fission --cli target.exe --info
fission --cli target.exe --sections

# Find interesting strings (config files, registry keys)
fission --cli target.exe --strings | grep -i "software\\\\company"
```

---

## Troubleshooting

### Q: One-shot mode hangs
**A**: Check if binary is corrupted or if decompiler is stuck. Use timeout:
```bash
timeout 30s fission --cli binary.exe --info
```

### Q: No output displayed
**A**: Check if binary loaded successfully. Try verbose mode:
```bash
fission --cli binary.exe --info -vvv
```

### Q: Permission denied
**A**: Ensure binary has read permissions:
```bash
chmod +r binary.exe
```

---

## Summary

One-shot mode provides:
- ✅ Fast, single-purpose analysis
- ✅ Easy scripting and automation
- ✅ CI/CD integration friendly
- ✅ Minimal resource usage
- ✅ Parseable output

Perfect for:
- 🎯 Automated testing
- 🎯 Batch processing
- 🎯 Quick triage
- 🎯 Integration with other tools
