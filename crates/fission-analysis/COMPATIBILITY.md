## fission-analysis compatibility policy

`fission-analysis` is a compatibility facade crate.

- Static/decomp ownership: `fission-static`
- Runtime/debug/plugin/unpacker ownership: `fission-dynamic`

Do not add new implementation modules under `src/analysis`, `src/debug`, `src/plugin`, `src/app`, `src/unpacker`, or `src/utils` in this crate.
Expose compatibility paths by re-exporting from owner crates in `src/lib.rs`.
