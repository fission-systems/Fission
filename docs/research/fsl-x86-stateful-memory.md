# Ordered stores and stack frames in the same FIR

Experimental continuation, 2026-10-11. Work stays on `codex/fsl-jvm-iadd-parity`.

`MemoryStoreLittle` / `memory.store.le %address, %value` is a canonical ordered
effect. Address must be u64 and value u8/u16/u32/u64. Package v11 adds tag 22;
v1..v10 formats, emission and old fixtures remain unchanged. Native lift/JIT,
stack/wave and GPU backends without this memory contract refuse the effect.

`MemoryWindow` carries explicit readonly/mutable permissions over flat bytes.
`execute_decoded_memory` and `X86Program::execute_mut` commit registers, flags
and memory only on success. Mutable transactions cap memory at 1 MiB.
Generated C/Rust requires disjoint caller scratch of exactly the memory length;
failure preserves architectural input storage and exit PC, while scratch is
unspecified. There is no guest fault, MMU, atomic or concurrent memory model.
Readonly execution/ABI remains available and refuses store-bearing programs.

The optional stateful x86 pool has 48 rules / 141 FIR bodies. It adds nine
register-to-memory MOV bodies and six unprefixed register PUSH/POP bodies to
the prior readonly pool. Memory syntax reuses ModRM/SIB/address binding; stack
arithmetic and ordered effects are FSL, without opcode-specific arithmetic
executors. PUSH reads its input before decrementing SP. POP reads at old SP,
increments SP, then writes its destination (including POP SP). Long-mode REX.B
register extension is admitted; other prefixed stack forms refuse.

The 16-bit contract uses modern IA-32 ordering, not the 8086 PUSH SP behavior.
Stack-address width equals selected mode and SS base is zero; descriptor B flag
is not recovered. The PUSH/POP bodies carry an additional Intel SDM Volume 2B
source URL and PDF hash (observed 2026-10-11), separately from the legacy source.

This v1 pool refuses memory immediates. The later optional
[v2 split-binding pool](fsl-x86-immediate-memory.md) adds MOV C7 stores. Memory ALU destinations without an authored body also refuse.
CALL/LEAVE, recovered CFG joins, full cspec allocation, source prototypes and
complete x86 migration remain future work. No external decoder/dependency change.

The derived FIR analysis records ordered stores and read-after-store evidence.
Exact equal-address/equal-width forwarding is permitted only with no intervening
possible alias. Proven disjoint ranges with the same modular address root can
be skipped. Partial overlaps and unknown address roots retain external-read
and possible-store dependencies as an overapproximation; no ABI/type is inferred.

Observed local gates: full x86-feature nextest 66 passed, no skips; strict FSL
Clippy with and without the x86 feature passed. Three fixed compiled ELF spans
(O0 i386 EBP frame, O0 AMD64 pointer spill/reload, O2 AMD64 external store)
produce 768 FIR states and 6,144 C/Rust O0/O2 success/rollback comparisons.
The failure corpus exhausts budget after an actual PUSH or MOV store.
Original 64-bit bytes run through Rosetta in 512 cases: result, six defined
flags, preserved RBP, balanced SP and external memory agree. Local i386 original
execution is unavailable; the research Linux CI runs its libc-free adapter.

The actual SLA/canonical P-code baseline agrees on GPRs, return PC and all 160
modeled memory bytes. AF differs in 96/256 ADD cases per fixture; other flags
agree. This remains an existing specification omission, not IR superiority.
The research repository locks sources/ELFs/package, 12 outputs and reports with
tool/host/source hashes. Whole-function equivalence and all-register/stack-memory
hardware snapshots are not established by this bounded experiment.
