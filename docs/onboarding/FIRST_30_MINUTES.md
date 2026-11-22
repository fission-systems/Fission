# First 30 minutes with Fission

Goal: clone → build → run one CLI command → know where semantics live.

## 1. Toolchain

Install current stable Rust and nextest using the
[contributor setup instructions](https://github.com/fission-systems/Fission/blob/main/CONTRIBUTING.md#development-setup).

## 2. Build the CLI

```bash
cargo build -p fission-cli --release
```

Binary: `target/release/fission_cli` (suffix `.exe` on Windows).

## 3. Smoke one evaluation binary

Use the [external evaluation guide](../EVALUATION.md) to inspect the checked-in
Windows x64 fixture. Start with `info`, `list` and `decomp --addr`.

## 4. Orient in the repo

Read in order:

1. [`docs/PROJECT_MAP.md`](../PROJECT_MAP.md) — crates vs directories
2. [`AGENTS.md`](https://github.com/fission-systems/Fission/blob/main/AGENTS.md) — canonical owners + anti-patterns
3. [`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md) — design spine

## 5. Tests you can run locally today

```bash
cargo nextest run -p fission-pcode
cargo nextest run -p fission-loader
```

## Next steps

- Debugging a bad decompilation: [`DEBUGGING_A_DECOMP_FAILURE.md`](DEBUGGING_A_DECOMP_FAILURE.md)
- Adding loader coverage: [`ADDING_A_LOADER_TEST.md`](ADDING_A_LOADER_TEST.md)
