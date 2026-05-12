# Architecture Diagrams

This page keeps the high-signal Mermaid diagrams for Fission's architecture and quality workflow. The prose contract remains in [`ARCHITECTURE.md`](./ARCHITECTURE.md); this file is a visual index for quick orientation.

## Ownership Map

```mermaid
flowchart TD
    Pcode["fission-pcode<br/>canonical semantics owner"]
    Structuring["fission-pcode::nir::structuring<br/>region legality owner"]
    Decompiler["fission-decompiler<br/>orchestration owner"]
    Static["fission-static<br/>facts and native preparation owner"]
    Loader["fission-loader<br/>format loading and provenance owner"]
    Printer["printer / CLI / GUI<br/>consume-only surfaces"]

    Loader --> Static
    Static --> Decompiler
    Pcode --> Structuring
    Structuring --> Decompiler
    Decompiler --> Printer

    Printer -. "must not reconstruct semantics" .-> Pcode
```

## Loader Pipeline

```mermaid
flowchart LR
    Detect["detect<br/>format family"] --> Probe["probe/load-spec<br/>architecture + load spec"]
    Probe --> Map["map<br/>file offsets / RVA / VA blocks"]
    Map --> Symbols["symbols<br/>imports / exports / thunks"]
    Symbols --> Finalize["finalize<br/>LoadedBinary + FunctionInfo"]
    Finalize --> Identity["identity report<br/>entropy / overlay / hints / evidence"]
```

## Structuring Pipeline

```mermaid
flowchart TD
    CFG["CFG / basic-block facts"] --> Graph["StructureGraph"]
    Graph --> Proof["RegionProof"]
    Proof --> Collapse["CollapseDriver"]
    Collapse --> HIR["structured HIR"]
    Proof -->|"incomplete legality"| Fallback["explicit unstructured / goto output"]
```

## Source Semantic Quality Workflow

```mermaid
flowchart LR
    Source["checked-in source"] --> Extract["function extraction"]
    Binary["checked-in binary"] --> Fission["fission_cli list/decomp"]
    Extract --> StaticCompare["static fingerprint comparison"]
    Fission --> StaticCompare
    Extract --> Behavior["dynamic behavior harness"]
    Fission --> Behavior
    StaticCompare --> Rows["source_semantic_rows.json"]
    Behavior --> Rows
    Rows --> Summary["summary JSON / Markdown"]
    Summary --> Debug["debug surfaces<br/>decomp / disasm / xrefs / inventory"]
```

> [!NOTE]
> Keep diagrams high-level. When a diagram starts encoding policy details, move that policy into prose in `ARCHITECTURE.md` and keep the diagram as an orientation aid.
