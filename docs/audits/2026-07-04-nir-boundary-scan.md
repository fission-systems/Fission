# NIR Boundary Scan

- Repo: `/Users/sjkim1127/Fission`
- Findings: `52`
- Violations: `5`
- Migration debt: `47`

| Severity | Edge | Location | Detail |
|---|---|---|---|
| `migration` | `action_pipeline -> normalize` | `crates/fission-pcode/src/nir/action_pipeline/pass.rs:97` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/expr/lower_expr.rs:687` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/init.rs:52` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/init.rs:54` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/init.rs:57` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/init.rs:75` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/loop_carried/shape.rs:34` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/loop_carried/shape.rs:43` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:1416` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2135` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2475` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2506` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2652` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2743` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2766` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/materialize/mod.rs:2838` | Known boundary debt; do not copy this pattern for new fixes. |
| `violation` | `builder -> normalize` | `crates/fission-pcode/src/nir/builder/materialize/same_block.rs:1924` | Owner-to-owner dependency should be moved through substrate facts. |
| `violation` | `builder -> render` | `crates/fission-pcode/src/nir/builder/materialize/trace.rs:2069` | Owner-to-owner dependency should be moved through substrate facts. |
| `violation` | `builder -> render` | `crates/fission-pcode/src/nir/builder/materialize/trace.rs:2128` | Owner-to-owner dependency should be moved through substrate facts. |
| `violation` | `builder -> render` | `crates/fission-pcode/src/nir/builder/materialize/trace.rs:2200` | Owner-to-owner dependency should be moved through substrate facts. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/mod.rs:272` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/state.rs:30` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/state.rs:31` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `builder -> structuring` | `crates/fission-pcode/src/nir/builder/state.rs:36` | Known boundary debt; do not copy this pattern for new fixes. |
| `violation` | `normalize -> structuring` | `crates/fission-pcode/src/nir/normalize/cleanup/control_flow.rs:3` | Owner-to-owner dependency should be moved through substrate facts. |
| `migration` | `pass -> builder` | `crates/fission-pcode/src/nir/pass/func.rs:1` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/func.rs:2` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/func.rs:3` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> builder` | `crates/fission-pcode/src/nir/pass/manager.rs:150` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/store.rs:2` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/store.rs:3` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/store.rs:4` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:2` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:6` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:7` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:268` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:269` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:280` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:287` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:304` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `pass -> structuring` | `crates/fission-pcode/src/nir/pass/structuring.rs:358` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `render -> structuring` | `crates/fission-pcode/src/nir/render/printer.rs:179` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `render -> structuring` | `crates/fission-pcode/src/nir/render/printer.rs:1145` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `structuring -> builder` | `crates/fission-pcode/src/nir/structuring/cfg_analysis/mod.rs:94` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `structuring -> normalize` | `crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs:570` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `structuring -> normalize` | `crates/fission-pcode/src/nir/structuring/switch.rs:2` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `structuring -> builder` | `crates/fission-pcode/src/nir/structuring/switch.rs:70` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `structuring -> builder` | `crates/fission-pcode/src/nir/structuring/switch.rs:90` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `support -> structuring` | `crates/fission-pcode/src/nir/support/switch_util.rs:37` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `support -> structuring` | `crates/fission-pcode/src/nir/support/switch_util.rs:110` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `support -> structuring` | `crates/fission-pcode/src/nir/support/switch_util.rs:116` | Known boundary debt; do not copy this pattern for new fixes. |
| `migration` | `vsa -> normalize` | `crates/fission-pcode/src/nir/vsa/jump_resolver.rs:17` | Known boundary debt; do not copy this pattern for new fixes. |
