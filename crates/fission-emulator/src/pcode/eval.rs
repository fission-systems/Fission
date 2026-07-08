use anyhow::Result;
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
use crate::pcode::state::MachineState;

pub enum StepResult {
    Next,
    Branch(u64),
    BranchRel(usize),
    /// A `CallOther` (USEROP) op was encountered.
    /// The emulator's main loop resolves the userop name and calls `dispatch_userop`.
    CallOther {
        userop_id: u32,
        input_vals: Vec<u64>,
        output_size: u32,
    },
    /// A conditional branch was evaluated.
    CBranch {
        condition_val: bool,
        /// SymNodeId of the condition AST node, if the condition was tainted.
        condition_node: Option<fission_solver::ast::SymNodeId>,
        /// The target if condition is true. If it's a relative offset within pcode, `rel_idx` is Some.
        true_rel_idx: Option<usize>,
        true_addr: Option<u64>,
    },
}

pub struct Evaluator<'a> {
    pub state: &'a mut MachineState,
    pub solver: &'a mut fission_solver::Solver,
}

impl<'a> Evaluator<'a> {
    pub fn new(state: &'a mut MachineState, solver: &'a mut fission_solver::Solver) -> Self {
        Self { state, solver }
    }

    // ── Varnode I/O ───────────────────────────────────────────────────────────

    fn read_varnode_u64(&mut self, vn: &Varnode) -> Result<u64> {
        if vn.is_constant {
            Ok(vn.constant_val as u64)
        } else {
            let data = self.state.read_space(vn.space_id, vn.offset, vn.size as usize)?;
            Ok(le_bytes_to_u64(&data))
        }
    }

    fn write_varnode_u64(&mut self, vn: &Varnode, val: u64) -> Result<()> {
        let bytes = val.to_le_bytes();
        // Clamp to 8 bytes — SIMD varnodes (XMM = 16B, YMM = 32B) are wider than u64
        let size = (vn.size as usize).min(8);
        self.state.write_space(vn.space_id, vn.offset, &bytes[..size])
    }

    fn read_varnode_shadow(&mut self, vn: &Varnode) -> Option<u32> {
        if vn.is_constant {
            return None;
        }
        // Just check the first byte for taint to keep it simple for now
        self.state.get_shadow_memory(vn.space_id, vn.offset)
    }

    fn write_varnode_shadow(&mut self, vn: &Varnode, node: u32) {
        if vn.is_constant {
            return;
        }
        for i in 0..vn.size as u64 {
            self.state.set_shadow_memory(vn.space_id, vn.offset + i, node);
        }
    }

    /// Read a float value from a varnode. Returns (bits, size_bytes).
    fn read_varnode_f64(&mut self, vn: &Varnode) -> Result<f64> {
        if vn.is_constant {
            // Interpret constant bits as IEEE-754
            return Ok(f64::from_bits(vn.constant_val as u64));
        }
        let data = self.state.read_space(vn.space_id, vn.offset, vn.size as usize)?;
        Ok(match vn.size {
            4 => f32::from_bits(u32::from_le_bytes(data[..4].try_into().unwrap())) as f64,
            8 => f64::from_bits(u64::from_le_bytes(data[..8].try_into().unwrap())),
            10 => {
                // x87 80-bit extended precision — approximate via f64
                let frac = u64::from_le_bytes(data[..8].try_into().unwrap());
                let exp_sign = u16::from_le_bytes(data[8..10].try_into().unwrap());
                let sign = (exp_sign >> 15) as u64;
                let exp = (exp_sign & 0x7FFF) as i32 - 16383;
                let mantissa = frac as f64 / (1u64 << 63) as f64;
                let val = mantissa * (2.0f64).powi(exp);
                if sign != 0 { -val } else { val }
            }
            _ => {
                tracing::warn!("read_varnode_f64: unsupported float size {}", vn.size);
                0.0
            }
        })
    }

