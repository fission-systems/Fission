# Replay a verification difference

The repository fixture compares the same Linux x86-64 ELF natively, through
Fission's emulator, and by evaluating its printed HIR. The observations include
the 32-bit return value and a three-byte buffer around the narrow write. The
pure arithmetic fixture also checks a generated Python function. Comparisons
cover the recorded concrete inputs.

## Run and save a case

From the repository root on x86-64 Linux:

```bash
cargo build --locked -p fission-dir --example concrete_replay
python3 scripts/test/replay_fixture.py --runner target/debug/examples/concrete_replay \
  --python-out /tmp/fission-ported.py
```

The runner compiles the fixed checked-in source with `cc`. A different host can
supply an x86-64 Linux cross-compiler via `--cc` and execute the fixture with
`--linux-container <image>`.

For the diagnostic narrowing case, add `--function narrowing_store`. A
difference exits with code 1 and writes a JSON witness (default
`replay-witness.json`) containing the fixture identity, function, concrete
inputs, initial bytes, instruction budget, and each observation. Pass
`--replay replay-witness.json` to the same script to check that case. Use the
same compiler and fixture bytes. `--witness <path>` chooses a new output path.

## Export one function

```bash
target/debug/examples/concrete_replay python <binary> <function> > ported.py
```

The generated module exposes `ported` with positional arguments. The exporter
supports single-return HIR functions using integer parameters, constants,
casts, basic arithmetic and bitwise expressions, and constant left shifts. It masks intermediate
values to their recovered widths and preserves the recovered signedness.
`replay_fixture.py` compares the resulting 32 bits with the fixture ABI.
Unsupported HIR forms produce an error.
