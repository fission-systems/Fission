# x86 generated front-end artifacts

This directory stores the repo-tracked output of the compiler-only Sleigh front-end wave for `x86-64.slaspec`.

Current artifacts:

- `include_expanded_manifest.json`
- `parsed_inventory.json`
- `normalized_pattern_graph.json`
- `semantic_action_ir.txt`
- `generated_frontend.rs`

Regeneration command:

```bash
cargo run -p fission-sleigh --example generate_x86_frontend
```

These files are deterministic compiler products. They are not yet the canonical runtime decoder path.
