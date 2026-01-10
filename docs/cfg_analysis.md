# CFG (Control Flow Graph) Analysis

## Overview

Fission's CFG analysis module provides comprehensive control flow graph analysis capabilities for binary functions. It extracts and analyzes control flow structures using Ghidra's Pcode IR as the foundation.

## Features

### 1. **Basic Block Extraction**

- Automatically identifies basic blocks from Pcode operations
- Tracks entry and exit blocks
- Maintains predecessor/successor relationships

### 2. **Dominator Tree Computation**

- Implements Cooper-Harvey-Kennedy algorithm for computing dominators
- Calculates immediate dominators (IDom)
- Computes dominance frontiers for SSA construction
- Provides dominator tree depth tracking

### 3. **Loop Detection**

- Identifies natural loops using back-edge detection
- Classifies loop types (While, Do-While, For, Infinite)
- Detects nested loops and computes nesting depth
- Finds loop exit points and latches

### 4. **Complexity Metrics**

- **Cyclomatic Complexity**: McCabe's M = E - N + 2P formula
- **Essential Complexity**: Measures unstructured constructs
- **Nesting Depth**: Maximum and average loop nesting
- **Maintainability Index**: Composite metric based on Halstead volume, complexity, and LOC

### 5. **Visualization**

- Export to Graphviz DOT format
- Customizable styling options
- Loop highlighting with colored subgraphs
- ASCII art representation for terminal output

## Usage

### Command Line

```bash
# Generate CFG summary
fission binary.exe --cfg 0x140001000

# Export to DOT format and render to PNG
fission binary.exe --cfg 0x140001000 --cfg-format dot -o function.dot
# Automatically generates function.png if Graphviz is installed

# Export to JSON
fission binary.exe --cfg 0x140001000 --cfg-format json -o function.json

# ASCII art in terminal
fission binary.exe --cfg 0x140001000 --cfg-format ascii
```

### Programmatic API

```rust
use fission_analysis::analysis::cfg::{CfgAnalysis, CfgVisualizer, DotOptions};
use fission_analysis::analysis::pcode::PcodeFunction;

// Get Pcode from decompiler
let pcode_json = decompiler.get_pcode(function_address)?;
let pcode_function = PcodeFunction::from_json(&pcode_json)?;

// Build CFG analysis
let analysis = CfgAnalysis::from_pcode(&pcode_function)?;

// Access results
println!("Blocks: {}", analysis.cfg.block_count());
println!("Cyclomatic Complexity: {}", analysis.metrics.cyclomatic_complexity);
println!("Loops detected: {}", analysis.loops.len());

// Generate DOT visualization
let dot_options = DotOptions::default();
let dot_content = CfgVisualizer::to_dot(
    &analysis.cfg,
    &analysis.loops,
    &dot_options
);

// Print summary report
println!("{}", analysis.summary());
```

## Output Formats

### Summary Format

```
=== CFG Analysis Summary ===
Basic Blocks: 15
Edges: 20
Entry Block: 0
Exit Blocks: [14]

=== Metrics ===
Cyclomatic Complexity: 6 (Moderate)
Max Nesting Depth: 2
Number of Loops: 1

=== Detected Loops ===
Loop 0: Header=3, Kind=While, Blocks=[3, 4, 5, 6]

=== Dominator Tree ===
Dominator Tree (root: BB0):
BB0
  BB1
    BB3
      BB4
      BB5
...
```

### DOT Format

Generates Graphviz DOT files with:

- Color-coded nodes (green=entry, red=exit, yellow=loop)
- Edge labels (T=true, F=false, back=loop back-edge)
- Loop clustering with dashed boundaries
- Customizable styling options

### JSON Format

```json
{
  "function_address": "0x140001000",
  "block_count": 15,
  "edge_count": 20,
  "cyclomatic_complexity": 6,
  "max_nesting_depth": 2,
  "loop_count": 1,
  "loops": [
    {
      "header": 3,
      "kind": "While",
      "body": [3, 4, 5, 6],
      "back_edges": [[6, 3]]
    }
  ],
  "blocks": [
    {
      "index": 0,
      " address": "0x140001000",
      "is_entry": true,
      "is_exit": false,
      "successors": [1, 2],
      "predecessors": [],
      "instruction_count": 5
    }
    ...
  ]
}
```

