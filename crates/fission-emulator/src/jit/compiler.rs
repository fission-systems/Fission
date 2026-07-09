//! Cranelift P-Code → host JIT (sole execution engine).
//!
//! - Multi-instruction translation blocks (TB) with soft direct chaining
//! - Intra-instruction relative BRANCH/CBRANCH via per-op Cranelift blocks
//! - Float ops via host callouts; values >8B via bulk byte callouts
//! - Space ids are whatever SLA assigned (no hardcoded register/ram)

use anyhow::Result;
use cranelift_codegen::ir::{
    types, AbiParam, BlockArg, InstBuilder, StackSlotData, StackSlotKind, condcodes::IntCC,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
use std::collections::HashMap;

use crate::jit::float_ops::{FloatBinOp, FloatUnOp};

/// One guest instruction already lifted to P-Code.
#[derive(Clone, Debug)]
pub struct GuestInsn {
    pub pc: u64,
    pub len: u32,
    pub ops: Vec<PcodeOp>,
}

/// The main JIT compiler context for Fission.
pub struct JitCompiler {
    pub module: JITModule,
    pub ctx: Context,
    pub builder_ctx: FunctionBuilderContext,
    compile_seq: u64,
}

impl JitCompiler {
    pub fn new() -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("opt_level", "speed").unwrap();

        let isa_builder = cranelift_native::builder()
            .map_err(|e| anyhow::anyhow!("Host machine is not supported by Cranelift: {}", e))?;

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| anyhow::anyhow!("Failed to build Cranelift ISA: {}", e))?;

        let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
        for (name, ptr) in [
            ("jit_read_space", crate::jit::callbacks::jit_read_space as *const u8),
            ("jit_write_space", crate::jit::callbacks::jit_write_space as *const u8),
            ("jit_read_bytes", crate::jit::callbacks::jit_read_bytes as *const u8),
            ("jit_write_bytes", crate::jit::callbacks::jit_write_bytes as *const u8),
            ("jit_float_binop", crate::jit::callbacks::jit_float_binop as *const u8),
            ("jit_float_unop", crate::jit::callbacks::jit_float_unop as *const u8),
            ("jit_int_flag", crate::jit::callbacks::jit_int_flag as *const u8),
            ("jit_call_other", crate::jit::callbacks::jit_call_other as *const u8),
            ("jit_count_insn", crate::jit::callbacks::jit_count_insn as *const u8),
            ("jit_chain", crate::jit::callbacks::jit_chain as *const u8),
            ("jit_exit_tb", crate::jit::callbacks::jit_exit_tb as *const u8),
            ("jit_read_register", crate::jit::callbacks::jit_read_register as *const u8),
            ("jit_write_register", crate::jit::callbacks::jit_write_register as *const u8),
            ("jit_read_memory", crate::jit::callbacks::jit_read_memory as *const u8),
            ("jit_write_memory", crate::jit::callbacks::jit_write_memory as *const u8),
        ] {
            jit_builder.symbol(name, ptr);
        }

        let module = JITModule::new(jit_builder);
        let ctx = module.make_context();
        let builder_ctx = FunctionBuilderContext::new();

        Ok(Self {
            module,
            ctx,
            builder_ctx,
            compile_seq: 0,
        })
    }

    /// Compile a single guest instruction (TB of length 1).
    pub fn compile_basic_block(
        &mut self,
        pc: u64,
        inst_len: u32,
        ops: &[PcodeOp],
    ) -> Result<*const u8> {
        self.compile_translation_block(&[GuestInsn {
            pc,
            len: inst_len,
            ops: ops.to_vec(),
        }])
    }

    /// Compile a multi-instruction translation block.
    ///
    /// Host signature: `extern "C" fn(*mut Emulator) -> u64` → final next PC
    /// (after hard/soft chaining via global chain table).
    pub fn compile_translation_block(&mut self, insns: &[GuestInsn]) -> Result<*const u8> {
        anyhow::ensure!(!insns.is_empty(), "empty translation block");

        let start_pc = insns[0].pc;

        // Flatten ops with remapped relative branch targets.
        // Also record which global op index starts each guest instruction.
        let mut flat: Vec<PcodeOp> = Vec::new();
        let mut insn_starts: Vec<(usize, u64, u32)> = Vec::new(); // (op_index, pc, len)

        for insn in insns {
            let base = flat.len();
            insn_starts.push((base, insn.pc, insn.len));
            for op in &insn.ops {
                let mut op = op.clone();
                remap_relative_branches(&mut op, base);
                flat.push(op);
            }
        }

        let fallthrough_pc = {
            let last = insns.last().unwrap();
            last.pc.wrapping_add(last.len as u64)
        };

        self.ctx.func.signature.params.clear();
        self.ctx.func.signature.returns.clear();
        self.ctx.func.signature.params.push(AbiParam::new(types::I64));
        self.ctx.func.signature.returns.push(AbiParam::new(types::I64));

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);

        // ── Imports ──────────────────────────────────────────────────────────
        let mut sig_rw = self.module.make_signature();
        sig_rw.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        sig_rw.returns.push(AbiParam::new(types::I64));
        let read_space_fn = self
            .module
            .declare_function("jit_read_space", Linkage::Import, &sig_rw)
            .unwrap();

        let mut sig_ww = self.module.make_signature();
        sig_ww.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        let write_space_fn = self
            .module
            .declare_function("jit_write_space", Linkage::Import, &sig_ww)
            .unwrap();

        let mut sig_bytes = self.module.make_signature();
        sig_bytes.params.extend([
            AbiParam::new(types::I64), // emu
            AbiParam::new(types::I64), // space
            AbiParam::new(types::I64), // offset
            AbiParam::new(types::I64), // ptr
            AbiParam::new(types::I64), // size
        ]);
        let read_bytes_fn = self
            .module
            .declare_function("jit_read_bytes", Linkage::Import, &sig_bytes)
            .unwrap();
        let write_bytes_fn = self
            .module
            .declare_function("jit_write_bytes", Linkage::Import, &sig_bytes)
            .unwrap();

        let mut sig_fbin = self.module.make_signature();
        sig_fbin.params.extend([
            AbiParam::new(types::I32),
            AbiParam::new(types::I32),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        sig_fbin.returns.push(AbiParam::new(types::I64));
        let float_binop_fn = self
            .module
            .declare_function("jit_float_binop", Linkage::Import, &sig_fbin)
            .unwrap();

        let mut sig_fun = self.module.make_signature();
        sig_fun.params.extend([
            AbiParam::new(types::I32),
            AbiParam::new(types::I32),
            AbiParam::new(types::I32),
            AbiParam::new(types::I64),
        ]);
        sig_fun.returns.push(AbiParam::new(types::I64));
        let float_unop_fn = self
            .module
            .declare_function("jit_float_unop", Linkage::Import, &sig_fun)
            .unwrap();

        let mut sig_iflag = self.module.make_signature();
        sig_iflag.params.extend([
            AbiParam::new(types::I32), // kind
            AbiParam::new(types::I32), // size
            AbiParam::new(types::I64), // a
            AbiParam::new(types::I64), // b
        ]);
        sig_iflag.returns.push(AbiParam::new(types::I64));
        let int_flag_fn = self
            .module
            .declare_function("jit_int_flag", Linkage::Import, &sig_iflag)
            .unwrap();

        let mut sig_callother = self.module.make_signature();
        sig_callother.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I32),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        sig_callother.returns.push(AbiParam::new(types::I64));
        let call_other_fn = self
            .module
            .declare_function("jit_call_other", Linkage::Import, &sig_callother)
            .unwrap();

        let mut sig_count = self.module.make_signature();
        sig_count.params.push(AbiParam::new(types::I64));
        let count_fn = self
            .module
            .declare_function("jit_count_insn", Linkage::Import, &sig_count)
            .unwrap();

        let mut sig_exit = self.module.make_signature();
        sig_exit.params.push(AbiParam::new(types::I64)); // emu
        sig_exit.params.push(AbiParam::new(types::I64)); // next_pc
        sig_exit.returns.push(AbiParam::new(types::I64));
        let exit_tb_fn = self
            .module
            .declare_function("jit_exit_tb", Linkage::Import, &sig_exit)
            .unwrap();

        // ── Blocks ───────────────────────────────────────────────────────────
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);

        let n_ops = flat.len();
        let mut op_blocks = Vec::with_capacity(n_ops.max(1));
        for _ in 0..n_ops {
            op_blocks.push(builder.create_block());
        }
        let exit_block = builder.create_block();
        builder.append_block_param(exit_block, types::I64);

        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let emu_ptr = builder.block_params(entry)[0];

        let read_space_ref = self.module.declare_func_in_func(read_space_fn, builder.func);
        let write_space_ref = self.module.declare_func_in_func(write_space_fn, builder.func);
        let read_bytes_ref = self.module.declare_func_in_func(read_bytes_fn, builder.func);
        let write_bytes_ref = self.module.declare_func_in_func(write_bytes_fn, builder.func);
        let float_binop_ref = self.module.declare_func_in_func(float_binop_fn, builder.func);
        let float_unop_ref = self.module.declare_func_in_func(float_unop_fn, builder.func);
        let int_flag_ref = self.module.declare_func_in_func(int_flag_fn, builder.func);
        let call_other_ref = self.module.declare_func_in_func(call_other_fn, builder.func);
        let count_ref = self.module.declare_func_in_func(count_fn, builder.func);
        let exit_tb_ref = self.module.declare_func_in_func(exit_tb_fn, builder.func);

        let default_next = builder.ins().iconst(types::I64, fallthrough_pc as i64);

        let mut var_map: HashMap<(u64, u64), Variable> = HashMap::new();
        let mut dirty: Vec<(u64, u64, u32, Variable)> = Vec::new();

        // Map insn start op-index → guest len for count call emission.
        let insn_start_set: HashMap<usize, ()> =
            insn_starts.iter().map(|(i, _, _)| (*i, ())).collect();

        macro_rules! ensure_var {
            ($space:expr, $offset:expr, $size:expr) => {{
                let key = ($space, $offset);
                if let Some(v) = var_map.get(&key) {
                    *v
                } else {
                    let v = builder.declare_var(types::I64);
                    var_map.insert(key, v);
                    if $space == 0 {
                        let c = builder.ins().iconst(types::I64, $offset as i64);
                        builder.def_var(v, c);
                    } else {
                        let sp = builder.ins().iconst(types::I64, $space as i64);
                        let off = builder.ins().iconst(types::I64, $offset as i64);
                        let sz = builder
                            .ins()
                            .iconst(types::I64, ($size as i64).min(8));
                        let call =
                            builder
                                .ins()
                                .call(read_space_ref, &[emu_ptr, sp, off, sz]);
                        let val = builder.inst_results(call)[0];
                        builder.def_var(v, val);
                    }
                    v
                }
            }};
        }

        macro_rules! load_vn {
            ($vn:expr) => {{
                let vn: &Varnode = $vn;
                if vn.is_constant {
                    builder.ins().iconst(types::I64, vn.constant_val as i64)
                } else if vn.size > 8 {
                    // Wide: load low 8 bytes only for scalar ops; bulk path for Copy/Load/Store.
                    let v = ensure_var!(vn.space_id, vn.offset, 8);
                    builder.use_var(v)
                } else {
                    let v = ensure_var!(vn.space_id, vn.offset, vn.size);
                    builder.use_var(v)
                }
            }};
        }

        macro_rules! store_vn {
            ($vn:expr, $val:expr) => {{
                let vn: &Varnode = $vn;
                let val = $val;
                if !vn.is_constant {
                    let v = ensure_var!(vn.space_id, vn.offset, vn.size.min(8));
                    builder.def_var(v, val);
                    dirty.push((vn.space_id, vn.offset, vn.size.min(8), v));
                }
            }};
        }

        if n_ops == 0 {
            // Still count guest insns and exit.
            for _ in insns {
                builder.ins().call(count_ref, &[emu_ptr]);
            }
            let arg = BlockArg::from(default_next);
            builder.ins().jump(exit_block, &[arg]);
        } else {
            builder.ins().jump(op_blocks[0], &[]);
        }

        for (idx, op) in flat.iter().enumerate() {
            builder.switch_to_block(op_blocks[idx]);

            // Guest-insn boundary accounting.
            if insn_start_set.contains_key(&idx) {
                builder.ins().call(count_ref, &[emu_ptr]);
            }

            let fallthrough = if idx + 1 < n_ops {
                Some(op_blocks[idx + 1])
            } else {
                None
            };
            let mut branched = false;

            match op.opcode {
                PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    if let Some(out) = op.output.as_ref() {
                        if !op.inputs.is_empty() {
                            let src = &op.inputs[0];
                            if out.size > 8 || src.size > 8 {
                                // Bulk copy via stack buffer.
                                let sz = out.size.max(src.size) as usize;
                                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                    StackSlotKind::ExplicitSlot,
                                    sz as u32,
                                    0,
                                ));
                                let ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                let sp = builder.ins().iconst(types::I64, src.space_id as i64);
                                let off = if src.is_constant {
                                    builder.ins().iconst(types::I64, src.offset as i64)
                                } else {
                                    builder.ins().iconst(types::I64, src.offset as i64)
                                };
                                // For constants, write bytes manually; for vars, read_bytes.
                                if src.is_constant {
                                    // store constant low bytes into slot via store sequence (max 8)
                                    let c = builder
                                        .ins()
                                        .iconst(types::I64, src.constant_val as i64);
                                    builder.ins().stack_store(c, slot, 0);
                                } else {
                                    let szv = builder.ins().iconst(types::I64, src.size as i64);
                                    builder.ins().call(
                                        read_bytes_ref,
                                        &[emu_ptr, sp, off, ptr, szv],
                                    );
                                }
                                let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                                let doff = builder.ins().iconst(types::I64, out.offset as i64);
                                let dsz = builder.ins().iconst(types::I64, out.size as i64);
                                builder.ins().call(
                                    write_bytes_ref,
                                    &[emu_ptr, dsp, doff, ptr, dsz],
                                );
                            } else {
                                let val = load_vn!(src);
                                store_vn!(out, val);
                            }
                        }
                    }
                }

                PcodeOpcode::Load => {
                    if let Some(out) = op.output.as_ref() {
                        if op.inputs.len() >= 2 {
                            let space_id = space_const(&op.inputs[0]);
                            let addr = load_vn!(&op.inputs[1]);
                            if out.size > 8 {
                                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                    StackSlotKind::ExplicitSlot,
                                    out.size,
                                    0,
                                ));
                                let ptr = builder.ins().stack_addr(types::I64, slot, 0);
                                let sp = builder.ins().iconst(types::I64, space_id as i64);
                                let sz = builder.ins().iconst(types::I64, out.size as i64);
                                builder
                                    .ins()
                                    .call(read_bytes_ref, &[emu_ptr, sp, addr, ptr, sz]);
                                let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                                let doff = builder.ins().iconst(types::I64, out.offset as i64);
                                builder
                                    .ins()
                                    .call(write_bytes_ref, &[emu_ptr, dsp, doff, ptr, sz]);
                            } else {
                                let sp = builder.ins().iconst(types::I64, space_id as i64);
                                let sz = builder.ins().iconst(types::I64, out.size as i64);
                                let call =
                                    builder
                                        .ins()
                                        .call(read_space_ref, &[emu_ptr, sp, addr, sz]);
                                let val = builder.inst_results(call)[0];
                                store_vn!(out, val);
                            }
                        }
                    }
                }

                PcodeOpcode::Store => {
                    if op.inputs.len() >= 3 {
                        let space_id = space_const(&op.inputs[0]);
                        let addr = load_vn!(&op.inputs[1]);
                        let val_vn = &op.inputs[2];
                        if val_vn.size > 8 {
                            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                val_vn.size,
                                0,
                            ));
                            let ptr = builder.ins().stack_addr(types::I64, slot, 0);
                            if !val_vn.is_constant {
                                let sp = builder.ins().iconst(types::I64, val_vn.space_id as i64);
                                let off = builder.ins().iconst(types::I64, val_vn.offset as i64);
                                let sz = builder.ins().iconst(types::I64, val_vn.size as i64);
                                builder
                                    .ins()
                                    .call(read_bytes_ref, &[emu_ptr, sp, off, ptr, sz]);
                            }
                            let dsp = builder.ins().iconst(types::I64, space_id as i64);
                            let sz = builder.ins().iconst(types::I64, val_vn.size as i64);
                            builder
                                .ins()
                                .call(write_bytes_ref, &[emu_ptr, dsp, addr, ptr, sz]);
                        } else {
                            let val = load_vn!(val_vn);
                            let sp = builder.ins().iconst(types::I64, space_id as i64);
                            let sz = builder.ins().iconst(types::I64, val_vn.size as i64);
                            builder
                                .ins()
                                .call(write_space_ref, &[emu_ptr, sp, addr, sz, val]);
                        }
                    }
                }

                PcodeOpcode::IntAdd => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().iadd(a, b));
                    }
                }
                // INT_CARRY / INT_SCARRY / INT_SBORROW — size-aware via host callout.
                PcodeOpcode::IntCarry | PcodeOpcode::IntSCarry | PcodeOpcode::IntSBorrow => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        let kind = match op.opcode {
                            PcodeOpcode::IntCarry => 0i64,
                            PcodeOpcode::IntSCarry => 1,
                            _ => 2,
                        };
                        let size = op.inputs[0].size.max(1) as i64;
                        let k = builder.ins().iconst(types::I32, kind);
                        let s = builder.ins().iconst(types::I32, size);
                        let call = builder.ins().call(int_flag_ref, &[k, s, a, b]);
                        store_vn!(out, builder.inst_results(call)[0]);
                    }
                }
                PcodeOpcode::IntSub | PcodeOpcode::PtrSub => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().isub(a, b));
                    }
                }
                PcodeOpcode::IntMult => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().imul(a, b));
                    }
                }
                PcodeOpcode::IntDiv => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().udiv(a, b));
                    }
                }
                PcodeOpcode::IntSDiv => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().sdiv(a, b));
                    }
                }
                PcodeOpcode::IntRem => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().urem(a, b));
                    }
                }
                PcodeOpcode::IntSRem => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().srem(a, b));
                    }
                }
                PcodeOpcode::IntAnd | PcodeOpcode::BoolAnd => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().band(a, b));
                    }
                }
                PcodeOpcode::IntOr | PcodeOpcode::BoolOr => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().bor(a, b));
                    }
                }
                PcodeOpcode::IntXor | PcodeOpcode::BoolXor => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().bxor(a, b));
                    }
                }
                PcodeOpcode::IntLeft => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().ishl(a, b));
                    }
                }
                PcodeOpcode::IntRight => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().ushr(a, b));
                    }
                }
                PcodeOpcode::IntSRight => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().sshr(a, b));
                    }
                }
                PcodeOpcode::IntNegate => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().bnot(a));
                    }
                }
                PcodeOpcode::Int2Comp => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().ineg(a));
                    }
                }
                PcodeOpcode::BoolNegate => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().bxor_imm(a, 1));
                    }
                }
                PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        let cc = match op.opcode {
                            PcodeOpcode::IntEqual => IntCC::Equal,
                            PcodeOpcode::IntNotEqual => IntCC::NotEqual,
                            PcodeOpcode::IntSLess => IntCC::SignedLessThan,
                            PcodeOpcode::IntSLessEqual => IntCC::SignedLessThanOrEqual,
                            PcodeOpcode::IntLess => IntCC::UnsignedLessThan,
                            PcodeOpcode::IntLessEqual => IntCC::UnsignedLessThanOrEqual,
                            _ => unreachable!(),
                        };
                        let b_res = builder.ins().icmp(cc, a, b);
                        store_vn!(out, builder.ins().uextend(types::I64, b_res));
                    }
                }
                PcodeOpcode::IntZExt => {
                    if let Some(out) = op.output.as_ref() {
                        let in_vn = &op.inputs[0];
                        let val = load_vn!(in_vn);
                        let bits = (in_vn.size as u64).saturating_mul(8).min(63);
                        let mask = if bits >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << bits) - 1
                        };
                        store_vn!(out, builder.ins().band_imm(val, mask as i64));
                    }
                }
                PcodeOpcode::IntSExt => {
                    if let Some(out) = op.output.as_ref() {
                        let in_vn = &op.inputs[0];
                        let val = load_vn!(in_vn);
                        let shift = 64i64 - (in_vn.size as i64 * 8);
                        if shift > 0 && shift < 64 {
                            let s = builder.ins().ishl_imm(val, shift);
                            store_vn!(out, builder.ins().sshr_imm(s, shift));
                        } else {
                            store_vn!(out, val);
                        }
                    }
                }
                PcodeOpcode::SubPiece => {
                    if let Some(out) = op.output.as_ref() {
                        let val = load_vn!(&op.inputs[0]);
                        let shift_bytes = if op.inputs.len() > 1 && op.inputs[1].is_constant {
                            op.inputs[1].constant_val as i64
                        } else {
                            0
                        };
                        let shifted = if shift_bytes > 0 {
                            builder.ins().ushr_imm(val, shift_bytes * 8)
                        } else {
                            val
                        };
                        let bits = (out.size as u64).saturating_mul(8).min(63);
                        let mask = if bits >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << bits) - 1
                        };
                        store_vn!(out, builder.ins().band_imm(shifted, mask as i64));
                    }
                }
                // PIECE: concatenate high || low  →  (high << low_bits) | low
                PcodeOpcode::Piece => {
                    if let Some(out) = op.output.as_ref() {
                        let high = load_vn!(&op.inputs[0]);
                        let low = load_vn!(&op.inputs[1]);
                        let low_bits = (op.inputs[1].size as i64).saturating_mul(8).min(63);
                        let shifted = if low_bits > 0 {
                            builder.ins().ishl_imm(high, low_bits)
                        } else {
                            high
                        };
                        store_vn!(out, builder.ins().bor(shifted, low));
                    }
                }
                // EXTRACT: (val >> offset) truncated to out.size
                PcodeOpcode::Extract => {
                    if let Some(out) = op.output.as_ref() {
                        let val = load_vn!(&op.inputs[0]);
                        let offset_bits = if op.inputs.len() > 1 {
                            if op.inputs[1].is_constant {
                                op.inputs[1].constant_val as i64
                            } else {
                                // dynamic offset: use runtime value (clamped later by mask)
                                0
                            }
                        } else {
                            0
                        };
                        let shifted = if op.inputs.len() > 1 && !op.inputs[1].is_constant {
                            let off = load_vn!(&op.inputs[1]);
                            builder.ins().ushr(val, off)
                        } else if offset_bits > 0 {
                            builder.ins().ushr_imm(val, offset_bits)
                        } else {
                            val
                        };
                        let bits = (out.size as u64).saturating_mul(8).min(63);
                        let mask = if bits >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << bits) - 1
                        };
                        store_vn!(out, builder.ins().band_imm(shifted, mask as i64));
                    }
                }
                // INSERT: dest with `src` bits inserted at offset for `size` bits
                PcodeOpcode::Insert => {
                    if let Some(out) = op.output.as_ref() {
                        // Ghidra INSERT: inputs = dest, src, position, size (const)
                        let dest = load_vn!(&op.inputs[0]);
                        let src = load_vn!(&op.inputs[1]);
                        let pos = if op.inputs.len() > 2 && op.inputs[2].is_constant {
                            op.inputs[2].constant_val as u32
                        } else {
                            0
                        };
                        let nbits = if op.inputs.len() > 3 && op.inputs[3].is_constant {
                            op.inputs[3].constant_val as u32
                        } else {
                            (op.inputs[1].size * 8).min(64)
                        };
                        let nbits = nbits.min(64);
                        let mask = if nbits >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << nbits) - 1
                        };
                        let clear_mask = !(mask.wrapping_shl(pos));
                        let cleared = builder.ins().band_imm(dest, clear_mask as i64);
                        let src_m = builder.ins().band_imm(src, mask as i64);
                        let inserted = if pos > 0 {
                            builder.ins().ishl_imm(src_m, pos as i64)
                        } else {
                            src_m
                        };
                        store_vn!(out, builder.ins().bor(cleared, inserted));
                    }
                }
                // LZCOUNT: leading zero bits in the input (width = input size * 8)
                PcodeOpcode::LzCount => {
                    if let Some(out) = op.output.as_ref() {
                        let val = load_vn!(&op.inputs[0]);
                        let width = (op.inputs[0].size as i64).saturating_mul(8).min(64);
                        // clz on host 64-bit, then adjust for smaller widths:
                        // lzcount_n(x) = clz64(x) - (64 - n)  when x != 0; else n
                        let clz = builder.ins().clz(val);
                        let adj = builder.ins().iconst(types::I64, 64 - width);
                        let adj_clz = builder.ins().isub(clz, adj);
                        let is_zero = builder.ins().icmp_imm(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            val,
                            0,
                        );
                        let full = builder.ins().iconst(types::I64, width);
                        let res = builder.ins().select(is_zero, full, adj_clz);
                        store_vn!(out, res);
                    }
                }
                PcodeOpcode::SegmentOp => {
                    // Treat as base + offset (segment calc simplified).
                    if let Some(out) = op.output.as_ref() {
                        if op.inputs.len() >= 2 {
                            let base = load_vn!(&op.inputs[0]);
                            let off = load_vn!(&op.inputs[1]);
                            store_vn!(out, builder.ins().iadd(base, off));
                        }
                    }
                }
                PcodeOpcode::PtrAdd => {
                    if let Some(out) = op.output.as_ref() {
                        let ptr = load_vn!(&op.inputs[0]);
                        let off = load_vn!(&op.inputs[1]);
                        let mul = if op.inputs.len() > 2 {
                            load_vn!(&op.inputs[2])
                        } else {
                            builder.ins().iconst(types::I64, 1)
                        };
                        let scaled = builder.ins().imul(off, mul);
                        store_vn!(out, builder.ins().iadd(ptr, scaled));
                    }
                }
                PcodeOpcode::PopCount => {
                    if let Some(out) = op.output.as_ref() {
                        let x0 = load_vn!(&op.inputs[0]);
                        let m1 = builder
                            .ins()
                            .iconst(types::I64, 0x5555_5555_5555_5555u64 as i64);
                        let m2 = builder
                            .ins()
                            .iconst(types::I64, 0x3333_3333_3333_3333u64 as i64);
                        let m4 = builder
                            .ins()
                            .iconst(types::I64, 0x0f0f_0f0f_0f0f_0f0fu64 as i64);
                        let h01 = builder
                            .ins()
                            .iconst(types::I64, 0x0101_0101_0101_0101u64 as i64);
                        let s1 = builder.ins().ushr_imm(x0, 1);
                        let t1 = builder.ins().band(s1, m1);
                        let x1 = builder.ins().isub(x0, t1);
                        let s2 = builder.ins().ushr_imm(x1, 2);
                        let t2 = builder.ins().band(s2, m2);
                        let b2 = builder.ins().band(x1, m2);
                        let x2 = builder.ins().iadd(b2, t2);
                        let s4 = builder.ins().ushr_imm(x2, 4);
                        let a4 = builder.ins().iadd(x2, s4);
                        let x3 = builder.ins().band(a4, m4);
                        let x4 = builder.ins().imul(x3, h01);
                        store_vn!(out, builder.ins().ushr_imm(x4, 56));
                    }
                }

                // ── Float (host callouts) ────────────────────────────────────
                PcodeOpcode::FloatAdd
                | PcodeOpcode::FloatSub
                | PcodeOpcode::FloatMult
                | PcodeOpcode::FloatDiv
                | PcodeOpcode::FloatEqual
                | PcodeOpcode::FloatNotEqual
                | PcodeOpcode::FloatLess
                | PcodeOpcode::FloatLessEqual => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        let size = op.inputs[0].size;
                        let fop = match op.opcode {
                            PcodeOpcode::FloatAdd => FloatBinOp::Add,
                            PcodeOpcode::FloatSub => FloatBinOp::Sub,
                            PcodeOpcode::FloatMult => FloatBinOp::Mul,
                            PcodeOpcode::FloatDiv => FloatBinOp::Div,
                            PcodeOpcode::FloatEqual => FloatBinOp::Equal,
                            PcodeOpcode::FloatNotEqual => FloatBinOp::NotEqual,
                            PcodeOpcode::FloatLess => FloatBinOp::Less,
                            PcodeOpcode::FloatLessEqual => FloatBinOp::LessEqual,
                            _ => unreachable!(),
                        };
                        let opi = builder.ins().iconst(types::I32, fop as i64);
                        let szi = builder.ins().iconst(types::I32, size as i64);
                        let call =
                            builder
                                .ins()
                                .call(float_binop_ref, &[opi, szi, a, b]);
                        store_vn!(out, builder.inst_results(call)[0]);
                    }
                }
                PcodeOpcode::FloatNeg
                | PcodeOpcode::FloatAbs
                | PcodeOpcode::FloatSqrt
                | PcodeOpcode::FloatNan
                | PcodeOpcode::FloatCeil
                | PcodeOpcode::FloatFloor
                | PcodeOpcode::FloatRound
                | PcodeOpcode::FloatTrunc
                | PcodeOpcode::FloatInt2Float
                | PcodeOpcode::FloatFloat2Float => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let in_sz = op.inputs[0].size;
                        let out_sz = out.size;
                        let fop = match op.opcode {
                            PcodeOpcode::FloatNeg => FloatUnOp::Neg,
                            PcodeOpcode::FloatAbs => FloatUnOp::Abs,
                            PcodeOpcode::FloatSqrt => FloatUnOp::Sqrt,
                            PcodeOpcode::FloatNan => FloatUnOp::Nan,
                            PcodeOpcode::FloatCeil => FloatUnOp::Ceil,
                            PcodeOpcode::FloatFloor => FloatUnOp::Floor,
                            PcodeOpcode::FloatRound => FloatUnOp::Round,
                            PcodeOpcode::FloatTrunc => FloatUnOp::Trunc,
                            PcodeOpcode::FloatInt2Float => FloatUnOp::Int2Float,
                            PcodeOpcode::FloatFloat2Float => FloatUnOp::Float2Float,
                            _ => unreachable!(),
                        };
                        let opi = builder.ins().iconst(types::I32, fop as i64);
                        let isz = builder.ins().iconst(types::I32, in_sz as i64);
                        let osz = builder.ins().iconst(types::I32, out_sz as i64);
                        let call =
                            builder
                                .ins()
                                .call(float_unop_ref, &[opi, isz, osz, a]);
                        store_vn!(out, builder.inst_results(call)[0]);
                    }
                }

                PcodeOpcode::Branch => {
                    let dest = &op.inputs[0];
                    if dest.space_id == 0 || dest.is_constant {
                        let rel = dest.constant_val as usize;
                        if rel < n_ops {
                            builder.ins().jump(op_blocks[rel], &[]);
                        } else {
                            builder
                                .ins()
                                .jump(exit_block, &[BlockArg::from(default_next)]);
                        }
                        branched = true;
                    } else {
                        let target = builder.ins().iconst(types::I64, dest.offset as i64);
                        builder
                            .ins()
                            .jump(exit_block, &[BlockArg::from(target)]);
                        branched = true;
                    }
                }

                PcodeOpcode::CBranch => {
                    let dest = &op.inputs[0];
                    let cond = load_vn!(&op.inputs[1]);
                    let is_true = builder.ins().icmp_imm(IntCC::NotEqual, cond, 0);

                    if dest.space_id == 0 || dest.is_constant {
                        let rel = dest.constant_val as usize;
                        let taken = if rel < n_ops {
                            op_blocks[rel]
                        } else {
                            exit_block
                        };
                        let not_taken = fallthrough.unwrap_or(exit_block);
                        emit_cond_branch(
                            &mut builder,
                            is_true,
                            taken,
                            not_taken,
                            exit_block,
                            default_next,
                        );
                        branched = true;
                    } else {
                        let target = builder.ins().iconst(types::I64, dest.offset as i64);
                        let then_b = builder.create_block();
                        let else_b = builder.create_block();
                        builder.ins().brif(is_true, then_b, &[], else_b, &[]);
                        builder.switch_to_block(then_b);
                        builder.seal_block(then_b);
                        builder
                            .ins()
                            .jump(exit_block, &[BlockArg::from(target)]);
                        builder.switch_to_block(else_b);
                        builder.seal_block(else_b);
                        if let Some(ft) = fallthrough {
                            builder.ins().jump(ft, &[]);
                        } else {
                            builder
                                .ins()
                                .jump(exit_block, &[BlockArg::from(default_next)]);
                        }
                        branched = true;
                    }
                }

                // Direct CALL: destination varnode *is* the code address (offset),
                // not a memory load. (CALLIND / BRANCHIND / RETURN load a value.)
                PcodeOpcode::Call => {
                    let dest = &op.inputs[0];
                    let addr = if dest.is_constant {
                        dest.constant_val as u64
                    } else {
                        // Ghidra: (ram, off, size) means jump *to* `off`.
                        dest.offset
                    };
                    let target = builder.ins().iconst(types::I64, addr as i64);
                    builder
                        .ins()
                        .jump(exit_block, &[BlockArg::from(target)]);
                    branched = true;
                }
                PcodeOpcode::CallInd | PcodeOpcode::BranchInd | PcodeOpcode::Return => {
                    let target = load_vn!(&op.inputs[0]);
                    builder
                        .ins()
                        .jump(exit_block, &[BlockArg::from(target)]);
                    branched = true;
                }

                PcodeOpcode::CallOther => {
                    let userop_id = if let Some(vn) = op.inputs.first() {
                        if vn.is_constant {
                            vn.constant_val as u32
                        } else {
                            vn.offset as u32
                        }
                    } else {
                        0
                    };
                    let argc = op.inputs.len().saturating_sub(1);
                    let args_ptr = if argc == 0 {
                        builder.ins().iconst(types::I64, 0)
                    } else {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            (argc * 8) as u32,
                            0,
                        ));
                        for (i, vn) in op.inputs.iter().skip(1).enumerate() {
                            let v = load_vn!(vn);
                            builder.ins().stack_store(v, slot, (i * 8) as i32);
                        }
                        builder.ins().stack_addr(types::I64, slot, 0)
                    };
                    let uid = builder.ins().iconst(types::I32, userop_id as i64);
                    let argc_v = builder.ins().iconst(types::I64, argc as i64);
                    let out_sz = builder.ins().iconst(
                        types::I64,
                        op.output.as_ref().map(|o| o.size as i64).unwrap_or(0),
                    );
                    let call = builder.ins().call(
                        call_other_ref,
                        &[emu_ptr, uid, args_ptr, argc_v, out_sz],
                    );
                    let halt = builder.inst_results(call)[0];
                    let is_halt = builder.ins().icmp_imm(IntCC::NotEqual, halt, 0);
                    let halt_b = builder.create_block();
                    let cont_b = builder.create_block();
                    builder.ins().brif(is_halt, halt_b, &[], cont_b, &[]);
                    builder.switch_to_block(halt_b);
                    builder.seal_block(halt_b);
                    builder
                        .ins()
                        .jump(exit_block, &[BlockArg::from(default_next)]);
                    builder.switch_to_block(cont_b);
                    builder.seal_block(cont_b);
                    if let Some(ft) = fallthrough {
                        builder.ins().jump(ft, &[]);
                    } else {
                        builder
                            .ins()
                            .jump(exit_block, &[BlockArg::from(default_next)]);
                    }
                    branched = true;
                }

                PcodeOpcode::MultiEqual | PcodeOpcode::Indirect => {
                    if let Some(out) = op.output.as_ref() {
                        if !op.inputs.is_empty() {
                            let val = load_vn!(&op.inputs[0]);
                            store_vn!(out, val);
                        }
                    }
                }

                _ => {
                    tracing::warn!(
                        "JIT: unimplemented opcode {:?} in TB@0x{:X} (no-op)",
                        op.opcode,
                        start_pc
                    );
                }
            }

            if !branched {
                if let Some(ft) = fallthrough {
                    builder.ins().jump(ft, &[]);
                } else {
                    builder
                        .ins()
                        .jump(exit_block, &[BlockArg::from(default_next)]);
                }
            }
            builder.seal_block(op_blocks[idx]);
        }

        // ── Exit: writeback + soft chain ─────────────────────────────────────
        builder.switch_to_block(exit_block);
        builder.seal_block(exit_block);
        let next_pc = builder.block_params(exit_block)[0];

        let mut last: HashMap<(u64, u64), (u32, Variable)> = HashMap::new();
        for (sp, off, sz, v) in dirty {
            last.insert((sp, off), (sz, v));
        }
        for ((sp, off), (sz, v)) in last {
            if sp == 0 {
                continue;
            }
            let val = builder.use_var(v);
            let spv = builder.ins().iconst(types::I64, sp as i64);
            let offv = builder.ins().iconst(types::I64, off as i64);
            let szv = builder.ins().iconst(types::I64, sz as i64);
            builder
                .ins()
                .call(write_space_ref, &[emu_ptr, spv, offv, szv, val]);
        }

        // Hard/soft TB exit — next_pc may be fallthrough or absolute branch/call.
        let call = builder.ins().call(exit_tb_ref, &[emu_ptr, next_pc]);
        let final_pc = builder.inst_results(call)[0];
        builder.ins().return_(&[final_pc]);
        builder.finalize();

        self.compile_seq = self.compile_seq.wrapping_add(1);
        let name = format!("jit_tb_{:X}_{}", start_pc, self.compile_seq);
        let id = self
            .module
            .declare_function(&name, Linkage::Export, &self.ctx.func.signature)
            .map_err(|e| anyhow::anyhow!("declare: {e}"))?;
        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| anyhow::anyhow!("define: {e}"))?;
        self.module.clear_context(&mut self.ctx);
        self.module
            .finalize_definitions()
            .map_err(|e| anyhow::anyhow!("finalize: {e}"))?;
        Ok(self.module.get_finalized_function(id))
    }
}

