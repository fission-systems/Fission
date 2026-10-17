//! GPU projections of the existing stack FIR, without a second semantic IR.
//!
//! One owner (block 0, thread 0 in every dimension) processes one state. This
//! does not model SIMT guest programs or reconstruct a GPU kernel ABI. Storage
//! is global, naturally aligned and mutually disjoint. Host synchronization is
//! required before observing the results. See the crate README for the ABI.

use std::fmt::Write;

use crate::semantics::width_mask;
use crate::{CompiledInstruction, FirOp, FslError, OutputLayer, StackContract};

pub(crate) fn emit_gpu_instruction(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    // Derived from ordered canonical FIR; unknown/state/lane effects and widths
    // above 64 refuse before a target string is returned.
    if crate::control::has_control(instruction) {
        return Err(FslError::at(
            1,
            1,
            "control FIR is unsupported by GPU outputs",
        ));
    }
    let contract = StackContract::for_instruction(instruction)?;
    match layer {
        OutputLayer::CudaCpp => Ok(cuda(instruction, contract, symbol)),
        OutputLayer::Ptx => Ok(ptx(instruction, contract, symbol)),
        _ => Err(FslError::at(1, 1, "expected a GPU output layer")),
    }
}

fn cuda(instruction: &CompiledInstruction, contract: StackContract, symbol: &str) -> String {
    // Inline PTX reads the actual CUDA coordinates. No CUDA runtime headers or
    // hand-authored substitute thread variables are needed for device compile.
    let mut text = String::from("// FSL GPU reference ABI v1; one state, one owning thread.\n// Disjoint valid global buffers; status 0 success, 1 underflow, 2 capacity, 3 invalid state.\n#if defined(__clang__) && defined(__CUDA__)\n#define FSL_GPU_ENTRY __attribute__((global))\n#else\n#define FSL_GPU_ENTRY __global__\n#endif\nstatic_assert(sizeof(unsigned long long) == 8, \"64-bit slots required\");\nstatic_assert(sizeof(unsigned int) == 4, \"32-bit status required\");\n");
    writeln!(text, "extern \"C\" FSL_GPU_ENTRY void {symbol}(unsigned long long *stack, unsigned long long *depth, unsigned long long capacity, unsigned int *status) {{").unwrap();
    text.push_str("    unsigned int owner = 0, coordinate;\n");
    for special in ["ctaid.x", "ctaid.y", "ctaid.z", "tid.x", "tid.y", "tid.z"] {
        writeln!(
            text,
            "    asm(\"mov.u32 %0, %%{special};\" : \"=r\"(coordinate));\n    owner |= coordinate;"
        )
        .unwrap();
    }
    text.push_str("    if (owner != 0 || status == nullptr) return;\n    if (stack == nullptr || depth == nullptr) { *status = 3; return; }\n    unsigned long long sp = *depth;\n    if (sp > capacity) { *status = 3; return; }\n");
    if contract.required_input != 0 {
        writeln!(
            text,
            "    if (sp < {}) {{ *status = 1; return; }}",
            contract.required_input
        )
        .unwrap();
    }
    if contract.extra_capacity != 0 {
        writeln!(
            text,
            "    if ({} > capacity - sp) {{ *status = 2; return; }}",
            contract.extra_capacity
        )
        .unwrap();
    }
    for op in &instruction.ops {
        match *op {
            FirOp::VmStackPop { output } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    unsigned long long v{} = stack[--sp] & 0x{mask:x}ULL;\n    (void)v{};",
                    output.0, output.0
                )
                .unwrap();
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    unsigned long long v{} = (v{} + v{}) & 0x{mask:x}ULL;\n    (void)v{};",
                    output.0, left.0, right.0, output.0
                )
                .unwrap();
            }
            FirOp::VmStackPush { value } => {
                writeln!(text, "    stack[sp++] = v{};", value.0).unwrap();
            }
            _ => unreachable!("stack contract admits only pop, wrapping add and push"),
        }
    }
    text.push_str("    *depth = sp;\n    *status = 0;\n}\n#undef FSL_GPU_ENTRY\n");
    text
}

