# Optional call provenance and partial recovered type context

This is producer evidence transport. It does not improve decompilation semantics,
change the primary NIR text, or certify executable equivalence.

## Calls

`PreHirExpr::Call` and `HirExpr::Call` carry an optional `CallOrigin` from a
real p-code `CALL`/`CALLIND`: instruction address and p-code sequence number.
Conversion and expression reconstruction retain it. Synthetic userops, math
helpers and tail-transfer summaries have no native call origin. Provenance is
excluded from semantic expression equality and legacy Debug keys.

The renderer uses the existing line trace and emits a call-only record for a
simple assignment to a variable, expression statement or return containing one
call. It deliberately abstains for memory LHSs, initializers, control headers,
multiple/nested calls and calls without an origin. One conditional subexpression
may contain the sole call; the record identifies that occurrence, not execution
on every path. The line trace does not introduce, move or re-evaluate effects.

```json
{
  "line_number": 12,
  "addresses": [4096],
  "defined_variables": [],
  "call_site": {"instruction_address": 4096, "pcode_sequence": 3}
}
```

The empty variable scope is required: it grants no variable ownership on that
line. Existing variable-definition records omit `call_site` and retain their
original contract. CLI prefix/filter remapping applies equally to both scopes.
Cloned occurrences retain the same native identity, including multiplicity;
consumers must reject shared/ambiguous native-call claims rather than deduplicate
them into multiple successful recoveries.

Consumers must validate the instruction start and call opcode against the exact
input image, establish a unique emitted occurrence, and resolve a direct target
from native bytes. Printed names, nearby PCs and assignment-definition PCs are
insufficient. An indirect call PC does not prove an indirect target.

## Partial type context

Recovered variables may carry `type_shape`, derived only from the existing
`NirBinding.ty`: scalar width/signedness, pointer pointee and inferred aggregate
field offsets/types. `unknown` and absent shapes remain unknown.

Aggregate shapes always say `layout_complete: false`; `observed_size` is the
inferred region size, not a proved complete C++ object size. Field names may be
generated. No base, virtual-base, bitfield, RTTI or full class reconstruction is
asserted. Transporting a field observation does not establish that a particular
memory operation accesses it. Consumers must not award a full-object match from
this partial context. A depth bound preserves unknown subtrees without guessing.
