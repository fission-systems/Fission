# Fission Native JIT Emulator: Design & Implementation Plan

This document outlines the architectural design and implementation milestones for Fission's cleanroom, native Rust JIT emulator. It synthesizes structural insights from QEMU (11.0.2) TCG and Unicorn (2.1.4) while maintaining Fission's purely symbolic, Rust-first design philosophy.

## 1. Fission Architecture Integration

### A. Boundary Between NIR/HIR and JIT Execution
Fission already translates guest architectures to a normalized intermediate representation (NIR/HIR) using Ghidra's Sleigh. 
- **The Boundary:** The JIT compiler will consume **NIR** as its input IR (similar to TCG ops). Execution will gracefully degrade to symbolic analysis or the solver only when it encounters symbolic memory or unmapped regions.
- **Hook Integration:** Like Unicorn, dynamic analysis hooks will be baked directly into the JIT compilation phase. If a hook is registered on an address, the JIT emits a Rust closure callback (`callout`) before the block execution, yielding control back to Fission dynamically without breaking the JIT loop.

### B. Time-Travel Debugging (TTD) vs. JIT Cache
- **Conflict:** JIT blocks execute directly on the host CPU, heavily manipulating memory and registers. TTD relies on deterministic, granular state tracking, which is lost during rapid JIT execution.
- **Resolution:** We will adopt a **Snapshot & Recompute** strategy. The JIT will run at full speed between major checkpoints (e.g., every 10,000 instructions or on system calls). If TTD needs to step backwards, we restore the nearest snapshot and fall back to the **Interpreter** to generate granular instruction-by-instruction states.

## 2. JIT Design Proposals

### A. Compilation Unit & Chaining
- **Basic Block JIT:** We will use a Basic Block (BB) compilation strategy. A BB terminates on branches or page boundaries.
- **Direct Chaining:** To prevent context-switching back to the Rust dispatcher loop, blocks will be chained via direct patching. The dispatcher will maintain an `RwLock<HashMap>` mapping Guest PCs to Host JIT block pointers. Once the destination block is known, the caller's tail jump is overwritten to point directly to the callee.

### B. Code Cache Structure & Invalidation
- **Page-Level Tracking:** Similar to QEMU's `PageDesc`, Fission will map virtual pages to a list of intersecting JIT blocks.
- **Self-Modifying Code (SMC):** Fission's MMU will trap writes to executable pages. Upon a trap, we flush/invalidate the specific blocks overlapping the written range and re-translate on the next execution pass. 
- **Hook Invalidation:** Adding or removing dynamic analysis hooks mid-execution will flush the relevant block to force JIT re-emission with the new hook state.

### C. Host Codegen Backend Choice (Tradeoff)
We must choose how to lower NIR to host machine code:
1. **Direct Emitters (Like QEMU TCG):** Write custom x64/AArch64 assembly emitters in Rust. 
   *Pros:* Zero compilation latency, ultra-fast block emission. *Cons:* High maintenance burden per architecture.
2. **Cranelift JIT:** Use Rust's `cranelift-jit` library.
   *Pros:* Extremely robust register allocation, cross-platform by default. *Cons:* Higher compilation latency per block (might cause stuttering compared to TCG).
*Decision:* We will use **Cranelift** for the initial implementation to ensure maintainability and robust register allocation, falling back to a custom emitter only if compilation latency becomes a proven bottleneck.

## 3. Implementation Milestones

To strictly mitigate risk, we will build from a correctness baseline rather than jumping straight to JIT compilation.

*   **Milestone 1: Correctness Baseline (Interpreter)**
    *   Implement a pure Rust interpreter consuming Sleigh NIR for a single architecture (e.g., x86_64).
    *   Validate execution against Unicorn/QEMU for deterministic outputs.
*   **Milestone 2: Basic Block JIT (Single Arch)**
    *   Integrate Cranelift to compile NIR blocks to host code.
    *   Run blocks in isolation (no chaining) and verify register state transitions match Milestone 1.
*   **Milestone 3: JIT Chaining & Cache Invalidation**
    *   Implement block chaining (direct jumps).
    *   Implement executable page protection and SMC block invalidation.
    *   Implement dynamic hook block flushing.
*   **Milestone 4: Multi-Architecture Extension**
    *   Ensure the NIR-to-Cranelift lowering pipeline handles varying endianness and register sizes for ARM/MIPS.
*   **Milestone 5: TTD Integration**
    *   Integrate the Snapshot & Recompute mechanism to allow time-travel debugging over JIT-executed segments.

## 4. Verification Method

- **Differential Testing:** Fission's CI will implement a differential execution fuzzer. We will step the Fission JIT and Unicorn Engine side-by-side over identical binaries.
- **State Hashing:** After every basic block, we hash the generic register state (GPRs) and dirtied memory pages. Any divergence immediately flags a JIT emission or lowering bug.

## 5. Risks & Challenges

1. **Self-Modifying Code (SMC):** Properly tracking dirty code pages and unwinding the host CPU state when the currently executing block is modified is notoriously difficult and error-prone.
2. **JIT Debugging:** When the JIT emits bad code, the host CPU simply crashes (SIGILL/SIGSEGV). Debugging this requires complex disassembly of dynamically allocated JIT memory.
3. **Register Allocation Overhead:** If Cranelift's compilation overhead is too high for short-lived basic blocks, we may be forced to write a linear-scan register allocator and custom emitters (incurring massive maintenance debt).
4. **Time-Travel Synchronization:** Guaranteeing that the JIT does not corrupt the symbolic memory model (COW memory) and that snapshots remain completely isolated during raw execution.
