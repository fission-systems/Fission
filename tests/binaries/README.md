# Test Binaries

This directory contains test binaries for validating Fission's decompiler functionality across different languages and platforms.

## Files

| File | Description |
|------|-------------|
| `test_swift.swift` | Swift source code with a simple class |
| `test_swift_bin` | Compiled Swift binary (Mach-O, arm64) |
| `test_go.go` | Go source code with struct and methods |
| `test_go_bin` | Compiled Go binary (stripped) |
| `test_go_bin_full` | Compiled Go binary (with symbols) |
| `test_objc.m` | Objective-C source code |
| `test_objc_bin` | Compiled Objective-C binary |
| `test_rust.rs` | Rust source code |
| `test_rust_bin` | Compiled Rust binary |

## Usage

```bash
# Decompile Swift binary
cargo run -p fission-cli -- --cli tests/binaries/test_swift_bin 0x1000010a4

# Analyze Go binary
cargo run -p fission-cli -- --cli tests/binaries/test_go_bin info

# Decompile Rust binary
cargo run -p fission-cli -- --cli tests/binaries/test_rust_bin <addr>
```

## Rebuilding

### Swift

```bash
swiftc -O test_swift.swift -o test_swift_bin
```

### Go

```bash
go build -o test_go_bin test_go.go
go build -ldflags="-s -w" -o test_go_bin_full test_go.go
```

### Objective-C

```bash
clang -framework Foundation test_objc.m -o test_objc_bin
```

### Rust

```bash
rustc test_rust.rs -o test_rust_bin
```
