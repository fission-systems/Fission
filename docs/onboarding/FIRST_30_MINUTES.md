# First 30 minutes with Fission

**Last verified:** 2026-05-02

Goal: clone → build → run one CLI command → know where semantics live.

## 1. Toolchain

Install Rust **1.85+** (see badge in [`README.md`](../../README.md)). Optional: `rustup component add rustfmt clippy`.

## 2. Build the CLI

```bash
cargo build -p fission-cli --release
```

Binary: `target/release/fission_cli` (suffix `.exe` on Windows).

## 3. Smoke one evaluation binary

Follow [`docs/EVALUATION.md`](../EVALUATION.md) — quickest path uses fixtures under:

`benchmark/binary/x86-64/window/small/binary/c/`

Example shape (exact flags in CLI doc):

```bash
./target/release/fission_cli info path/to/test_functions.exe
./target/release/fission_cli list path/to/test_functions.exe
./target/release/fission_cli decomp path/to/test_functions.exe --addr <entry_from_list>
```

## 4. Orient in the repo

Read in order:

1. [`docs/PROJECT_MAP.md`](../PROJECT_MAP.md) — crates vs directories  
2. [`AGENTS.md`](../../AGENTS.md) — canonical owners + anti-patterns  
3. [`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md) — design spine  

## 5. Tests you can run locally today

```bash
cargo test -p fission-pcode
cargo test -p fission-loader
```

## Next steps

- Debugging a bad decompilation: [`DEBUGGING_A_DECOMP_FAILURE.md`](DEBUGGING_A_DECOMP_FAILURE.md)  
- Adding loader coverage: [`ADDING_A_LOADER_TEST.md`](ADDING_A_LOADER_TEST.md)  
- Automation lanes: [`crates/fission-automation/AGENTS.md`](../../crates/fission-automation/AGENTS.md)
