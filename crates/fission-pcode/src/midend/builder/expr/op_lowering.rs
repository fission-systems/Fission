use super::*;

impl<'a> PreviewBuilder<'a> {
    /// Load element type: float when same-block non-copy consumers are
    /// float-class only. Unique temps reuse offsets across the function, so
    /// this deliberately does not scan later blocks.
    fn memory_load_type_for_output(&self, _op: &PcodeOp, output: &Varnode) -> NirType {
        let Some(site) = self.current_lowering_site else {
            return type_from_size(output.size, false);
        };
        let Some(block) = self.pcode.blocks.get(site.block_idx) else {
            return type_from_size(output.size, false);
        };
        let mut work: Vec<(usize, Varnode)> = vec![(site.op_idx, output.clone())];
        let mut saw_float = false;
        let mut saw_non_float = false;
        let mut visited = 0usize;
        while let Some((def_op_idx, vn)) = work.pop() {
            visited += 1;
            if visited > 24 {
                break;
            }
            for (use_idx, use_op) in block.ops.iter().enumerate().skip(def_op_idx + 1) {
                if !use_op.inputs.iter().any(|input| {
                    input.space_id == vn.space_id
                        && input.offset == vn.offset
                        && input.size == vn.size
                }) {
                    continue;
                }
                match use_op.opcode {
                    PcodeOpcode::Copy | PcodeOpcode::Cast => {
                        if let Some(out) = use_op.output.as_ref() {
                            work.push((use_idx, out.clone()));
                        }
                    }
                    PcodeOpcode::FloatAdd
                    | PcodeOpcode::FloatSub
                    | PcodeOpcode::FloatMult
                    | PcodeOpcode::FloatDiv
                    | PcodeOpcode::FloatNeg
                    | PcodeOpcode::FloatAbs
                    | PcodeOpcode::FloatSqrt
                    | PcodeOpcode::FloatCeil
                    | PcodeOpcode::FloatFloor
                    | PcodeOpcode::FloatRound
                    | PcodeOpcode::FloatEqual
                    | PcodeOpcode::FloatNotEqual
                    | PcodeOpcode::FloatLess
                    | PcodeOpcode::FloatLessEqual
                    | PcodeOpcode::FloatNan
                    | PcodeOpcode::FloatTrunc
                    | PcodeOpcode::FloatFloat2Float => {
                        saw_float = true;
                    }
                    // Spill/reload of the loaded value is not counter-evidence.
                    PcodeOpcode::Store => {}
                    _ => {
                        saw_non_float = true;
                    }
                }
            }
        }
        if saw_float && !saw_non_float {
            float_type_from_size(output.size)
        } else {
            type_from_size(output.size, false)
        }
    }