fn ptx(instruction: &CompiledInstruction, contract: StackContract, symbol: &str) -> String {
    let mut text = format!("// FSL GPU reference ABI v1; valid aligned disjoint global buffers.\n.version 7.0\n.target sm_70\n.address_size 64\n\n.visible .entry {symbol}(\n    .param .u64 stack_ptr,\n    .param .u64 depth_ptr,\n    .param .u64 capacity,\n    .param .u64 status_ptr\n) {{\n    .reg .pred %p;\n    .reg .u32 %owner, %coord, %result;\n    .reg .u64 %stack, %depth, %capacity, %status, %sp, %offset, %addr, %remaining;\n");
    // Use individual declarations, including valid bodies without SSA values.
    for value in &instruction.values {
        writeln!(text, "    .reg .b64 %v{};", value.id.0).unwrap();
    }
    text.push_str("    mov.u32 %owner, 0;\n");
    for special in ["ctaid.x", "ctaid.y", "ctaid.z", "tid.x", "tid.y", "tid.z"] {
        writeln!(
            text,
            "    mov.u32 %coord, %{special};\n    or.b32 %owner, %owner, %coord;"
        )
        .unwrap();
    }
    text.push_str("    setp.ne.u32 %p, %owner, 0;\n    @%p bra DONE;\n    ld.param.u64 %status, [status_ptr];\n    setp.eq.u64 %p, %status, 0;\n    @%p bra DONE;\n    cvta.to.global.u64 %status, %status;\n    mov.u32 %result, 3;\n    ld.param.u64 %stack, [stack_ptr];\n    ld.param.u64 %depth, [depth_ptr];\n    ld.param.u64 %capacity, [capacity];\n    setp.eq.u64 %p, %stack, 0;\n    @%p bra REPORT;\n    setp.eq.u64 %p, %depth, 0;\n    @%p bra REPORT;\n    cvta.to.global.u64 %stack, %stack;\n    cvta.to.global.u64 %depth, %depth;\n    ld.global.u64 %sp, [%depth];\n    setp.gt.u64 %p, %sp, %capacity;\n    @%p bra REPORT;\n");
    if contract.required_input != 0 {
        writeln!(
            text,
            "    mov.u32 %result, 1;\n    setp.lt.u64 %p, %sp, {};\n    @%p bra REPORT;",
            contract.required_input
        )
        .unwrap();
    }
    if contract.extra_capacity != 0 {
        writeln!(text, "    mov.u32 %result, 2;\n    sub.u64 %remaining, %capacity, %sp;\n    setp.lt.u64 %p, %remaining, {};\n    @%p bra REPORT;", contract.extra_capacity).unwrap();
    }
    for op in &instruction.ops {
        match *op {
            FirOp::VmStackPop { output } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(text, "    sub.u64 %sp, %sp, 1;\n    shl.b64 %offset, %sp, 3;\n    add.u64 %addr, %stack, %offset;\n    ld.global.u64 %v{}, [%addr];\n    and.b64 %v{}, %v{}, 0x{mask:x};", output.0, output.0, output.0).unwrap();
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    add.u64 %v{}, %v{}, %v{};\n    and.b64 %v{}, %v{}, 0x{mask:x};",
                    output.0, left.0, right.0, output.0, output.0
                )
                .unwrap();
            }
            FirOp::VmStackPush { value } => {
                writeln!(text, "    shl.b64 %offset, %sp, 3;\n    add.u64 %addr, %stack, %offset;\n    st.global.u64 [%addr], %v{};\n    add.u64 %sp, %sp, 1;", value.0).unwrap();
            }
            _ => unreachable!("stack contract admits only pop, wrapping add and push"),
        }
    }
    text.push_str("    st.global.u64 [%depth], %sp;\n    mov.u32 %result, 0;\nREPORT:\n    st.global.u32 [%status], %result;\nDONE:\n    ret;\n}\n");
    text
}