### ASCII Format

```
Control Flow Graph
==================

BB0 @ 0x140001000 [ENTRY]
  -> BB1 (T), BB2 (F)

BB1 @ 0x140001010
  <- BB0
  -> BB3 ()

BB2 @ 0x140001020
  <- BB0
  -> BB3 ()

BB3 @ 0x140001030 [EXIT]
  <- BB1, BB2
```

## Architecture

### Module Structure

```
cfg/
├── basic_block.rs     - BasicBlock and edge definitions
├── graph.rs           - ControlFlowGraph structure and builder
├── dominator.rs       - Dominator tree computation
├── loops.rs           - Loop detection and classification
├── metrics.rs         - Complexity metrics calculation
├── visualization.rs   - DOT and ASCII visualization
└── mod.rs             - Public API exports
```

### Key Data Structures

```rust
pub struct BasicBlock {
    pub index: usize,
    pub start_address: u64,
    pub end_address: u64,
    pub operations: Vec<PcodeOp>,
    pub successors: Vec<BlockEdge>,
    pub predecessors: Vec<usize>,
    pub is_entry: bool,
    pub is_exit: bool,
}

pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
    pub entry_block: usize,
    pub exit_blocks: Vec<usize>,
    pub function_address: u64,
}

pub struct DominatorTree {
    pub idom: HashMap<usize, usize>,
    pub children: HashMap<usize, Vec<usize>>,
    pub dominance_frontier: HashMap<usize, HashSet<usize>>,
    pub depth: HashMap<usize, usize>,
}

pub struct Loop {
    pub header: usize,
    pub body: HashSet<usize>,
    pub back_edges: Vec<(usize, usize)>,
    pub exit_edges: Vec<(usize, usize)>,
    pub kind: LoopKind,
    pub depth: usize,
    pub parent: Option<usize>,
}

pub struct CfgMetrics {
    pub cyclomatic_complexity: usize,
    pub essential_complexity: usize,
    pub max_nesting_depth: usize,
    pub loop_count: usize,
    pub exit_count: usize,
}
```

## Complexity Ratings

| Cyclomatic Complexity | Rating | Recommendation |
|----------------------|--------|----------------|
| 1-5 | Low | Simple function, good testability |
| 6-10 | Moderate | Consider breaking down if it grows |
| 11-20 | High | Recommend refactoring |
| 21-50 | Very High | Strongly recommend refactoring |
| 50+ | Extreme | Function should be split immediately |

## Implementation Details

### Dominator Tree Algorithm

Uses the Cooper-Harvey-Kennedy iterative algorithm:

1. Initialize entry block as self-dominating
2. Iterate over blocks in reverse postorder
3. Compute idom[b] as intersection of predecessors' dominators
4. Repeat until convergence (fixed point)

### Loop Detection

1. Find back edges: edges where target dominates source
2. Group back edges by header block
3. Find natural loop body via backward traversal from latches
4. Classify loop type based on exit location:
   - **While**: Exit from header
   - **Do-While**: Exit from latch
   - **Infinite**: No exits
   - **For**: Detected via pattern matching (future work)

### Cyclomatic Complexity

Two formulas supported:

- **M = E - N + 2P**: Edges - Nodes + 2×Components
- **M = D + 1**: Decision points + 1

## Performance

- **CFG Building**: O(N) where N = number of Pcode operations
- **Dominator Tree**: O(N²) worst case, typically O(N log N)
- **Loop Detection**: O(N + E) where E = number of edges
- **Metrics**: O(N)

Typical performance for 100-block function: < 10ms

## Future Enhancements

- [ ] Structural analysis (if-else, switch detection)
- [ ] Inter-procedural CFG (call graph integration)
- [ ] Path analysis (all paths from entry to exit)
- [ ] Hot path identification
- [ ] CFG diff for patch analysis
- [ ] Interactive visualization (web-based)

## References

- Cooper, Keith D., et al. "A Simple, Fast Dominance Algorithm." (2001)
- McCabe, Thomas J. "A Complexity Measure." IEEE TSE (1976)
- Lengauer, Thomas, and Robert Endre Tarjan. "A fast algorithm for finding dominators in a flowgraph." TOPLAS (1979)
