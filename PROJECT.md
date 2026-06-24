# Project: Fission NIR Transformation Pipeline Refactoring

## Architecture
- **NirFunc**: A dedicated representation wrapping `PreviewBuilder`'s mutable states, passed as the intermediate representation (`ir`) to each pass.
- **NirPass**: Trait with signature: `fn run(&mut self, ir: &mut NirFunc, store: &mut AnalysisStore) -> PassResult;`.
- **AnalysisStore**: Cache structure for CFG analysis results (dominance, post-dominance, loops), cleared/invalidated when passes mutate the IR.
- **PassManager**: Engine executing registered `NirPass` passes until a fixed point (no changes) or a maximum round limit is reached.
- **Integration**: Placed within the Fission decompiler's structuring driver (`CollapseDriver` / `PreviewBuilder::build_multiblock_body()`) and/or the normalization stages.

## File Layout
All new types will reside in a new dedicated `pass` module:
```text
crates/fission-pcode/src/nir/
├── pass/
│   ├── mod.rs          # Module entry point, exports NirPass, PassResult, etc.
│   ├── func.rs         # Implements NirFunc wrapper
│   ├── store.rs        # Implements AnalysisStore cache
│   └── manager.rs      # Implements PassManager
```

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Exploration & Design | Codebase analysis and draft interfaces | None | DONE |
| 2 | Core Interfaces (R1) | Implement `NirPass`, `PassManager`, `AnalysisStore`, `NirFunc` | M1 | IN_PROGRESS |
| 3 | Normalization Migration (R2) | Migrate existing normalization passes and CFG analysis | M2 | PLANNED |
| 4 | Driver Integration (R3) | Integrate `PassManager` into structuring driver | M3 | PLANNED |
| 5 | E2E & Verification | Run tests, source-semantic benchmark, and Forensic Auditor | M4 | PLANNED |

## Interface Contracts
### `NirPass` Trait
- Signature: `fn run(&mut self, ir: &mut NirFunc<'_>, store: &mut AnalysisStore) -> PassResult;`
- Returns: `PassResult` (indicating if any changes were made and if they invalidate analyses).

### `NirFunc` Wrapping
- Wraps `PreviewBuilder` mutable states: `successors`, `predecessors`, `virtual_block_map`, `lowered_block_stmts_cache`, `locals`.
- Tracks `cfg_version` and `ir_version`.

### `AnalysisStore` Caching
- Caches `CfgFactCache` and other analyses based on `cfg_version`.
- Lazily re-evaluates dominators, SCC, and loops on version mismatch.
