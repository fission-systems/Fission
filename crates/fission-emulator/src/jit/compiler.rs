//! Cranelift P-Code → host JIT (sole execution engine).
//!
//! - Multi-instruction translation blocks (TB) with soft direct chaining
//! - Intra-instruction relative BRANCH/CBRANCH via per-op Cranelift blocks
//! - Float ops via host callouts; values >8B via bulk byte callouts
//! - Space ids are whatever SLA assigned (no hardcoded register/ram)

use anyhow::Result;
use cranelift_codegen::ir::{
    types, AbiParam, BlockArg, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind,
    condcodes::IntCC,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
use std::collections::HashMap;

use crate::jit::callbacks::{SymBinOpKind, SymUnOpKind};
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
            ("jit_callother_result", crate::jit::callbacks::jit_callother_result as *const u8),
            ("jit_count_insn", crate::jit::callbacks::jit_count_insn as *const u8),
            ("jit_count_pcode", crate::jit::callbacks::jit_count_pcode as *const u8),
            ("jit_chain", crate::jit::callbacks::jit_chain as *const u8),
            ("jit_exit_tb", crate::jit::callbacks::jit_exit_tb as *const u8),
            ("jit_sym_cbranch_gate", crate::jit::callbacks::jit_sym_cbranch_gate as *const u8),
            ("jit_host_reg_base", crate::jit::callbacks::jit_host_reg_base as *const u8),
            ("jit_reg_bulk_flush", crate::jit::callbacks::jit_reg_bulk_flush as *const u8),
            ("jit_shadow_copy", crate::jit::callbacks::jit_shadow_copy as *const u8),
            ("jit_shadow_load", crate::jit::callbacks::jit_shadow_load as *const u8),
            ("jit_shadow_store", crate::jit::callbacks::jit_shadow_store as *const u8),
            ("jit_shadow_binop", crate::jit::callbacks::jit_shadow_binop as *const u8),
            ("jit_shadow_unop", crate::jit::callbacks::jit_shadow_unop as *const u8),
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
        register_space: u64,
    ) -> Result<*const u8> {
        self.compile_translation_block(
            &[GuestInsn {
                pc,
                len: inst_len,
                ops: ops.to_vec(),
            }],
            register_space,
        )
    }

    /// Compile a multi-instruction translation block.
    ///
    /// Host signature: `extern "C" fn(*mut Emulator) -> u64` → final next PC
    /// (after hard/soft chaining via global chain table).
    ///
    /// `register_space` is the SLA register-space id used for zero-callout
    /// host register-file loads.
    pub fn compile_translation_block(
        &mut self,
        insns: &[GuestInsn],
        register_space: u64,
    ) -> Result<*const u8> {
        anyhow::ensure!(!insns.is_empty(), "empty translation block");
        use crate::pcode::state::HOST_REG_FILE_SIZE;

        let start_pc = insns[0].pc;

        // Flatten ops with remapped relative branch targets.
        // Also record which global op index starts each guest instruction.
        let mut flat: Vec<PcodeOp> = Vec::new();
        let mut insn_starts: Vec<(usize, u64, u32)> = Vec::new(); // (op_index, pc, len)

        for insn in insns {
            let base = flat.len();
            insn_starts.push((base, insn.pc, insn.len));
            for (local_i, op) in insn.ops.iter().enumerate() {
                let mut op = op.clone();
                // SLEIGH relative BRANCH/CBRANCH offsets are from the *current*
                // p-code op, not the instruction start. Convert to absolute flat index.
                remap_relative_branches(&mut op, base, local_i, insn.ops.len());
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

        let mut sig_pcode = self.module.make_signature();
        sig_pcode.params.push(AbiParam::new(types::I64));
        sig_pcode.returns.push(AbiParam::new(types::I64));
        let count_pcode_fn = self
            .module
            .declare_function("jit_count_pcode", Linkage::Import, &sig_pcode)
            .unwrap();

        // jit_callother_result(emu) -> u64
        let mut sig_cor = self.module.make_signature();
        sig_cor.params.push(AbiParam::new(types::I64));
        sig_cor.returns.push(AbiParam::new(types::I64));
        let callother_result_fn = self
            .module
            .declare_function("jit_callother_result", Linkage::Import, &sig_cor)
            .unwrap();

        let mut sig_exit = self.module.make_signature();
        sig_exit.params.push(AbiParam::new(types::I64)); // emu
        sig_exit.params.push(AbiParam::new(types::I64)); // next_pc
        sig_exit.returns.push(AbiParam::new(types::I64));
        let exit_tb_fn = self
            .module
            .declare_function("jit_exit_tb", Linkage::Import, &sig_exit)
            .unwrap();

        // jit_sym_cbranch_gate(emu, cond_val, space, offset, taken, not_taken) -> u64
        let mut sig_sym = self.module.make_signature();
        sig_sym.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        sig_sym.returns.push(AbiParam::new(types::I64));
        let sym_gate_fn = self
            .module
            .declare_function("jit_sym_cbranch_gate", Linkage::Import, &sig_sym)
            .unwrap();

        // jit_host_reg_base(emu) -> ptr
        let mut sig_regbase = self.module.make_signature();
        sig_regbase.params.push(AbiParam::new(types::I64));
        sig_regbase.returns.push(AbiParam::new(types::I64));
        let host_reg_base_fn = self
            .module
            .declare_function("jit_host_reg_base", Linkage::Import, &sig_regbase)
            .unwrap();

        // shadow helpers share similar shapes
        let mut sig_sh_copy = self.module.make_signature();
        sig_sh_copy.params.extend([
            AbiParam::new(types::I64), // emu
            AbiParam::new(types::I64), // dst_sp
            AbiParam::new(types::I64), // dst_off
            AbiParam::new(types::I64), // dst_sz
            AbiParam::new(types::I64), // src_sp
            AbiParam::new(types::I64), // src_off
        ]);
        let shadow_copy_fn = self
            .module
            .declare_function("jit_shadow_copy", Linkage::Import, &sig_sh_copy)
            .unwrap();

        let mut sig_sh_load = self.module.make_signature();
        sig_sh_load.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        let shadow_load_fn = self
            .module
            .declare_function("jit_shadow_load", Linkage::Import, &sig_sh_load)
            .unwrap();

        let mut sig_sh_store = self.module.make_signature();
        sig_sh_store.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        let shadow_store_fn = self
            .module
            .declare_function("jit_shadow_store", Linkage::Import, &sig_sh_store)
            .unwrap();

        // jit_shadow_binop(emu, dst_sp, dst_off, dst_sz,
        //   a_sp, a_off, a_val, a_size, b_sp, b_off, b_val, b_size, op_kind)
        let mut sig_sh_bin = self.module.make_signature();
        for _ in 0..13 {
            sig_sh_bin.params.push(AbiParam::new(types::I64));
        }
        // op_kind is i32-compatible but we pass i64 for ABI simplicity
        let shadow_binop_fn = self
            .module
            .declare_function("jit_shadow_binop", Linkage::Import, &sig_sh_bin)
            .unwrap();

        // jit_shadow_unop(emu, dst_sp, dst_off, dst_sz, a_sp, a_off, a_val, a_size, op_kind)
        let mut sig_sh_un = self.module.make_signature();
        for _ in 0..9 {
            sig_sh_un.params.push(AbiParam::new(types::I64));
        }
        let shadow_unop_fn = self
            .module
            .declare_function("jit_shadow_unop", Linkage::Import, &sig_sh_un)
            .unwrap();

        // jit_reg_bulk_flush(emu, entries_ptr, count)
        let mut sig_bulk = self.module.make_signature();
        sig_bulk.params.extend([
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
            AbiParam::new(types::I64),
        ]);
        let reg_bulk_fn = self
            .module
            .declare_function("jit_reg_bulk_flush", Linkage::Import, &sig_bulk)
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
        let callother_result_ref = self
            .module
            .declare_func_in_func(callother_result_fn, builder.func);
        let count_ref = self.module.declare_func_in_func(count_fn, builder.func);
        let count_pcode_ref = self
            .module
            .declare_func_in_func(count_pcode_fn, builder.func);
        let exit_tb_ref = self.module.declare_func_in_func(exit_tb_fn, builder.func);
        let sym_gate_ref = self.module.declare_func_in_func(sym_gate_fn, builder.func);
        let host_reg_base_ref = self
            .module
            .declare_func_in_func(host_reg_base_fn, builder.func);
        let shadow_copy_ref = self
            .module
            .declare_func_in_func(shadow_copy_fn, builder.func);
        let shadow_load_ref = self
            .module
            .declare_func_in_func(shadow_load_fn, builder.func);
        let shadow_store_ref = self
            .module
            .declare_func_in_func(shadow_store_fn, builder.func);
        let shadow_binop_ref = self
            .module
            .declare_func_in_func(shadow_binop_fn, builder.func);
        let shadow_unop_ref = self
            .module
            .declare_func_in_func(shadow_unop_fn, builder.func);
        let reg_bulk_ref = self.module.declare_func_in_func(reg_bulk_fn, builder.func);

        let default_next = builder.ins().iconst(types::I64, fallthrough_pc as i64);

        // One host-reg-base load per TB — subsequent register loads use MemFlags.
        let reg_base_call = builder.ins().call(host_reg_base_ref, &[emu_ptr]);
        let host_reg_base = builder.inst_results(reg_base_call)[0];

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
                    } else if $space == register_space
                        && ($offset as usize) + ($size as usize).min(8) <= HOST_REG_FILE_SIZE
                    {
                        // Zero-callout register load from host_reg_file.
                        let ptr = builder
                            .ins()
                            .iadd_imm(host_reg_base, $offset as i64);
                        let flags = MemFlagsData::trusted();
                        let val = builder.ins().load(types::I64, flags, ptr, 0);
                        // Mask to size if < 8
                        let val = if ($size as u32) < 8 && ($size as u32) > 0 {
                            let mask = if $size >= 8 {
                                u64::MAX
                            } else {
                                (1u64 << ($size * 8)) - 1
                            };
                            builder.ins().band_imm(val, mask as i64)
                        } else {
                            val
                        };
                        builder.def_var(v, val);
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

        macro_rules! emit_shadow_binop {
            ($out:expr, $a:expr, $b:expr, $a_val:expr, $b_val:expr, $kind:expr) => {{
                let out: &Varnode = $out;
                let a: &Varnode = $a;
                let b: &Varnode = $b;
                let a_val = $a_val;
                let b_val = $b_val;
                let kind: u32 = $kind;
                if !out.is_constant {
                    let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                    let doff = builder.ins().iconst(types::I64, out.offset as i64);
                    let dsz = builder.ins().iconst(types::I64, out.size as i64);
                    let asp = builder.ins().iconst(
                        types::I64,
                        if a.is_constant { 0 } else { a.space_id as i64 },
                    );
                    let aoff = builder.ins().iconst(
                        types::I64,
                        if a.is_constant {
                            0
                        } else {
                            a.offset as i64
                        },
                    );
                    let asz = builder.ins().iconst(
                        types::I64,
                        if a.is_constant {
                            8
                        } else {
                            a.size as i64
                        },
                    );
                    let bsp = builder.ins().iconst(
                        types::I64,
                        if b.is_constant { 0 } else { b.space_id as i64 },
                    );
                    let boff = builder.ins().iconst(
                        types::I64,
                        if b.is_constant {
                            0
                        } else {
                            b.offset as i64
                        },
                    );
                    let bsz = builder.ins().iconst(
                        types::I64,
                        if b.is_constant {
                            8
                        } else {
                            b.size as i64
                        },
                    );
                    let kind_v = builder.ins().iconst(types::I64, kind as i64);
                    builder.ins().call(
                        shadow_binop_ref,
                        &[
                            emu_ptr, dsp, doff, dsz, asp, aoff, a_val, asz, bsp, boff, b_val,
                            bsz, kind_v,
                        ],
                    );
                }
            }};
        }

        macro_rules! emit_shadow_unop {
            ($out:expr, $a:expr, $a_val:expr, $kind:expr) => {{
                let out: &Varnode = $out;
                let a: &Varnode = $a;
                let a_val = $a_val;
                let kind: u32 = $kind;
                if !out.is_constant {
                    let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                    let doff = builder.ins().iconst(types::I64, out.offset as i64);
                    let dsz = builder.ins().iconst(types::I64, out.size as i64);
                    let asp = builder.ins().iconst(
                        types::I64,
                        if a.is_constant { 0 } else { a.space_id as i64 },
                    );
                    let aoff = builder.ins().iconst(
                        types::I64,
                        if a.is_constant {
                            0
                        } else {
                            a.offset as i64
                        },
                    );
                    let asz = builder.ins().iconst(
                        types::I64,
                        if a.is_constant {
                            8
                        } else {
                            a.size as i64
                        },
                    );
                    let kind_v = builder.ins().iconst(types::I64, kind as i64);
                    builder.ins().call(
                        shadow_unop_ref,
                        &[emu_ptr, dsp, doff, dsz, asp, aoff, a_val, asz, kind_v],
                    );
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

        // `load_vn!` always zero-extends a narrower-than-8-byte operand into
        // its I64 Cranelift value (see `ensure_var!`'s masking and the
        // `jit_read_space` host callout, both size-aware but sign-agnostic).
        // That's correct for unsigned ops (IntLess/IntAdd/IntAnd/...) and for
        // ops where signedness of the *input* representation doesn't affect
        // the result (Add/Sub/Mult: two's-complement wraparound arithmetic
        // is identical either way once the output is masked to its own
        // width). It is **wrong** for ops whose result genuinely depends on
        // the input's sign -- signed compare/divide/remainder/arithmetic-
        // shift -- where a negative narrow value (e.g. a `dword` `-1` =
        // `0xFFFFFFFF`) must be sign-extended to a huge *negative* I64
        // (`0xFFFFFFFFFFFFFFFF`), not left as a huge *positive* one. Same
        // ishl/sshr-immediate technique `IntSExt` below already uses.
        macro_rules! sign_extend_val {
            ($val:expr, $size:expr) => {{
                let val = $val;
                let shift = 64i64 - (($size as i64) * 8);
                if shift > 0 && shift < 64 {
                    let s = builder.ins().ishl_imm(val, shift);
                    builder.ins().sshr_imm(s, shift)
                } else {
                    val
                }
            }};
        }

        macro_rules! load_vn_signed {
            ($vn:expr) => {{
                let vn: &Varnode = $vn;
                let val = load_vn!(vn);
                sign_extend_val!(val, vn.size.min(8))
            }};
        }

        macro_rules! store_vn {
            ($vn:expr, $val:expr) => {{
                let vn: &Varnode = $vn;
                let val = $val;
                if !vn.is_constant {
                    let v = ensure_var!(vn.space_id, vn.offset, vn.size.min(8));
                    builder.def_var(v, val);
                    // Zero-callout register store into host_reg_file (size-correct).
                    let rsz = vn.size.min(8) as u32;
                    if vn.space_id == register_space
                        && (vn.offset as usize) + (rsz as usize) <= HOST_REG_FILE_SIZE
                    {
                        let ptr = builder
                            .ins()
                            .iadd_imm(host_reg_base, vn.offset as i64);
                        let flags = MemFlagsData::trusted();
                        match rsz {
                            1 => {
                                builder.ins().istore8(flags, val, ptr, 0);
                            }
                            2 => {
                                builder.ins().istore16(flags, val, ptr, 0);
                            }
                            4 => {
                                builder.ins().istore32(flags, val, ptr, 0);
                            }
                            _ => {
                                builder.ins().store(flags, val, ptr, 0);
                            }
                        }
                    }
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

            // Per-pcode-op fuse (breaks relative CBRANCH livelocks under max_inst).
            {
                let tick = builder.ins().call(count_pcode_ref, &[emu_ptr]);
                let stop = builder.inst_results(tick)[0];
                let is_stop = builder.ins().icmp_imm(IntCC::NotEqual, stop, 0);
                let stop_b = builder.create_block();
                let cont_b = builder.create_block();
                builder.ins().brif(is_stop, stop_b, &[], cont_b, &[]);
                builder.switch_to_block(stop_b);
                builder.seal_block(stop_b);
                builder
                    .ins()
                    .jump(exit_block, &[BlockArg::from(default_next)]);
                builder.switch_to_block(cont_b);
                builder.seal_block(cont_b);
            }

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
                                // Shadow COPY
                                let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                                let doff = builder.ins().iconst(types::I64, out.offset as i64);
                                let dsz = builder.ins().iconst(types::I64, out.size as i64);
                                let ssp = builder.ins().iconst(
                                    types::I64,
                                    if src.is_constant {
                                        0
                                    } else {
                                        src.space_id as i64
                                    },
                                );
                                let soff = builder.ins().iconst(
                                    types::I64,
                                    if src.is_constant {
                                        0
                                    } else {
                                        src.offset as i64
                                    },
                                );
                                builder.ins().call(
                                    shadow_copy_ref,
                                    &[emu_ptr, dsp, doff, dsz, ssp, soff],
                                );
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
                            // Taint from loaded memory bytes → dest varnode
                            let dsp = builder.ins().iconst(types::I64, out.space_id as i64);
                            let doff = builder.ins().iconst(types::I64, out.offset as i64);
                            let dsz = builder.ins().iconst(types::I64, out.size as i64);
                            let msp = builder.ins().iconst(types::I64, space_id as i64);
                            builder.ins().call(
                                shadow_load_ref,
                                &[emu_ptr, dsp, doff, dsz, msp, addr],
                            );
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
                        // Taint memory from value varnode
                        let msp = builder.ins().iconst(types::I64, space_id as i64);
                        let sz = builder.ins().iconst(types::I64, val_vn.size as i64);
                        let vsp = builder.ins().iconst(
                            types::I64,
                            if val_vn.is_constant {
                                0
                            } else {
                                val_vn.space_id as i64
                            },
                        );
                        let voff = builder.ins().iconst(
                            types::I64,
                            if val_vn.is_constant {
                                0
                            } else {
                                val_vn.offset as i64
                            },
                        );
                        builder.ins().call(
                            shadow_store_ref,
                            &[emu_ptr, msp, addr, sz, vsp, voff],
                        );
                    }
                }

                PcodeOpcode::IntAdd => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().iadd(a, b));
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::Add as u32
                        );
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
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::Sub as u32
                        );
                    }
                }
                PcodeOpcode::IntMult => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().imul(a, b));
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::Mul as u32
                        );
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
                        let a = load_vn_signed!(&op.inputs[0]);
                        let b = load_vn_signed!(&op.inputs[1]);
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
                        let a = load_vn_signed!(&op.inputs[0]);
                        let b = load_vn_signed!(&op.inputs[1]);
                        store_vn!(out, builder.ins().srem(a, b));
                    }
                }
                PcodeOpcode::IntAnd | PcodeOpcode::BoolAnd => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().band(a, b));
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::And as u32
                        );
                    }
                }
                PcodeOpcode::IntOr | PcodeOpcode::BoolOr => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().bor(a, b));
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::Or as u32
                        );
                    }
                }
                PcodeOpcode::IntXor | PcodeOpcode::BoolXor => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().bxor(a, b));
                        emit_shadow_binop!(
                            out,
                            &op.inputs[0],
                            &op.inputs[1],
                            a,
                            b,
                            SymBinOpKind::Xor as u32
                        );
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
                        // Only the value being shifted needs sign-extension;
                        // the shift *count* (`b`) is a plain magnitude, not
                        // a signed quantity -- sign-extending it would be
                        // wrong if its own high bit happened to be set.
                        let a = load_vn_signed!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        store_vn!(out, builder.ins().sshr(a, b));
                    }
                }
                PcodeOpcode::IntNegate => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().bnot(a));
                        emit_shadow_unop!(out, &op.inputs[0], a, SymUnOpKind::Not as u32);
                    }
                }
                PcodeOpcode::Int2Comp => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().ineg(a));
                        emit_shadow_unop!(out, &op.inputs[0], a, SymUnOpKind::Neg as u32);
                    }
                }
                PcodeOpcode::BoolNegate => {
                    if let Some(out) = op.output.as_ref() {
                        let a = load_vn!(&op.inputs[0]);
                        store_vn!(out, builder.ins().bxor_imm(a, 1));
                        emit_shadow_unop!(out, &op.inputs[0], a, SymUnOpKind::BoolNot as u32);
                    }
                }
                PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual => {
                    if let Some(out) = op.output.as_ref() {
                        // `a`/`b` (zero-extended) still feed `emit_shadow_binop!`
                        // below unchanged -- the symbolic/taint side-channel
                        // re-derives sign context itself from each operand's
                        // own declared size, so it isn't affected by (and
                        // shouldn't be given) the sign-extended values used
                        // only for the *actual* signed comparison here.
                        let a = load_vn!(&op.inputs[0]);
                        let b = load_vn!(&op.inputs[1]);
                        let signed = matches!(
                            op.opcode,
                            PcodeOpcode::IntSLess | PcodeOpcode::IntSLessEqual
                        );
                        let (cmp_a, cmp_b) = if signed {
                            (
                                sign_extend_val!(a, op.inputs[0].size.min(8)),
                                sign_extend_val!(b, op.inputs[1].size.min(8)),
                            )
                        } else {
                            (a, b)
                        };
                        let cc = match op.opcode {
                            PcodeOpcode::IntEqual => IntCC::Equal,
                            PcodeOpcode::IntNotEqual => IntCC::NotEqual,
                            PcodeOpcode::IntSLess => IntCC::SignedLessThan,
                            PcodeOpcode::IntSLessEqual => IntCC::SignedLessThanOrEqual,
                            PcodeOpcode::IntLess => IntCC::UnsignedLessThan,
                            PcodeOpcode::IntLessEqual => IntCC::UnsignedLessThanOrEqual,
                            _ => unreachable!(),
                        };
                        let b_res = builder.ins().icmp(cc, cmp_a, cmp_b);
                        store_vn!(out, builder.ins().uextend(types::I64, b_res));
                        let sk = match op.opcode {
                            PcodeOpcode::IntEqual => SymBinOpKind::Eq as u32,
                            PcodeOpcode::IntNotEqual => SymBinOpKind::Neq as u32,
                            PcodeOpcode::IntSLess => SymBinOpKind::Slt as u32,
                            PcodeOpcode::IntSLessEqual => SymBinOpKind::Sle as u32,
                            PcodeOpcode::IntLess => SymBinOpKind::Ult as u32,
                            PcodeOpcode::IntLessEqual => SymBinOpKind::Ule as u32,
                            _ => SymBinOpKind::Eq as u32,
                        };
                        emit_shadow_binop!(out, &op.inputs[0], &op.inputs[1], a, b, sk);
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
                        let sk = match op.opcode {
                            PcodeOpcode::FloatAdd => SymBinOpKind::FloatAdd as u32,
                            PcodeOpcode::FloatSub => SymBinOpKind::FloatSub as u32,
                            PcodeOpcode::FloatMult => SymBinOpKind::FloatMul as u32,
                            PcodeOpcode::FloatDiv => SymBinOpKind::FloatDiv as u32,
                            PcodeOpcode::FloatEqual => SymBinOpKind::FloatEq as u32,
                            PcodeOpcode::FloatNotEqual => SymBinOpKind::FloatNeq as u32,
                            PcodeOpcode::FloatLess => SymBinOpKind::FloatLt as u32,
                            PcodeOpcode::FloatLessEqual => SymBinOpKind::FloatLe as u32,
                            _ => SymBinOpKind::FloatAdd as u32,
                        };
                        emit_shadow_binop!(out, &op.inputs[0], &op.inputs[1], a, b, sk);
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
                        let sk = match op.opcode {
                            PcodeOpcode::FloatNeg => SymUnOpKind::FloatNeg as u32,
                            PcodeOpcode::FloatAbs => SymUnOpKind::FloatAbs as u32,
                            PcodeOpcode::FloatSqrt => SymUnOpKind::FloatSqrt as u32,
                            PcodeOpcode::FloatNan => SymUnOpKind::FloatNan as u32,
                            PcodeOpcode::FloatCeil => SymUnOpKind::FloatCeil as u32,
                            PcodeOpcode::FloatFloor => SymUnOpKind::FloatFloor as u32,
                            PcodeOpcode::FloatRound => SymUnOpKind::FloatRound as u32,
                            PcodeOpcode::FloatTrunc => SymUnOpKind::FloatTrunc as u32,
                            PcodeOpcode::FloatInt2Float => SymUnOpKind::FloatInt2Float as u32,
                            PcodeOpcode::FloatFloat2Float => SymUnOpKind::FloatFloat2Float as u32,
                            _ => SymUnOpKind::FloatNeg as u32,
                        };
                        emit_shadow_unop!(out, &op.inputs[0], a, sk);
                    }
                }

                PcodeOpcode::Branch => {
                    let dest = &op.inputs[0];
                    if dest.space_id == 0 || dest.is_constant {
                        let abs = dest.constant_val;
                        if abs >= 0 && (abs as usize) < n_ops {
                            builder.ins().jump(op_blocks[abs as usize], &[]);
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
                    let cond_vn = &op.inputs[1];
                    let cond = load_vn!(cond_vn);

                    // Symbolic gate: if condition is tainted, stop TB for manager fork.
                    {
                        // Absolute guest target when dest is a RAM/code address;
                        // relative p-code indices have no guest VA — fall back to
                        // fallthrough so the concrete path still has a resume PC.
                        let taken_addr = if dest.space_id == 0 || dest.is_constant {
                            fallthrough_pc
                        } else {
                            dest.offset
                        };
                        let not_taken_addr = fallthrough_pc;
                        let csp = builder.ins().iconst(
                            types::I64,
                            if cond_vn.is_constant {
                                0i64
                            } else {
                                cond_vn.space_id as i64
                            },
                        );
                        let coff = builder.ins().iconst(
                            types::I64,
                            if cond_vn.is_constant {
                                0i64
                            } else {
                                cond_vn.offset as i64
                            },
                        );
                        let t_a = builder.ins().iconst(types::I64, taken_addr as i64);
                        let n_a = builder.ins().iconst(types::I64, not_taken_addr as i64);
                        let gcall = builder.ins().call(
                            sym_gate_ref,
                            &[emu_ptr, cond, csp, coff, t_a, n_a],
                        );
                        let stop = builder.inst_results(gcall)[0];
                        let is_stop = builder.ins().icmp_imm(IntCC::NotEqual, stop, 0);
                        let stop_b = builder.create_block();
                        let cont_sym = builder.create_block();
                        builder.ins().brif(is_stop, stop_b, &[], cont_sym, &[]);
                        builder.switch_to_block(stop_b);
                        builder.seal_block(stop_b);
                        // Exit at the concrete branch target so SimulationManager
                        // can resume both forks at real guest PCs.
                        let is_true =
                            builder.ins().icmp_imm(IntCC::NotEqual, cond, 0);
                        let concrete_next =
                            builder.ins().select(is_true, t_a, n_a);
                        builder
                            .ins()
                            .jump(exit_block, &[BlockArg::from(concrete_next)]);
                        builder.switch_to_block(cont_sym);
                        builder.seal_block(cont_sym);
                    }

                    let is_true = builder.ins().icmp_imm(IntCC::NotEqual, cond, 0);

                    if dest.space_id == 0 || dest.is_constant {
                        let abs = dest.constant_val;
                        let taken = if abs >= 0 && (abs as usize) < n_ops {
                            op_blocks[abs as usize]
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
                    // Flush dirty so HLE sees live SSA; after the call, reload
                    // those slots so exit writeback cannot clobber HLE updates
                    // (e.g. syscall RAX). This is the hot-path reg cache coherence
                    // point for mid-TB HLE.
                    let mut flushed: HashMap<(u64, u64), (u32, cranelift_frontend::Variable)> =
                        HashMap::new();
                    for (sp, off, sz, v) in &dirty {
                        flushed.insert((*sp, *off), (*sz, *v));
                    }
                    // Flush dirty before HLE:
                    // - registers → one bulk callout (host already has IR stores;
                    //   bulk re-syncs AddressSpace from SSA values)
                    // - unique/ram → per-slot write_space
                    {
                        let mut reg_entries: Vec<(u64, u32, cranelift_frontend::Variable)> =
                            Vec::new();
                        for ((sp, off), (sz, v)) in &flushed {
                            if *sp == 0 {
                                continue;
                            }
                            if *sp == register_space
                                && (*off as usize) + (*sz as usize) <= HOST_REG_FILE_SIZE
                            {
                                reg_entries.push((*off, *sz, *v));
                                continue;
                            }
                            let val = builder.use_var(*v);
                            let spv = builder.ins().iconst(types::I64, *sp as i64);
                            let offv = builder.ins().iconst(types::I64, *off as i64);
                            let szv = builder.ins().iconst(types::I64, *sz as i64);
                            builder
                                .ins()
                                .call(write_space_ref, &[emu_ptr, spv, offv, szv, val]);
                        }
                        if !reg_entries.is_empty() {
                            let n = reg_entries.len();
                            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                (n * 24) as u32,
                                0,
                            ));
                            for (i, (off, sz, v)) in reg_entries.iter().enumerate() {
                                let base = (i * 24) as i32;
                                let offv = builder.ins().iconst(types::I64, *off as i64);
                                let szv = builder.ins().iconst(types::I64, *sz as i64);
                                let val = builder.use_var(*v);
                                builder.ins().stack_store(offv, slot, base);
                                builder.ins().stack_store(szv, slot, base + 8);
                                builder.ins().stack_store(val, slot, base + 16);
                            }
                            let ptr = builder.ins().stack_addr(types::I64, slot, 0);
                            let cnt = builder.ins().iconst(types::I64, n as i64);
                            builder
                                .ins()
                                .call(reg_bulk_ref, &[emu_ptr, ptr, cnt]);
                        }
                    }

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

                    // Reload flushed vars (HLE may have mutated them).
                    // Registers: zero-callout load from host_reg_file.
                    for ((sp, off), (sz, v)) in flushed {
                        if sp == 0 {
                            continue;
                        }
                        if sp == register_space
                            && (off as usize) + (sz as usize) <= HOST_REG_FILE_SIZE
                        {
                            let ptr = builder.ins().iadd_imm(host_reg_base, off as i64);
                            let flags = MemFlagsData::trusted();
                            let val = builder.ins().load(types::I64, flags, ptr, 0);
                            builder.def_var(v, val);
                            continue;
                        }
                        let spv = builder.ins().iconst(types::I64, sp as i64);
                        let offv = builder.ins().iconst(types::I64, off as i64);
                        let szv = builder.ins().iconst(types::I64, sz as i64);
                        let rcall =
                            builder
                                .ins()
                                .call(read_space_ref, &[emu_ptr, spv, offv, szv]);
                        let val = builder.inst_results(rcall)[0];
                        builder.def_var(v, val);
                    }
                    // Drop pre-HLE dirty so TB exit does not re-write stale SSA.
                    dirty.clear();

                    // CallOther data result after reload so it is not clobbered
                    // (e.g. segment_fs → linear address into unique/output).
                    if let Some(out) = op.output.as_ref() {
                        let rcall = builder
                            .ins()
                            .call(callother_result_ref, &[emu_ptr]);
                        let val = builder.inst_results(rcall)[0];
                        store_vn!(out, val);
                    }

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
            // Do not seal op_blocks here: relative CBRANCH may jump backward to
            // an earlier p-code op (common in CRT). Seal after all edges exist.
        }

        for &b in &op_blocks {
            builder.seal_block(b);
        }

        // ── Exit: writeback + soft chain ─────────────────────────────────────
        builder.switch_to_block(exit_block);
        builder.seal_block(exit_block);
        let next_pc = builder.block_params(exit_block)[0];

        // TB exit writeback: registers → one bulk flush; unique/ram → per-slot.
        {
            let mut last: HashMap<(u64, u64), (u32, Variable)> = HashMap::new();
            for (sp, off, sz, v) in dirty {
                last.insert((sp, off), (sz, v));
            }
            let mut reg_entries: Vec<(u64, u32, Variable)> = Vec::new();
            for ((sp, off), (sz, v)) in last {
                if sp == 0 {
                    continue;
                }
                if sp == register_space && (off as usize) + (sz as usize) <= HOST_REG_FILE_SIZE {
                    reg_entries.push((off, sz, v));
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
            if !reg_entries.is_empty() {
                let n = reg_entries.len();
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (n * 24) as u32,
                    0,
                ));
                for (i, (off, sz, v)) in reg_entries.iter().enumerate() {
                    let base = (i * 24) as i32;
                    let offv = builder.ins().iconst(types::I64, *off as i64);
                    let szv = builder.ins().iconst(types::I64, *sz as i64);
                    let val = builder.use_var(*v);
                    builder.ins().stack_store(offv, slot, base);
                    builder.ins().stack_store(szv, slot, base + 8);
                    builder.ins().stack_store(val, slot, base + 16);
                }
                let ptr = builder.ins().stack_addr(types::I64, slot, 0);
                let cnt = builder.ins().iconst(types::I64, n as i64);
                builder.ins().call(reg_bulk_ref, &[emu_ptr, ptr, cnt]);
            }
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

/// Convert SLEIGH relative BRANCH/CBRANCH destinations to absolute flat indices.
///
/// In lifted p-code, a constant branch target is an offset **relative to the
/// current op** within the instruction (e.g. TZCNT loop: `CBranch +5` from the
/// LSB test, `Branch -6` back to the loop head). Adding only `base` (instruction
/// start in the flat stream) treats the constant as absolute-within-insn and
/// breaks backward loops: `-6 as usize` becomes a huge out-of-range index and
/// the JIT falls through, so `tzcnt` never iterates and size-class recovery
/// livelocks (static CRT stop_pc `0x10035A3`).
/// `insn_op_count` is the number of p-code ops in the *single guest
/// instruction* `op` belongs to (before flattening into the TB-wide `flat`
/// vec) -- used to validate a candidate relative delta, not to bound the
/// search.
fn remap_relative_branches(op: &mut PcodeOp, base: usize, local_index: usize, insn_op_count: usize) {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            if let Some(dest) = op.inputs.first_mut() {
                // Two encodings of a branch destination reach here:
                // 1. An already-resolved absolute guest address (the common
                //    case -- normal cross-instruction jumps/calls).
                // 2. A relative delta, from SLEIGH's own intra-instruction
                //    control flow (loop/skip constructs within one
                //    instruction's semantic template, e.g. TZCNT's bit-scan
                //    loop) -- always small in magnitude, since it can only
                //    ever address an op within the *same* instruction.
                //
                // Ghidra's own decompiler-facing lifter tags relative deltas
                // with space_id==0 (constant space) and is_constant==true,
                // but this emulator's own SLEIGH decode path does not draw
                // that distinction the same way -- confirmed empirically
                // (real static-binary fixture, `space_id`/`is_constant` were
                // identical for both an absolute target and a relative -1/-2
                // delta; the raw delta value showed up in `offset`, not
                // `constant_val`). Rather than guess a magic-number
                // threshold, validate against the one invariant that's
                // actually true of every relative delta: it must land
                // *inside this same instruction's own op range* when added
                // to `local_index`. A real guest address essentially never
                // satisfies that by coincidence (every loaded binary in
                // practice starts well above the highest plausible
                // instruction op count).
                let raw = if dest.offset != 0 {
                    dest.offset as u32
                } else {
                    dest.constant_val as u32
                };
                let delta = i32::from_le_bytes(raw.to_le_bytes());
                let candidate = local_index as i64 + delta as i64;
                let is_relative = delta != 0
                    && candidate >= 0
                    && (candidate as usize) < insn_op_count
                    && candidate as usize != local_index;
                if is_relative {
                    // Downstream (this file's Branch/CBranch codegen) reads
                    // `constant_val` as a flat TB op index precisely when
                    // `space_id == 0 || is_constant` -- mark this resolved
                    // the same way, or it falls into the "absolute guest
                    // address" branch and treats the flat index as a real
                    // address instead.
                    let abs = base as i128 + candidate as i128;
                    dest.constant_val = abs as i64;
                    dest.is_constant = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 4, // matches RUST_SLEIGH_REGISTER_SPACE_ID's numbering
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn imm(val: i64, size: u32) -> Varnode {
        Varnode {
            space_id: 0,
            offset: 0,
            size,
            is_constant: true,
            constant_val: val,
        }
    }

    /// Real `Emulator` construction, same pattern as `selfjit::compiler`'s
    /// own tests -- a real loaded ELF is the simplest way to get a
    /// fully-formed `MachineState`/register space, even though these
    /// tests' compiled code never touches the binary's own instructions.
    fn make_emu() -> crate::core::Emulator {
        use crate::core::Emulator;
        use crate::os::LinuxEnv;
        use crate::pcode::state::MachineState;

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_static_printf_malloc.elf");
        let binary = fission_loader::loader::LoadedBinary::from_file(&path)
            .expect("load real test ELF");
        let mut state = MachineState::new();
        let _info = crate::os::linux::loader::load_elf(&mut state, &binary).expect("load_elf");
        let load_spec = binary.load_spec().expect("load spec").clone();
        let sleigh =
            fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
                &load_spec,
            )
            .expect("sleigh frontend candidates")
            .into_iter()
            .next()
            .expect("at least one sleigh frontend");
        let arch = crate::arch::ArchInfo::from_language_id(
            load_spec.pair.language_id.as_str(),
            Some(&binary),
        )
        .expect("arch info");
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator")
    }

    /// Compiles `ops` as a single-instruction TB via the real Cranelift
    /// `JitCompiler` (the backend `run_instruction` actually dispatches to
    /// -- unlike `selfjit`, which is scaffolding, not the live execution
    /// path), executes it against a real `Emulator`, and returns that
    /// emulator so the caller can read out whichever register-space
    /// offsets it cares about.
    fn compile_and_run(ops: Vec<PcodeOp>) -> crate::core::Emulator {
        let insns = [GuestInsn {
            pc: 0x1000,
            len: 4,
            ops,
        }];
        let mut compiler = JitCompiler::new().expect("cranelift backend available");
        let func_ptr = compiler
            .compile_translation_block(&insns, 4)
            .expect("compile");
        let mut emu = make_emu();
        let f: extern "C" fn(*mut crate::core::Emulator) -> u64 =
            unsafe { std::mem::transmute(func_ptr) };
        let next_pc = f(&mut emu as *mut _);
        assert_eq!(next_pc, 0x1004, "unconditional fallthrough PC");
        emu
    }

    fn read_reg(emu: &mut crate::core::Emulator, offset: u64) -> u64 {
        let bytes = emu.state.read_space(4, offset, 8).expect("read register");
        u64::from_le_bytes(bytes.try_into().unwrap())
    }

    fn copy_const(out_offset: u64, val: i64, size: u32) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1000,
            output: Some(reg(out_offset, size)),
            inputs: vec![imm(val, size)],
            asm_mnemonic: None,
        }
    }

    fn binop_sized(opcode: PcodeOpcode, out: u64, a: u64, a_size: u32, b: u64, b_size: u32) -> PcodeOp {
        PcodeOp {
            seq_num: 1,
            opcode,
            address: 0x1000,
            output: Some(reg(out, 8)),
            inputs: vec![reg(a, a_size), reg(b, b_size)],
            asm_mnemonic: None,
        }
    }

    /// Regression test for a real bug found via `fission-verify`'s
    /// emulator-grounded ground-truth tier: a *narrower-than-register*
    /// (`dword`) negative operand in a signed comparison used to evaluate
    /// wrong, because `load_vn!` always zero-extends into the I64
    /// Cranelift value it works with, and `IntSLess`/`IntSLessEqual`
    /// compared that zero-extended (i.e. now huge-positive) value directly
    /// -- `0 >= -1i32` (stored as a `dword` `0xFFFFFFFF`) evaluated false.
    /// Real repro: `clamp(0, -1, 0)` in `control_flow_gcc_O0.exe` took the
    /// wrong branch at its first `cmp`/`jge`. Also covers `IntSDiv`/
    /// `IntSRem`/`IntSRight`, which had the identical defect.
    #[test]
    fn signed_ops_on_narrow_negative_memory_operand_are_correct() {
        // r0 = -1 (dword, i.e. 0xFFFFFFFF); r1 = 0 (dword).
        let ops = vec![
            copy_const(0, -1, 4),
            copy_const(8, 0, 4),
            // 0 >= -1 (signed) -- i.e. !(0 < -1) -- must be true.
            binop_sized(PcodeOpcode::IntSLess, 16, 8, 4, 0, 4), // 0 <s -1 -> 0 (false)
            binop_sized(PcodeOpcode::IntSLessEqual, 24, 0, 4, 8, 4), // -1 <=s 0 -> 1 (true)
            binop_sized(PcodeOpcode::IntSDiv, 32, 8, 4, 0, 4), // 0 / -1 -> 0
            binop_sized(PcodeOpcode::IntSRem, 40, 8, 4, 0, 4), // 0 % -1 -> 0
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), 0, "0 <s -1 should be false");
        assert_eq!(read_reg(&mut emu, 24), 1, "-1 <=s 0 should be true");
        assert_eq!(read_reg(&mut emu, 32), 0, "0 / -1 == 0");
        assert_eq!(read_reg(&mut emu, 40), 0, "0 % -1 == 0");

        // A case where the *sign* of the divisor actually changes the
        // result: -6 / -1 == 6 (would be a huge unsigned value if -1
        // wasn't correctly sign-extended before the divide). Also checks
        // `IntSRight` (arithmetic shift) preserves the sign of a narrow
        // negative value being shifted: -8 (dword) >> 1 == -4, not some
        // huge positive value from shifting zeros into a wrongly
        // zero-extended operand.
        let ops2 = vec![
            copy_const(0, -6, 4),
            copy_const(8, -1, 4),
            copy_const(16, -8, 4),
            copy_const(24, 1, 4),
            binop_sized(PcodeOpcode::IntSDiv, 32, 0, 4, 8, 4),
            binop_sized(PcodeOpcode::IntSRight, 40, 16, 4, 24, 4),
        ];
        let mut emu2 = compile_and_run(ops2);
        assert_eq!(read_reg(&mut emu2, 32), 6, "-6 / -1 == 6");
        assert_eq!(read_reg(&mut emu2, 40) as i64, -4, "-8 >>s 1 == -4");
    }
}
