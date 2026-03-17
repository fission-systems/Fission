# Type Back-Propagation Via Signature Injection

This document describes the architecture used to inject function signatures from
`fission-signatures` into the native decompiler core so caller-side argument
types can be recovered through normal type inference.

The original motivating example was a call like:

```c
MessageBoxA(local_1, local_2, ...);
```

If the callee prototype is known, the decompiler can propagate the parameter
types back into the caller and recover types such as `HWND` or `LPCSTR` instead
of leaving the arguments as untyped locals.

## Core Idea

The native decompiler core already performs type inference over p-code and call
sites. If a callee prototype is available early enough, caller-side argument
types can be inferred without inventing a separate custom back-propagation
engine.

So the architectural problem was not "how do we manually infer the caller
types?" but rather:

**how do we inject externally known signatures into the core at the right
boundary?**

## Existing Pieces

### Rust Side: `fission-signatures`

The Rust-side signature data model provides information such as:

- function name
- return type
- ordered parameter list
- parameter type names
- optional enum-group metadata

The data source is JSON-based and already structured well enough to represent
WinAPI-style prototype information.

### Native Side: Prototype Enforcement

The native decompiler side already had a prototype-enforcement layer that could:

- look up a function by symbol name
- build prototype pieces
- apply those pieces to the active architecture / function prototype

That meant the missing piece was not the enforcement mechanism itself, but a
clean bridge from Rust-side signature data into the native-side database used by
prototype enforcement.

### Existing FFI Flow

The existing FFI path already supported:

- loading a binary
- registering symbols
- decompiling a target address

So signature injection naturally belonged beside symbol injection in the same
high-level lifecycle.

## Design Options Considered

Three bridge options were considered.

### Option A: JSON Blob Over FFI

Rust serializes signatures into a single JSON array and passes that blob to the
native layer through a dedicated FFI call.

Pros:

- simple ownership model
- low FFI complexity
- keeps Rust-side schema close to the source data

Cons:

- requires JSON parsing on the native side

### Option B: Shared JSON Files

Rust passes a path and the native side loads JSON files directly.

Pros:

- smaller FFI surface

Cons:

- path and packaging issues
- runtime file-layout dependency
- weaker embedding story

### Option C: C ABI Struct Arrays

Rust converts all signatures into C ABI structs and passes them as arrays.

Pros:

- no parsing overhead after conversion

Cons:

- much higher FFI complexity
- string-lifetime and ownership hazards
- brittle schema evolution

## Recommended Architecture

The preferred design was Option A: JSON blob injection over FFI.

Why:

- it matches the Rust-side signature data naturally
- it avoids file-path dependency in packaged environments
- it keeps the cross-language contract relatively stable

## Expected Data Flow

1. Rust loads or aggregates API signature data
2. Rust serializes the signature set as JSON
3. Rust sends that JSON through a dedicated FFI entry point
4. The native layer parses and stores injected signatures in the active
   decompiler context
5. Prototype enforcement checks injected signatures first
6. When a symbol name matches a known signature, the prototype is applied
7. Normal core type inference propagates those types through caller data flow

## Native Integration Points

The decompiler context needs a place to store injected signatures for the
current analysis session.

The prototype enforcer then needs to:

- consult injected signatures before falling back to built-in or older
  database-driven sources
- resolve textual type names into actual decompiler datatypes
- apply the resulting prototype to functions already present in the symbol table

This last point matters:

**the function must already be present in the symbol table for prototype
application to succeed.**

So symbol registration remains a prerequisite for successful injection.

## Why This Works

The strength of this architecture is that it does not duplicate Ghidra-style
type inference logic. It feeds better callee prototypes into the existing core,
then lets the normal inference pipeline do the heavy lifting.

That keeps the design:

- simpler
- closer to the native core's intended extension points
- easier to reason about than an entirely separate back-propagation engine

## Relationship To FID And Imported Symbol Recovery

This architecture also composes well with other name-recovery systems.

If FID, IAT symbol recovery, or other imported-symbol sources recover a stable
function name, and the injected signature database contains a matching
prototype, then type information can be seeded automatically from that name.

That makes signature injection a useful bridge between:

- symbol recovery
- prototype enforcement
- caller-side type recovery

## Implementation Status

The original Korean draft tracked this as a phased rollout and recorded the
bridge as implemented. The high-level pieces it described were:

- context-side injected-signature storage
- JSON FFI ingestion
- prototype-enforcer priority for injected signatures
- Rust-side wrapper for sending signature JSON
- signature seeding during decompiler preparation

## Limits And Caveats

- Prototype application depends on symbol-table registration order
- Type-name resolution still depends on the native side's datatype mapping
- Calling-convention handling must be consistent with architecture/platform
  defaults
- Better signatures help only where naming or import recovery can identify the
  target function reliably

## Takeaway

The important architectural lesson is:

**caller-side type improvement did not require a new inference engine. It
required a reliable signature-injection path into the existing core.**

That remains useful context even as more of the decompiler brain moves into
Rust-owned pipelines.