    fn write_varnode_f64(&mut self, vn: &Varnode, val: f64) -> Result<()> {
        match vn.size {
            4 => {
                let bits = (val as f32).to_bits().to_le_bytes();
                self.state.write_space(vn.space_id, vn.offset, &bits)
            }
            8 => {
                let bits = val.to_bits().to_le_bytes();
                self.state.write_space(vn.space_id, vn.offset, &bits)
            }
            10 => {
                // x87 80-bit — store approximate representation
                let bits = val.to_bits();
                let mut buf = [0u8; 10];
                buf[..8].copy_from_slice(&bits.to_le_bytes());
                self.state.write_space(vn.space_id, vn.offset, &buf)
            }
            _ => {
                tracing::warn!("write_varnode_f64: unsupported float size {}", vn.size);
                Ok(())
            }
        }
    }

    // ── Main dispatch ─────────────────────────────────────────────────────────

    /// Evaluates a single P-Code operation against the current machine state.
    pub fn step(&mut self, op: &PcodeOp) -> Result<StepResult> {
        match op.opcode {

            // ── Memory ───────────────────────────────────────────────────────

            PcodeOpcode::Copy => {
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("COPY must have output");
                let node = self.read_varnode_shadow(&op.inputs[0]);
                if let Some(id) = node {
                    self.write_varnode_shadow(out, id);
                }
                self.write_varnode_u64(out, val)?;
            }

            PcodeOpcode::Load => {
                let space_id = op.inputs[0].constant_val as u64;
                let addr_node = self.read_varnode_shadow(&op.inputs[1]);
                let out = op.output.as_ref().expect("LOAD must have output");
                
                if let Some(ptr_node) = addr_node {
                    // Symbolic pointer! Use the angr-style mixin.
                    let result_node = crate::pcode::memory_mixin::MemoryMixin::handle_symbolic_read(
                        self.state,
                        self.solver,
                        space_id,
                        ptr_node,
                        out.size,
                    )?;
                    
                    // We must still give the output varnode some concrete value (0 is fine)
                    let zeros = vec![0u8; out.size as usize];
                    self.state.write_space(out.space_id, out.offset, &zeros)?;
                    self.write_varnode_shadow(out, result_node);
                } else {
                    // Concrete pointer
                    let addr = self.read_varnode_u64(&op.inputs[1])?;
                    let raw = self.state.read_space(space_id, addr, out.size as usize)?;
                    let node = self.state.get_shadow_memory(space_id, addr);
                    if let Some(id) = node {
                        self.write_varnode_shadow(out, id);
                    }
                    self.state.write_space(out.space_id, out.offset, &raw)?;
                }
            }

            PcodeOpcode::Store => {
                let space_id = op.inputs[0].constant_val as u64;
                let ptr_node = self.read_varnode_shadow(&op.inputs[1]);
                let val_node = self.read_varnode_shadow(&op.inputs[2]);
                
                if let (Some(p_node), Some(v_node)) = (ptr_node, val_node) {
                    // Symbolic pointer AND symbolic value
                    crate::pcode::memory_mixin::MemoryMixin::handle_symbolic_write(
                        self.state,
                        self.solver,
                        space_id,
                        p_node,
                        v_node,
                        op.inputs[2].size,
                    )?;
                } else if let Some(p_node) = ptr_node {
                    // Symbolic pointer, concrete value
                    let val = self.read_varnode_u64(&op.inputs[2])?;
                    let v_node = self.solver.register_node(fission_solver::SymExpr::new_const(val, op.inputs[2].size));
                    crate::pcode::memory_mixin::MemoryMixin::handle_symbolic_write(
                        self.state,
                        self.solver,
                        space_id,
                        p_node,
                        v_node,
                        op.inputs[2].size,
                    )?;
                } else {
                    // Concrete pointer
                    let addr = self.read_varnode_u64(&op.inputs[1])?;
                    let val = self.read_varnode_u64(&op.inputs[2])?;
                    let bytes = val.to_le_bytes();
                    let store_size = (op.inputs[2].size as usize).min(8);
                    self.state.write_space(space_id, addr, &bytes[..store_size])?;
                    if let Some(id) = val_node {
                        for i in 0..op.inputs[2].size as u64 {
                            self.state.set_shadow_memory(space_id, addr + i, id);
                        }
                    }
                }
            }

            // ── Integer arithmetic ────────────────────────────────────────────

            PcodeOpcode::IntAdd => {
                let (a, b, out) = self.int_binary(&op)?;
                let a_node = self.read_varnode_shadow(&op.inputs[0]);
                let b_node = self.read_varnode_shadow(&op.inputs[1]);
                if a_node.is_some() || b_node.is_some() {
                    use fission_solver::SymExpr;
                    let a_expr = a_node.and_then(|id| self.solver.nodes.get(&id).cloned()).unwrap_or_else(|| SymExpr::new_const(a, op.inputs[0].size));
                    let b_expr = b_node.and_then(|id| self.solver.nodes.get(&id).cloned()).unwrap_or_else(|| SymExpr::new_const(b, op.inputs[1].size));
                    let new_expr = SymExpr::new_add(a_expr, b_expr);
                    let new_id = self.solver.register_node(new_expr);
                    self.write_varnode_shadow(out, new_id);
                }
                self.write_varnode_u64(out, a.wrapping_add(b))?;
            }
            PcodeOpcode::IntSub => {
                let (a, b, out) = self.int_binary(&op)?;
                self.write_varnode_u64(out, a.wrapping_sub(b))?;
            }
            PcodeOpcode::IntMult => {
                let (a, b, out) = self.int_binary(&op)?;
                self.write_varnode_u64(out, a.wrapping_mul(b))?;
            }
            PcodeOpcode::IntDiv => {
                let (a, b, out) = self.int_binary(&op)?;
                if b == 0 { tracing::warn!("INT_DIV by zero"); self.write_varnode_u64(out, 0)?; }
                else { self.write_varnode_u64(out, a.wrapping_div(b))?; }
            }
            PcodeOpcode::IntSDiv => {
                let a_raw = self.read_varnode_u64(&op.inputs[0])?;
                let b_raw = self.read_varnode_u64(&op.inputs[1])?;
                let out   = op.output.as_ref().expect("INT_SDIV must have output");
                let sz    = op.inputs[0].size;
                let a = sign_extend(a_raw, sz);
                let b = sign_extend(b_raw, sz);
                if b == 0 { tracing::warn!("INT_SDIV by zero"); self.write_varnode_u64(out, 0)?; }
                else { self.write_varnode_u64(out, a.wrapping_div(b) as u64)?; }
            }
            PcodeOpcode::IntRem => {
                let (a, b, out) = self.int_binary(&op)?;
                if b == 0 { tracing::warn!("INT_REM by zero"); self.write_varnode_u64(out, 0)?; }
                else { self.write_varnode_u64(out, a.wrapping_rem(b))?; }
            }
            PcodeOpcode::IntSRem => {
                let a_raw = self.read_varnode_u64(&op.inputs[0])?;
                let b_raw = self.read_varnode_u64(&op.inputs[1])?;
                let out   = op.output.as_ref().expect("INT_SREM must have output");
                let sz    = op.inputs[0].size;
                let a = sign_extend(a_raw, sz);
                let b = sign_extend(b_raw, sz);
                if b == 0 { tracing::warn!("INT_SREM by zero"); self.write_varnode_u64(out, 0)?; }
                else { self.write_varnode_u64(out, a.wrapping_rem(b) as u64)?; }
            }

            // ── Bitwise ───────────────────────────────────────────────────────

            PcodeOpcode::IntAnd  => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, a & b)?; }
            PcodeOpcode::IntOr   => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, a | b)?; }
            PcodeOpcode::IntXor  => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, a ^ b)?; }
            PcodeOpcode::IntLeft => {
                let (a, b, o) = self.int_binary(&op)?;
                let shift = (b & 0x7F) as u32;
                self.write_varnode_u64(o, if shift >= 64 { 0 } else { a << shift })?;
            }
            PcodeOpcode::IntRight => {
                let (a, b, o) = self.int_binary(&op)?;
                let shift = (b & 0x7F) as u32;
                self.write_varnode_u64(o, if shift >= 64 { 0 } else { a >> shift })?;
            }
            PcodeOpcode::IntSRight => {
                let a_raw = self.read_varnode_u64(&op.inputs[0])?;
                let b_raw = self.read_varnode_u64(&op.inputs[1])?;
                let out   = op.output.as_ref().expect("INT_SRIGHT must have output");
                let sz    = op.inputs[0].size;
                let a     = sign_extend(a_raw, sz);
                let shift = (b_raw & 0x7F) as u32;
                let res   = if shift >= 64 { if a < 0 { -1i64 } else { 0i64 } } else { a >> shift };
                self.write_varnode_u64(out, res as u64)?;
            }
            PcodeOpcode::IntNegate => {
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("INT_NEGATE must have output");
                self.write_varnode_u64(out, !val)?;
            }
            PcodeOpcode::Int2Comp => {
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("INT_2COMP must have output");
                self.write_varnode_u64(out, (!val).wrapping_add(1))?;
            }

            // ── Integer comparison ────────────────────────────────────────────

            PcodeOpcode::IntEqual    => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, bool_u64(a == b))?; }
            PcodeOpcode::IntNotEqual => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, bool_u64(a != b))?; }
            PcodeOpcode::IntLess     => { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, bool_u64(a < b))?; }
            PcodeOpcode::IntLessEqual=> { let (a, b, o) = self.int_binary(&op)?; self.write_varnode_u64(o, bool_u64(a <= b))?; }
            PcodeOpcode::IntSLess    => {
                let a = sign_extend(self.read_varnode_u64(&op.inputs[0])?, op.inputs[0].size);
                let b = sign_extend(self.read_varnode_u64(&op.inputs[1])?, op.inputs[1].size);
                let o = op.output.as_ref().expect("INT_SLESS must have output");
                self.write_varnode_u64(o, bool_u64(a < b))?;
            }
            PcodeOpcode::IntSLessEqual => {
                let a = sign_extend(self.read_varnode_u64(&op.inputs[0])?, op.inputs[0].size);
                let b = sign_extend(self.read_varnode_u64(&op.inputs[1])?, op.inputs[1].size);
                let o = op.output.as_ref().expect("INT_SLESSEQUAL must have output");
                self.write_varnode_u64(o, bool_u64(a <= b))?;
            }

            // ── Carry / borrow flags (u128 widening for correctness) ──────────

            PcodeOpcode::IntCarry => {
                // Unsigned addition carry: (a + b) overflows the N-bit range
                let a   = self.read_varnode_u64(&op.inputs[0])? as u128;
                let b   = self.read_varnode_u64(&op.inputs[1])? as u128;
                let out = op.output.as_ref().expect("INT_CARRY must have output");
                let sz  = op.inputs[0].size as u32;
                let max = size_max_u128(sz);
                let carry = (a + b) > max;
                self.write_varnode_u64(out, bool_u64(carry))?;
            }
            PcodeOpcode::IntSCarry => {
                // Signed addition overflow: (a + b) overflows N-bit signed range
                let a_raw = self.read_varnode_u64(&op.inputs[0])?;
                let b_raw = self.read_varnode_u64(&op.inputs[1])?;
                let out   = op.output.as_ref().expect("INT_SCARRY must have output");
                let sz    = op.inputs[0].size;
                let a = sign_extend(a_raw, sz) as i128;
                let b = sign_extend(b_raw, sz) as i128;
                let sum = a + b;
                let bits = sz * 8;
                let min = -(1i128 << (bits - 1));
                let max =  (1i128 << (bits - 1)) - 1;
                let overflow = sum < min || sum > max;
                self.write_varnode_u64(out, bool_u64(overflow))?;
            }
            PcodeOpcode::IntSBorrow => {
                // Signed subtraction borrow/overflow: (a - b) overflows N-bit signed range
                let a_raw = self.read_varnode_u64(&op.inputs[0])?;
                let b_raw = self.read_varnode_u64(&op.inputs[1])?;
                let out   = op.output.as_ref().expect("INT_SBORROW must have output");
                let sz    = op.inputs[0].size;
                let a = sign_extend(a_raw, sz) as i128;
                let b = sign_extend(b_raw, sz) as i128;
                let diff = a - b;
                let bits = sz * 8;
                let min = -(1i128 << (bits - 1));
                let max =  (1i128 << (bits - 1)) - 1;
                let borrow = diff < min || diff > max;
                self.write_varnode_u64(out, bool_u64(borrow))?;
            }

            // ── Extension / truncation ────────────────────────────────────────

            PcodeOpcode::IntZExt => {
                let val = self.read_varnode_u64(&op.inputs[0])?; // Already zero-padded
                let out = op.output.as_ref().expect("INT_ZEXT must have output");
                self.write_varnode_u64(out, val)?;
            }
            PcodeOpcode::IntSExt => {
                let val_raw = self.read_varnode_u64(&op.inputs[0])?;
                let out     = op.output.as_ref().expect("INT_SEXT must have output");
                let sval    = sign_extend(val_raw, op.inputs[0].size);
                self.write_varnode_u64(out, sval as u64)?;
            }
            PcodeOpcode::SubPiece => {
                let val   = self.read_varnode_u64(&op.inputs[0])?;
                let trunc = self.read_varnode_u64(&op.inputs[1])?; // byte offset
                let out   = op.output.as_ref().expect("SUBPIECE must have output");
                let shift = trunc.saturating_mul(8);
                let res   = if shift >= 64 { 0 } else { val >> shift };
                self.write_varnode_u64(out, res)?;
            }
            PcodeOpcode::Piece => {
                let hi      = self.read_varnode_u64(&op.inputs[0])?;
                let lo      = self.read_varnode_u64(&op.inputs[1])?;
                let lo_bits = (op.inputs[1].size * 8) as u64;
                let out     = op.output.as_ref().expect("PIECE must have output");
                let res     = (hi << lo_bits) | lo;
                self.write_varnode_u64(out, res)?;
            }
            PcodeOpcode::PopCount => {
                let val  = self.read_varnode_u64(&op.inputs[0])?;
                let out  = op.output.as_ref().expect("POPCOUNT must have output");
                let sz   = op.inputs[0].size;
                let mask = size_mask_u64(sz);
                self.write_varnode_u64(out, (val & mask).count_ones() as u64)?;
            }
            PcodeOpcode::LzCount => {
                // Count leading zeros within the input bitwidth
                let val  = self.read_varnode_u64(&op.inputs[0])?;
                let out  = op.output.as_ref().expect("LZCOUNT must have output");
                let bits = (op.inputs[0].size * 8) as u32;
                let lz   = if val == 0 { bits } else {
                    val.leading_zeros().saturating_sub(64 - bits)
                };
                self.write_varnode_u64(out, lz as u64)?;
            }

            // ── Boolean ───────────────────────────────────────────────────────

            PcodeOpcode::BoolNegate => {
                let v = self.read_varnode_u64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("BOOL_NEGATE must have output");
                self.write_varnode_u64(o, bool_u64(v == 0))?;
            }
            PcodeOpcode::BoolAnd => {
                let (a, b, o) = self.int_binary(&op)?;
                self.write_varnode_u64(o, bool_u64(a != 0 && b != 0))?;
            }
            PcodeOpcode::BoolOr => {
                let (a, b, o) = self.int_binary(&op)?;
                self.write_varnode_u64(o, bool_u64(a != 0 || b != 0))?;
            }
            PcodeOpcode::BoolXor => {
                let (a, b, o) = self.int_binary(&op)?;
                self.write_varnode_u64(o, bool_u64((a != 0) ^ (b != 0)))?;
            }

            // ── Float arithmetic ──────────────────────────────────────────────

            PcodeOpcode::FloatAdd => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_ADD must have output");
                self.write_varnode_f64(o, a + b)?;
            }
            PcodeOpcode::FloatSub => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_SUB must have output");
                self.write_varnode_f64(o, a - b)?;
            }
            PcodeOpcode::FloatMult => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_MULT must have output");
                self.write_varnode_f64(o, a * b)?;
            }
            PcodeOpcode::FloatDiv => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_DIV must have output");
                self.write_varnode_f64(o, a / b)?; // NaN/Inf handled by IEEE-754
            }
            PcodeOpcode::FloatNeg => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_NEG must have output");
                self.write_varnode_f64(o, -a)?;
            }
            PcodeOpcode::FloatAbs => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_ABS must have output");
                self.write_varnode_f64(o, a.abs())?;
            }
            PcodeOpcode::FloatSqrt => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_SQRT must have output");
                self.write_varnode_f64(o, a.sqrt())?;
            }
            PcodeOpcode::FloatCeil => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_CEIL must have output");
                self.write_varnode_f64(o, a.ceil())?;
            }
            PcodeOpcode::FloatFloor => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_FLOOR must have output");
                self.write_varnode_f64(o, a.floor())?;
            }
            PcodeOpcode::FloatRound => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_ROUND must have output");
                self.write_varnode_f64(o, a.round())?;
            }


            // ── Float comparison ──────────────────────────────────────────────

            PcodeOpcode::FloatEqual => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_EQUAL must have output");
                self.write_varnode_u64(o, bool_u64(a == b))?;
            }
            PcodeOpcode::FloatNotEqual => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_NOTEQUAL must have output");
                self.write_varnode_u64(o, bool_u64(a != b))?;
            }
            PcodeOpcode::FloatLess => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_LESS must have output");
                self.write_varnode_u64(o, bool_u64(a < b))?;
            }
            PcodeOpcode::FloatLessEqual => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let b = self.read_varnode_f64(&op.inputs[1])?;
                let o = op.output.as_ref().expect("FLOAT_LESSEQUAL must have output");
                self.write_varnode_u64(o, bool_u64(a <= b))?;
            }
            PcodeOpcode::FloatNan => {
                let a = self.read_varnode_f64(&op.inputs[0])?;
                let o = op.output.as_ref().expect("FLOAT_NAN must have output");
                self.write_varnode_u64(o, bool_u64(a.is_nan()))?;
            }

            // ── Float ↔ Integer conversions ────────────────────────────────────

            PcodeOpcode::FloatInt2Float => {
                let val_raw = self.read_varnode_u64(&op.inputs[0])?;
                let out     = op.output.as_ref().expect("INT2FLOAT must have output");
                let sz      = op.inputs[0].size;
                let ival    = sign_extend(val_raw, sz);
                self.write_varnode_f64(out, ival as f64)?;
            }
            PcodeOpcode::FloatFloat2Float => {
                // Precision conversion (e.g. f32 → f64 or vice versa)
                let val = self.read_varnode_f64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("FLOAT2FLOAT must have output");
                self.write_varnode_f64(out, val)?;
            }
            PcodeOpcode::FloatTrunc => {
                // Float → signed integer (truncate toward zero)
                let val = self.read_varnode_f64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("TRUNC must have output");
                let ival = val.trunc() as i64;
                self.write_varnode_u64(out, ival as u64)?;
            }

            // ── Control flow ──────────────────────────────────────────────────

            PcodeOpcode::Branch => {
                let dest = &op.inputs[0];
                if dest.space_id == 0 || dest.is_constant {
                    return Ok(StepResult::BranchRel(dest.offset as usize));
                }
                return Ok(StepResult::Branch(dest.offset));
            }
            PcodeOpcode::CBranch => {
                let dest = &op.inputs[0];
                let condition = self.read_varnode_u64(&op.inputs[1])?;
                let condition_val = condition != 0;
                // Capture tainted condition for symbolic execution
                let condition_node = self.read_varnode_shadow(&op.inputs[1]);
                let is_rel = dest.space_id == 0 || dest.is_constant;
                return Ok(StepResult::CBranch {
                    condition_val,
                    condition_node,
                    true_rel_idx: if is_rel { Some(dest.offset as usize) } else { None },
                    true_addr: if !is_rel { Some(dest.offset) } else { None },
                });
            }
            PcodeOpcode::Call => {
                let dest = &op.inputs[0];
                // Call always branches to an external address
                return Ok(StepResult::Branch(dest.offset));
            }
            PcodeOpcode::CallInd | PcodeOpcode::BranchInd => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                return Ok(StepResult::Branch(target));
            }
            PcodeOpcode::Return => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                return Ok(StepResult::Branch(target));
            }

            // ── Pointer arithmetic ────────────────────────────────────────────

            PcodeOpcode::PtrAdd => {
                let ptr        = self.read_varnode_u64(&op.inputs[0])?;
                let offset     = self.read_varnode_u64(&op.inputs[1])?;
                let multiplier = self.read_varnode_u64(&op.inputs[2])?;
                let out        = op.output.as_ref().expect("PTRADD must have output");
                self.write_varnode_u64(out, ptr.wrapping_add(offset.wrapping_mul(multiplier)))?;
            }
            PcodeOpcode::PtrSub => {
                let ptr    = self.read_varnode_u64(&op.inputs[0])?;
                let offset = self.read_varnode_u64(&op.inputs[1])?;
                let out    = op.output.as_ref().expect("PTRSUB must have output");
                self.write_varnode_u64(out, ptr.wrapping_add(offset))?;
            }

            // ── SSA / type / special ──────────────────────────────────────────

            PcodeOpcode::Cast => {
                // Type cast — semantics are bit-identical, just copy.
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("CAST must have output");
                self.write_varnode_u64(out, val)?;
            }
            PcodeOpcode::MultiEqual => {
                // SSA phi node — pick the first non-zero input or the first input.
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let out = op.output.as_ref().expect("MULTIEQUAL must have output");
                self.write_varnode_u64(out, val)?;
            }
            PcodeOpcode::Indirect => {
                // SSA indirect — if there is an output, copy input 0.
                if let Some(out) = &op.output {
                    let val = self.read_varnode_u64(&op.inputs[0])?;
                    let out = out.clone();
                    self.write_varnode_u64(&out, val)?;
                }
            }
            PcodeOpcode::SegmentOp => {
                // Segment base + offset — treat as plain add.
                if op.inputs.len() >= 2 {
                    let base   = self.read_varnode_u64(&op.inputs[0])?;
                    let offset = self.read_varnode_u64(&op.inputs[1])?;
                    if let Some(out) = &op.output {
                        let out = out.clone();
                        self.write_varnode_u64(&out, base.wrapping_add(offset))?;
                    }
                }
            }
            PcodeOpcode::CPoolRef => {
                // Constant pool reference — return 0 (stub; real JVM analysis unsupported).
                tracing::warn!("CPOOLREF not supported; returning 0");
                if let Some(out) = &op.output {
                    let out = out.clone();
                    self.write_varnode_u64(&out, 0)?;
                }
            }
            PcodeOpcode::New => {
                // Heap allocation op (JVM/CLR) — return a fixed dummy address.
                tracing::warn!("NEW not supported; returning dummy heap 0x60000000");
                if let Some(out) = &op.output {
                    let out = out.clone();
                    self.write_varnode_u64(&out, 0x60000000)?;
                }
            }
            PcodeOpcode::CallOther => {
                // inputs[0] is a constant varnode holding the userop index.
                // inputs[1..] are the actual arguments.
                let userop_id = if let Some(vn) = op.inputs.first() {
                    if vn.is_constant {
                        vn.constant_val as u32
                    } else {
                        vn.offset as u32
                    }
                } else {
                    0
                };
                let mut input_vals = Vec::with_capacity(op.inputs.len().saturating_sub(1));
                for vn in op.inputs.iter().skip(1) {
                    let v = self.read_varnode_u64(vn).unwrap_or(0);
                    input_vals.push(v);
                }
                let output_size = op.output.as_ref().map(|o| o.size).unwrap_or(0);
                return Ok(StepResult::CallOther { userop_id, input_vals, output_size });
            }

            _ => {
                tracing::warn!("Unimplemented P-Code opcode: {:?}", op.opcode);
            }
        }
        Ok(StepResult::Next)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Read two binary inputs + output varnode reference.
    fn int_binary<'b>(&mut self, op: &'b PcodeOp) -> Result<(u64, u64, &'b Varnode)> {
        let a   = self.read_varnode_u64(&op.inputs[0])?;
        let b   = self.read_varnode_u64(&op.inputs[1])?;
        let out = op.output.as_ref().expect("Binary op must have output");
        Ok((a, b, out))
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

fn sign_extend(val: u64, size: u32) -> i64 {
    let shift = 64 - (size * 8);
    ((val as i64) << shift) >> shift
}

fn bool_u64(b: bool) -> u64 {
    if b { 1 } else { 0 }
}

/// Returns the inclusive maximum unsigned value for `size` bytes.
fn size_max_u128(size: u32) -> u128 {
    if size >= 16 { u128::MAX }
    else { (1u128 << (size * 8)) - 1 }
}

/// Returns a u64 mask covering `size` bytes.
fn size_mask_u64(size: u32) -> u64 {
    if size >= 8 { u64::MAX } else { (1u64 << (size * 8)) - 1 }
}

fn le_bytes_to_u64(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        v |= (b as u64) << (i * 8);
    }
    v
}
