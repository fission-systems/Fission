# Changelog: x86-32 Sleigh P-code 100% Parity

**Date:** 2026-04-30
**Scope:** `fission-sleigh` runtime – x86-32 non-shared-cursor P-code correctness

## Summary

Achieved **100% P-code parity** (50/50) with Ghidra for the `EverPlanet_KR_v1842_U_DEVM.exe` (x86 32-bit) binary, while preserving 100% parity on the canonical x86-64 benchmark. All fixes are architectural/algorithmic — no x86-specific hardcoding or heuristics.

Previous parity: **46/50 (92%)**. All 4 remaining `input_varnode_mismatch` errors resolved.

---

## Root Cause

Ghidra's Sleigh runtime computes a token field's absolute byte position as:

```
read_position = point.getOffset() + bytestart
```

where `point.getOffset()` = `parent_constructor_offset + operand.reloffset`.

Fission was computing:

```
read_position = ctx.cursor + byte_start
```

and ignoring `reloffset` entirely for non-shared-cursor architectures (x86-32). For sequential operands (tokens after a `;` in the Sleigh pattern, e.g. `imm32` in `addr32: [imm32] is mod=0 & r_m=5; imm32`), `reloffset=1` (one byte after the ModRM byte), causing Fission to read from the ModRM position instead of the displacement position.

This affected `SlaTokenField`, `SlaVarnodeList`, `SlaValueMap`, and `SlaPatternExpression` operand specs.

---

## Changes

### `crates/fission-sleigh/src/compiler/ir/types.rs`

- Added `reloffset: i32` field (with `#[serde(default)]`) to:
  - `CompiledOperandSpec::SlaTokenField`
  - `CompiledOperandSpec::SlaVarnodeList`
  - `CompiledOperandSpec::SlaValueMap`
  - `CompiledOperandSpec::SlaPatternExpression`

### `crates/fission-sleigh/src/compiler/sla/symbols.rs`

- Propagated `symbol.reloffset` (from `ATTRIB_OFF` / `ATTR_OFF = 6`) to all four spec variants in `compiled_operand_spec_for_symbol()`.

### `crates/fission-sleigh/src/runtime/spine/compiled_table/walker.rs`

- Changed `token_base_for_sla_field(&self) -> usize` to `token_base_for_sla_field(&self, reloffset: i32) -> usize`.
- For non-shared-cursor architectures: `base = (ctx.cursor + reloffset).max(0)` instead of just `ctx.cursor`.
- For shared-cursor (x86-64): behavior unchanged.
- Updated all `bind_operand` match arms for `SlaTokenField`, `SlaVarnodeList`, `SlaValueMap` to pass `*reloffset`.
- Added non-shared-cursor `TokenField` fast-path for `SlaPatternExpression` that applies `reloffset` via `token_base_for_sla_field`.

### `crates/fission-sleigh/src/compiler/codegen.rs`

- Added `reloffset: _` to pattern destructures for `SlaTokenField`, `SlaVarnodeList`, `SlaValueMap`, `SlaPatternExpression`.

---

## Validation

| Binary | Before | After |
|--------|--------|-------|
| `EverPlanet_KR_v1842_U_DEVM.exe` (x86-32) | 46/50 (92%) | **50/50 (100%)** |
| canonical x86-64 benchmark (`test-functions-add`) | 100% | **100%** (no regression) |