    pub(in crate::midend) fn lower_def_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let created_trace = if self.active_trace_id.is_none() {
            let trace_id = self.next_trace_id();
            self.active_trace_id = Some(trace_id);
            true
        } else {
            false
        };
        let result = self.lower_def_op_inner(op, visiting);
        if created_trace {
            self.last_trace_id = self.active_trace_id;
            self.active_trace_id = None;
        }
        result
    }

    fn lower_def_op_inner(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        match op.opcode {
            PcodeOpcode::Copy => self.lower_varnode(&op.inputs[0], visiting),
            PcodeOpcode::IntZExt => {
                // Zero-extend from a narrower source must first keep only the source
                // width. Classic x86 `movzx r32/r64, r8` after a wider ADD (e.g. RC4
                // keystream `(s[i]+s[j]) % 256`) is exactly this: the p-code is
                // `INT_ZEXT edx <- al` after `INT_ADD eax, …`. If the low-byte
                // truncation is lost, the sum is used as a pointer index and can
                // go out of bounds of a 256-byte table.
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let input = op
                    .inputs
                    .first()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let expr = self.lower_varnode(input, visiting)?;
                // Partial-register ZExt (`movzx r32, al` / `movzx r32, ax`) must keep a
                // source-width truncation before widening. Prefer an explicit narrow cast
                // plus AND mask so normalize cannot drop the low-byte lane when the parent
                // was a wider ADD (RC4 keystream index: `(s[i]+s[j]) % 256`).
                // Do not apply this to full-width 4→8 ZExt (ordinary zero-extend).
                if input.size > 0 && input.size <= 2 && input.size < output.size {
                    let narrow_ty = type_from_size(input.size, false);
                    let out_ty = type_from_size(output.size, false);
                    let bits = (input.size as u32).saturating_mul(8);
                    let mask = (1i64 << bits) - 1;
                    let truncated = PreHirExpr::Cast {
                        ty: narrow_ty,
                        expr: Box::new(expr),
                    };
                    return Ok(PreHirExpr::Binary {
                        op: PreHirBinaryOp::And,
                        lhs: Box::new(truncated),
                        rhs: Box::new(PreHirExpr::Const(mask, out_ty.clone())),
                        ty: out_ty,
                    });
                }
                Ok(PreHirExpr::Cast {
                    ty: type_from_size(output.size, false),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Cast | PcodeOpcode::IntSExt => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(PreHirExpr::Cast {
                    ty: type_from_size(output.size, matches!(op.opcode, PcodeOpcode::IntSExt)),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Load => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::UnsupportedExprMemoryBackedVarnode);
                }
                let out = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprMemoryBackedVarnode)?;
                // When this load's only consumers are float ops (or copies into
                // float ops), recover float element type instead of uint-by-size.
                let load_ty = self.memory_load_type_for_output(op, out);
                if let Some((slot_name, _)) =
                    self.try_stack_slot_lvalue_for_memory_op(op, &op.inputs[1], load_ty.clone())
                {
                    Ok(PreHirExpr::Var(slot_name))
                } else if let Some(peb_expr) = self.try_peb_field_var(&op.inputs[1]) {
                    Ok(peb_expr)
                } else if let Some(teb_expr) = self.try_teb_field_var(&op.inputs[1]) {
                    Ok(teb_expr)
                } else if let Some(global) = self.resolve_relocated_load_pointer(op, 16) {
                    Ok(if global.byte_offset == 0 {
                        PreHirExpr::AddressOfGlobal(global.name)
                    } else {
                        PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::AddressOfGlobal(global.name)),
                            offset: global.byte_offset,
                        }
                    })
                } else if let Some(addr) = self.resolve_global_address(&op.inputs[1], 16)
                    && let Some(value) = self.read_readonly_scalar_from_binary(addr, out.size)
                {
                    Ok(PreHirExpr::Const(value as i64, load_ty))
                } else {
                    Ok(PreHirExpr::Load {
                        ptr: Box::new(self.lower_memory_pointer(&op.inputs[1], visiting)?),
                        ty: load_ty,
                    })
                }
            }
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => self.lower_ptr_op(op, visiting),
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::FloatAdd
            | PcodeOpcode::FloatDiv
            | PcodeOpcode::FloatMult
            | PcodeOpcode::FloatSub
            | PcodeOpcode::FloatEqual
            | PcodeOpcode::FloatNotEqual
            | PcodeOpcode::FloatLess
            | PcodeOpcode::FloatLessEqual => self.lower_binary_op(op, visiting),
            PcodeOpcode::FloatInt2Float | PcodeOpcode::FloatFloat2Float => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(PreHirExpr::Cast {
                    ty: float_type_from_size(output.size),
                    expr: Box::new(expr),
                })
            }
            // `FLOAT_TRUNC` is a float-to-*integer* truncating conversion
            // (Ghidra's own `TypeOpFloatTrunc` declares output metatype
            // `TYPE_INT`, input `TYPE_FLOAT` -- unlike CEIL/FLOOR/ROUND
            // below, which stay float-to-float) -- i.e. `(int)x`, not
            // `trunc(x)`.
            PcodeOpcode::FloatTrunc => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(PreHirExpr::Cast {
                    ty: type_from_size(output.size, true),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::FloatNeg => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                self.note_operand_metatypes(op.opcode, &[&expr]);
                Ok(PreHirExpr::Unary {
                    op: PreHirUnaryOp::Neg,
                    expr: Box::new(expr),
                    ty: float_type_from_size(output.size),
                })
            }
            // ABS/SQRT/CEIL/FLOOR/ROUND stay float-to-float (Ghidra's own
            // `TypeOpFunc` declarations all use `TYPE_FLOAT, TYPE_FLOAT`)
            // and, unlike the CPU-flag-level intrinsics below
            // (`__carry`/`__sborrow`, which have no real C equivalent),
            // these correspond exactly to real `<math.h>` functions, so
            // they're rendered under their real libc names rather than a
            // synthetic `__`-prefixed marker.
            PcodeOpcode::FloatAbs
            | PcodeOpcode::FloatSqrt
            | PcodeOpcode::FloatCeil
            | PcodeOpcode::FloatFloor
            | PcodeOpcode::FloatRound => {
                let ty = op
                    .output
                    .as_ref()
                    .map(|out| float_type_from_size(out.size))
                    .unwrap_or(NirType::Unknown);
                let name = match op.opcode {
                    PcodeOpcode::FloatAbs => "fabs",
                    PcodeOpcode::FloatSqrt => "sqrt",
                    PcodeOpcode::FloatCeil => "ceil",
                    PcodeOpcode::FloatFloor => "floor",
                    PcodeOpcode::FloatRound => "round",
                    _ => unreachable!(),
                };
                self.lower_intrinsic_call(op, visiting, name, ty)
            }
            PcodeOpcode::FloatNan => {
                self.lower_intrinsic_call(op, visiting, "__isnan", NirType::Bool)
            }
            PcodeOpcode::IntNegate | PcodeOpcode::BoolNegate | PcodeOpcode::Int2Comp => {
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                self.note_operand_metatypes(op.opcode, &[&expr]);
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let ty = type_from_size(output.size, false);
                let op = match op.opcode {
                    PcodeOpcode::IntNegate => PreHirUnaryOp::BitNot,
                    PcodeOpcode::BoolNegate => PreHirUnaryOp::Not,
                    PcodeOpcode::Int2Comp => PreHirUnaryOp::Neg,
                    _ => return Err(MlilPreviewError::UnsupportedExprVarnodeLowering),
                };
                Ok(PreHirExpr::Unary {
                    op,
                    expr: Box::new(expr),
                    ty,
                })
            }
            PcodeOpcode::IntCarry => {
                self.lower_intrinsic_call(op, visiting, "__carry", NirType::Bool)
            }
            PcodeOpcode::IntSCarry => {
                self.lower_intrinsic_call(op, visiting, "__scarry", NirType::Bool)
            }
            PcodeOpcode::IntSBorrow => {
                self.lower_intrinsic_call(op, visiting, "__sborrow", NirType::Bool)
            }
            PcodeOpcode::PopCount => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__popcount",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::LzCount => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__lzcnt",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                self.lower_call(op, None, visiting)
            }
            PcodeOpcode::Piece => self.lower_piece_op(op, visiting),
            PcodeOpcode::SubPiece => self.lower_subpiece_op(op, visiting),
            PcodeOpcode::MultiEqual => self.lower_multiequal(op, visiting),
            PcodeOpcode::Indirect => {
                if let Some(input) = op.inputs.first() {
                    self.lower_varnode(input, visiting)
                } else {
                    Err(MlilPreviewError::UnsupportedExprIndirectValueSource)
                }
            }
            _ => {
                self.record_unsupported_inventory_event(
                    "lower_def_op_unsupported",
                    op.output.as_ref(),
                    Some(op),
                    Some(op.opcode),
                    self.current_lowering_site
                        .map(|site| self.pcode.blocks[site.block_idx].start_address),
                    Some(u64::from(op.seq_num)),
                    false,
                    "opcode_not_lowered",
                );
                self.debug_preview_log(&format!(
                    "[mlil-preview] stage=lower_def_op_unsupported opcode={:?} asm={}\n",
                    op.opcode,
                    op.asm_mnemonic.as_deref().unwrap_or("<none>")
                ));
                Err(MlilPreviewError::UnsupportedPattern("opcode"))
            }
        }
    }

    pub(in crate::midend) fn lower_multiequal(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let mut lowered: Vec<Option<PreHirExpr>> = Vec::with_capacity(op.inputs.len());
        for input in &op.inputs {
            match self.lower_varnode(input, visiting) {
                Ok(expr) => lowered.push(Some(expr)),
                Err(_) => lowered.push(None),
            }
        }

        // Collect only the successfully-lowered expressions.
        let resolved: Vec<&PreHirExpr> = lowered.iter().filter_map(Option::as_ref).collect();

        if resolved.is_empty() {
            // All inputs failed — nothing to coalesce.
            return Err(MlilPreviewError::UnsupportedExprMultiequal);
        }

        // Check whether all successfully-resolved inputs have the same
        // canonical expression (ignoring cast wrappers).  If so, that value
        // is the definitive join — this covers both the "all-same" case and
        // the "partial failure with a unique surviving value" case (e.g. one
        // predecessor is a loop back-edge whose def-chain failed because the
        // back-edge varnode traces to the same MultiEqual, and the other
        // predecessor resolves to the function-entry value).
        let canonical = strip_casts(resolved[0]);
        if resolved.iter().all(|e| strip_casts(e) == canonical) {
            return Ok(resolved[0].clone());
        }

        Err(MlilPreviewError::UnsupportedExprMultiequal)
    }

    /// Lower the pointer operand of a memory access.
    ///
    /// Same as `lower_varnode`, except frame arithmetic stays arithmetic:
    /// `&local` is what the address *value* means, and putting it under the
    /// dereference the access already carries only spells the same slot twice.
    pub(in crate::midend) fn lower_memory_pointer(
        &mut self,
        ptr: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let outer = std::mem::replace(&mut self.lowering_memory_pointer, true);
        let lowered = self.lower_varnode(ptr, visiting);
        self.lowering_memory_pointer = outer;
        lowered
    }

    pub(in crate::midend) fn lower_ptr_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if let Some(address) = self.stack_local_address_expr(op) {
            return Ok(address);
        }
        let base = self.lower_varnode(&op.inputs[0], visiting)?;
        let offset = if op.inputs.len() > 1 && op.inputs[1].is_constant {
            op.inputs[1].constant_val
        } else {
            0
        };
        if op.opcode == PcodeOpcode::PtrAdd && op.inputs.len() > 2 && op.inputs[2].is_constant {
            let index = self.lower_varnode(&op.inputs[1], visiting)?;
            let elem_ty = type_from_size(op.inputs[2].constant_val as u32, false);
            return Ok(PreHirExpr::Index {
                base: Box::new(base),
                index: Box::new(index),
                elem_ty,
            });
        }
        if (op.opcode == PcodeOpcode::PtrAdd || op.opcode == PcodeOpcode::PtrSub)
            && op.inputs.len() > 1
            && !op.inputs[1].is_constant
        {
            let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprPtrArithmetic)?;
            let arith_op = if op.opcode == PcodeOpcode::PtrAdd {
                PreHirBinaryOp::Add
            } else {
                PreHirBinaryOp::Sub
            };
            return Ok(PreHirExpr::Binary {
                op: arith_op,
                lhs: Box::new(base),
                rhs: Box::new(rhs),
                ty: type_from_size(output.size, false),
            });
        }
        Ok(PreHirExpr::PtrOffset {
            base: Box::new(base),
            offset,
        })
    }

    /// Record the operand metatype `opcode` implies for whatever names its
    /// inputs lowered to.
    ///
    /// Only plain variable references carry evidence: a constant has its own
    /// type, and a compound expression's type comes from its own operator.
    /// Evidence is recorded, never applied here -- `apply_operand_metatypes`
    /// decides whether it beats what the binding already has.
    pub(super) fn note_operand_metatypes(&mut self, opcode: PcodeOpcode, operands: &[&PreHirExpr]) {
        let Some(meta) = pcode_input_metatype(opcode) else {
            return;
        };
        for operand in operands {
            if let PreHirExpr::Var(name) = operand {
                // First writer wins: a name used by two ops with conflicting
                // metatypes is ambiguous, and picking the later one would make
                // the result depend on lowering order.
                self.operand_metatypes.entry(name.clone()).or_insert(meta);
            }
        }
    }

    pub(in crate::midend) fn lower_binary_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::UnsupportedExprVarnodeLowering);
        }
        // Frame arithmetic that lands on a known local is that local's
        // address, not a number computed from a register the output never
        // declares.
        if let Some(address) = self.stack_local_address_expr(op) {
            return Ok(address);
        }
        if op.opcode == PcodeOpcode::IntXor
            && VarnodeKey::from(&op.inputs[0]) == VarnodeKey::from(&op.inputs[1])
        {
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
            return Ok(PreHirExpr::Const(0, type_from_size(output.size, false)));
        }
        // x86 CDQ + IDIV: dividend is Piece(sign_fill(L), L). Use signed L alone.
        if matches!(op.opcode, PcodeOpcode::IntSRem | PcodeOpcode::IntSDiv)
            && let Some(low) = self.try_cdq_signed_dividend_low(&op.inputs[0])
        {
            let lhs = self.lower_varnode(&low, visiting)?;
            let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
            // Remainder/quotient lane is the machine width of the low half (EAX),
            // not the 64-bit piece — match signed C `%` / `/` on that width.
            let bits = low
                .size
                .saturating_mul(8)
                .max(output.size.saturating_mul(8));
            let ty = NirType::Int { bits, signed: true };
            let lhs = PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(lhs),
            };
            let rhs = PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(rhs),
            };
            return Ok(PreHirExpr::Binary {
                op: map_binary_op(op.opcode)?,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                ty,
            });
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        self.note_operand_metatypes(op.opcode, &[&lhs, &rhs]);
        let (lhs, rhs) = if matches!(op.opcode, PcodeOpcode::IntLess | PcodeOpcode::IntLessEqual) {
            let bits = op.inputs[0].size.saturating_mul(8);
            (
                self.coerce_unsigned_compare_operand(lhs, bits),
                self.coerce_unsigned_compare_operand(rhs, bits),
            )
        } else {
            (lhs, rhs)
        };
        let output = op
            .output
            .as_ref()
            .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
        let ty = if is_comparison(op.opcode) {
            NirType::Bool
        } else if matches!(
            op.opcode,
            PcodeOpcode::FloatAdd
                | PcodeOpcode::FloatDiv
                | PcodeOpcode::FloatMult
                | PcodeOpcode::FloatSub
        ) {
            float_type_from_size(output.size)
        } else {
            pcode_output_type_from_size(op.opcode, output.size)
        };
        Ok(PreHirExpr::Binary {
            op: map_binary_op(op.opcode)?,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        })
    }

    /// Preserve the bit-vector interpretation of a compound signed value when
    /// an x86 flag recovery path reconstructs an unsigned comparison.  Leave
    /// unknown/atomic operands untouched; their eventual ABI/type recovery is
    /// the source of truth and blindly adding `(uint)` to every register pair
    /// changes the established output shape.
    pub(in crate::midend) fn coerce_unsigned_compare_operand(
        &self,
        expr: PreHirExpr,
        bits: u32,
    ) -> PreHirExpr {
        if matches!(expr, PreHirExpr::Var(_)) {
            return expr;
        }
        let NirType::Int {
            bits: source_bits,
            signed: true,
        } = expr_type(&expr)
        else {
            return expr;
        };
        if source_bits != bits {
            return expr;
        }
        PreHirExpr::Cast {
            ty: NirType::Int {
                bits: bits.max(1),
                signed: false,
            },
            expr: Box::new(expr),
        }
    }

    /// CDQ-class dividend low half for signed rem/div.
    ///
    /// Recognized forms (x86 `cdq; idiv` / SLEIGH):
    /// - `Piece(H, L)` with `H` arithmetic sign-fill of `L`
    /// - direct `IntSExt(L)` as the wide dividend
    /// - `IntOr(IntLeft(H, k), L')` with `k == 8*sizeof(L)`, `L'` zero/copy of `L`,
    ///   and `H` the high half of `IntSExt(L)` (SubPiece) or other CDQ sign-fill
    ///
    /// Returns `L` so SRem/SDiv lower as C signed `%`/`/` on that half-width.
    fn try_cdq_signed_dividend_low(&self, dividend: &Varnode) -> Option<Varnode> {
        // Peel Copy/Cast wrappers on the dividend itself (some templates stage
        // the wide Or into a unique via Copy before SRem).
        let mut current = dividend.clone();
        for _ in 0..6 {
            let (_, def) = self.lookup_def_site(&current)?;
            match def.opcode {
                PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    current = def.inputs.first()?.clone();
                    continue;
                }
                PcodeOpcode::IntSExt => return def.inputs.first().cloned(),
                PcodeOpcode::Piece if def.inputs.len() >= 2 => {
                    let high = &def.inputs[0];
                    let low = &def.inputs[1];
                    return if self.varnode_is_sign_fill_of(high, low) {
                        Some(low.clone())
                    } else {
                        None
                    };
                }
                // SLEIGH idiv form: (ZExt(hi) << 32) | ZExt(lo) via IntOr/IntLeft.
                PcodeOpcode::IntOr if def.inputs.len() >= 2 => {
                    return self
                        .try_cdq_low_from_or_shl_dividend(&def.inputs[0], &def.inputs[1])
                        .or_else(|| {
                            self.try_cdq_low_from_or_shl_dividend(&def.inputs[1], &def.inputs[0])
                        });
                }
                _ => return None,
            }
        }
        None
    }

    /// Match `shifted_high | low_ext` where `shifted_high` is `IntLeft(H, k)`
    /// (after peeling ZExt/Copy on the left arm) with `k` equal to the low half
    /// width in bits, and `H` is CDQ sign-fill of the core of `low_ext`.
    fn try_cdq_low_from_or_shl_dividend(
        &self,
        shifted_high: &Varnode,
        low_ext: &Varnode,
    ) -> Option<Varnode> {
        // High arm may be ZExt/Copy of the IntLeft result in some SLEIGH paths.
        let left_vn = self.peel_cdq_width_ext(shifted_high)?;
        let (_, left_def) = self.lookup_def_site(&left_vn)?;
        if left_def.opcode != PcodeOpcode::IntLeft || left_def.inputs.len() < 2 {
            return None;
        }
        let high = &left_def.inputs[0];
        let shift = &left_def.inputs[1];
        if !shift.is_constant {
            return None;
        }
        let shift_amt = shift.constant_val as u32;
        // Peel ZExt/Copy/Cast on the low side to the machine-width half.
        let low = self.peel_cdq_width_ext(low_ext)?;
        let low_bits = u32::from(low.size.saturating_mul(8));
        if shift_amt != low_bits && shift_amt != 32 && shift_amt != 64 {
            return None;
        }
        if !self.varnode_is_sign_fill_of(high, &low) {
            return None;
        }
        Some(low)
    }

    /// Peel ZExt / Copy / Cast wrappers (width promotion of the low/high half).
    fn peel_cdq_width_ext(&self, vn: &Varnode) -> Option<Varnode> {
        let mut current = vn.clone();
        for _ in 0..6 {
            let Some((_, op)) = self.lookup_def_site(&current) else {
                return Some(current);
            };
            match op.opcode {
                PcodeOpcode::IntZExt | PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    current = op.inputs.first()?.clone();
                }
                _ => return Some(current),
            }
        }
        Some(current)
    }

    /// True when `high` is CDQ-class **arithmetic** sign-fill of `low`.
    ///
    /// Accept:
    /// - `IntSRight` of `low` (or short ZExt/SExt/Copy/Cast chain to `low`)
    /// - `IntSExt` of `low` (after peeling Copy/Cast wrappers)
    /// - `SubPiece(IntSExt(low), offset == low.size)` — SLEIGH CDQ high half
    /// - `IntZExt` / `Copy` / `Cast` wrappers **around the above only**
    ///
    /// Reject bare `Copy(low)`, bare `IntZExt(low)`, or logical `IntRight` alone —
    /// peeling wrappers must land on SAR/SExt/SubPiece(SExt), never on `low` itself.
    fn varnode_is_sign_fill_of(&self, high: &Varnode, low: &Varnode) -> bool {
        let low_key = VarnodeKey::from(low);
        let mut current = high.clone();
        for _ in 0..8 {
            // High reduced to exact `low` without an intervening SAR/SExt/
            // SubPiece(SExt) is not sign-fill (e.g. Piece(Copy(L), L)).
            // Use exact key match only — register alias helpers can be broader
            // than "same value" and must not reject SubPiece high halves.
            if VarnodeKey::from(&current) == low_key {
                return false;
            }
            let Some((_, hop)) = self.lookup_def_site(&current) else {
                return false;
            };
            match hop.opcode {
                // Only arithmetic right-shift is CDQ-class sign fill via shift.
                PcodeOpcode::IntSRight => {
                    let Some(base) = hop.inputs.first() else {
                        return false;
                    };
                    if !self.varnode_related_to_cdq_low(base, low) {
                        return false;
                    }
                    let Some(shift) = hop.inputs.get(1) else {
                        return false;
                    };
                    if !shift.is_constant {
                        return false;
                    }
                    let shift_amt = shift.constant_val as i64;
                    // CDQ: SAR L, 31  or SAR (sext L), 32  (word-width shift).
                    let bits = i64::from(low.size.saturating_mul(8));
                    return shift_amt == bits - 1
                        || shift_amt == bits
                        || shift_amt == 31
                        || shift_amt == 63;
                }
                // Full signed widen of low is a valid wide-dividend form.
                PcodeOpcode::IntSExt => {
                    return hop
                        .inputs
                        .first()
                        .is_some_and(|base| self.varnode_related_to_cdq_low(base, low));
                }
                // High half of sign-extend: SubPiece(SExt(L), offset == |L|).
                PcodeOpcode::SubPiece if hop.inputs.len() >= 2 => {
                    let base = &hop.inputs[0];
                    let offset = &hop.inputs[1];
                    if !offset.is_constant {
                        return false;
                    }
                    let off = offset.constant_val as u64;
                    if off != u64::from(low.size) {
                        return false;
                    }
                    // base must be IntSExt(low) (possibly through Copy/Cast).
                    let mut sext_vn = base.clone();
                    for _ in 0..4 {
                        let Some((_, sop)) = self.lookup_def_site(&sext_vn) else {
                            return false;
                        };
                        match sop.opcode {
                            PcodeOpcode::IntSExt => {
                                return sop
                                    .inputs
                                    .first()
                                    .is_some_and(|b| self.varnode_related_to_cdq_low(b, low));
                            }
                            PcodeOpcode::Copy | PcodeOpcode::Cast => {
                                let Some(input) = sop.inputs.first() else {
                                    return false;
                                };
                                sext_vn = input.clone();
                            }
                            _ => return false,
                        }
                    }
                    return false;
                }
                // Peel width/copy wrappers on the high side only — never treat
                // the peeled-to `low` as fill (checked at loop head).
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt => {
                    let Some(input) = hop.inputs.first() else {
                        return false;
                    };
                    current = input.clone();
                }
                // Logical right-shift alone is not sign-fill.
                PcodeOpcode::IntRight => return false,
                _ => return false,
            }
        }
        false
    }

    /// `base` is `low`, an alias, or a short ZExt/SExt/Copy/Cast chain from `low`.
    /// Used only as the *input* of IntSRight / IntSExt / SubPiece(SExt), not as high itself.
    fn varnode_related_to_cdq_low(&self, base: &Varnode, low: &Varnode) -> bool {
        if self.varnode_aliases_value(base, low) || VarnodeKey::from(base) == VarnodeKey::from(low)
        {
            return true;
        }
        let mut current = base.clone();
        for _ in 0..4 {
            let Some((_, op)) = self.lookup_def_site(&current) else {
                return false;
            };
            match op.opcode {
                PcodeOpcode::IntSExt
                | PcodeOpcode::IntZExt
                | PcodeOpcode::Copy
                | PcodeOpcode::Cast => {
                    let Some(input) = op.inputs.first() else {
                        return false;
                    };
                    if self.varnode_aliases_value(input, low)
                        || VarnodeKey::from(input) == VarnodeKey::from(low)
                    {
                        return true;
                    }
                    current = input.clone();
                }
                _ => return false,
            }
        }
        false
    }
}