fn space_const(vn: &Varnode) -> u64 {
    if vn.is_constant {
        vn.constant_val as u64
    } else {
        vn.offset
    }
}

fn remap_relative_branches(op: &mut PcodeOp, base: usize) {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            if let Some(dest) = op.inputs.first_mut() {
                if dest.space_id == 0 || dest.is_constant {
                    dest.constant_val = (dest.constant_val as usize + base) as i64;
                    dest.offset = dest.constant_val as u64;
                }
            }
        }
        _ => {}
    }
}

fn emit_cond_branch(
    builder: &mut FunctionBuilder,
    is_true: cranelift_codegen::ir::Value,
    taken: cranelift_codegen::ir::Block,
    not_taken: cranelift_codegen::ir::Block,
    exit_block: cranelift_codegen::ir::Block,
    default_next: cranelift_codegen::ir::Value,
) {
    if taken == exit_block && not_taken == exit_block {
        builder
            .ins()
            .jump(exit_block, &[BlockArg::from(default_next)]);
        return;
    }
    if taken == exit_block {
        let then_b = builder.create_block();
        let else_b = builder.create_block();
        builder.ins().brif(is_true, then_b, &[], else_b, &[]);
        builder.switch_to_block(then_b);
        builder.seal_block(then_b);
        builder
            .ins()
            .jump(exit_block, &[BlockArg::from(default_next)]);
        builder.switch_to_block(else_b);
        builder.seal_block(else_b);
        builder.ins().jump(not_taken, &[]);
        return;
    }
    if not_taken == exit_block {
        let then_b = builder.create_block();
        let else_b = builder.create_block();
        builder.ins().brif(is_true, then_b, &[], else_b, &[]);
        builder.switch_to_block(then_b);
        builder.seal_block(then_b);
        builder.ins().jump(taken, &[]);
        builder.switch_to_block(else_b);
        builder.seal_block(else_b);
        builder
            .ins()
            .jump(exit_block, &[BlockArg::from(default_next)]);
        return;
    }
    builder.ins().brif(is_true, taken, &[], not_taken, &[]);
}
