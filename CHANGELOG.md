# Changelog

## [Unreleased] - 2026-01-13

### Added

- **Loop Exit Elimination**: Implemented `eliminate_loop_exits` in `CFGStructurizer` to convert `goto` statements targeting labels immediately following a loop into structured `break` statements.
- **Continue Recovery**: Enhanced `convert_unconditional_backward_goto` to identify loop headers and transform backward jumps to the header into `continue` statements.

### Improved

- **For-Loop Pattern Recognition**: Significantly enhanced regex patterns in `CFGStructurizer` to recover C-style `for` loops with complex initializations, variable bounds, and various increment styles.
- **Nested Loop Handling**: Refactored `convert_nested_loop_patterns` for better stability when dealing with multi-level control flow.
- **Decompilation Pipeline Integration**: Integrated the refined `CFGStructurizer` into the main `PostProcessPipeline`, ensuring all decompiled output benefits from these structural improvements.
- **Ghidra Batch Analysis**: Updated `pyghidra_decompile_batch.py` and `compare_decompilers_v2.py` to capture and display assembly listings from Ghidra even when using cached batch results.

### Fixed

- Fixed an issue where Ghidra's assembly was not being displayed in comparison reports during batch mode.
- Improved the robustness of similarity measurements by better normalizing Fission's structured output.
