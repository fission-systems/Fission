use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::collections::{BTreeMap, HashMap, HashSet};

mod cfg;
mod normalize;
mod piece;
mod printer;
mod structuring;
mod types;
#[cfg(test)]
mod tests;

pub use self::types::*;
use self::{cfg::*, normalize::*, printer::*};

const UNIQUE_SPACE_ID: u64 = 1;
const REGISTER_SPACE_ID: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackBase {
    Rsp,
    Rbp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StackSlot {
    id: StackSlotId,
    name: String,
    ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VarnodeKey {
    space_id: u64,
    offset: u64,
    size: u32,
    is_constant: bool,
    constant_val: i64,
}

impl From<&Varnode> for VarnodeKey {
    fn from(value: &Varnode) -> Self {
        Self {
            space_id: value.space_id,
            offset: value.offset,
            size: value.size,
            is_constant: value.is_constant,
            constant_val: value.constant_val,
        }
    }
}

#[derive(Debug)]
struct PreviewBuilder<'a> {
    pcode: &'a PcodeFunction,
    options: &'a MlilPreviewOptions,
    type_context: Option<&'a PreviewTypeContext>,
    defs: HashMap<VarnodeKey, &'a PcodeOp>,
    address_to_index: HashMap<u64, usize>,
    layout_fallthrough: Vec<Option<usize>>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    params: BTreeMap<usize, NirBinding>,
    locals: BTreeMap<i64, StackSlot>,
    locals_next_id: StackSlotId,
    temps: BTreeMap<String, NirBinding>,
    temp_next_id: u32,
    materialized_vns: HashMap<VarnodeKey, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinearExit {
    Join(usize),
    Return,
    End,
}

#[derive(Debug, Clone)]
struct SubpieceOrigin {
    base: VarnodeKey,
    base_vn: Varnode,
    base_size: u32,
    byte_offset: i64,
    piece_size: u32,
}

pub fn render_mlil_preview(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, None)
}

pub fn render_mlil_preview_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Result<String, MlilPreviewError> {
    if options.pe_x64_only && !options.is_pe_x64() {
        return Err(MlilPreviewError::UnsupportedArchitecture);
    }

    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    let mut builder = PreviewBuilder::new(pcode, options, type_context);
    let mut hir = builder.build_hir(name, address)?;
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=normalize start fn=0x{address:x}");
    }
    normalize_function_body(&mut hir.body);
    if let Some(context) = type_context {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        apply_preview_type_hints(&mut hir, context);
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    Ok(print_hir_function(&hir))
}

impl<'a> PreviewBuilder<'a> {
    fn new(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        let mut defs = HashMap::new();
        for block in &pcode.blocks {
            for op in &block.ops {
                if let Some(output) = &op.output {
                    defs.insert(VarnodeKey::from(output), op);
                }
            }
        }
        let address_to_index = pcode
            .blocks
            .iter()
            .enumerate()
            .map(|(idx, block)| (block.start_address, idx))
            .collect::<HashMap<_, _>>();
        let layout_fallthrough = build_layout_fallthrough_map(pcode);
        let successors = build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        let predecessors = build_predecessor_index_map(&successors);
        Self {
            pcode,
            options,
            type_context,
            defs,
            address_to_index,
            layout_fallthrough,
            successors,
            predecessors,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            temps: BTreeMap::new(),
            temp_next_id: 0,
            materialized_vns: HashMap::new(),
        }
    }

    fn build_hir(&mut self, name: &str, _address: u64) -> Result<HirFunction, MlilPreviewError> {
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

        let mut body = Vec::new();
        if self.pcode.blocks.len() == 1 {
            let block = &self.pcode.blocks[0];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(0)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => body.push(HirStmt::Goto(block_label(target))),
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => body.push(HirStmt::If {
                    cond,
                    then_body: vec![HirStmt::Goto(block_label(true_target))],
                    else_body: false_target
                        .map(block_label)
                        .map(HirStmt::Goto)
                        .into_iter()
                        .collect(),
                }),
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                }
            }
        } else {
            body = self.build_multiblock_body()?;
        }

        let return_type = body
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr_type(expr)),
                HirStmt::Return(None) => Some(NirType::Unknown),
                _ => None,
            })
            .unwrap_or(NirType::Unknown);

        Ok(HirFunction {
            name: name.to_string(),
            params: self.params.values().cloned().collect(),
            locals: self
                .locals
                .values()
                .map(|slot| NirBinding {
                    name: slot.name.clone(),
                    ty: slot.ty.clone(),
                    surface_type_name: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            body,
        })
    }

    fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }
            match op.opcode {
                PcodeOpcode::Store => {
                    if op.inputs.len() < 3 {
                        return Err(MlilPreviewError::LoweringFailed);
                    }
                    let lhs = if let Some((slot_name, _slot_ty)) = self.try_stack_slot_lvalue(
                        &op.inputs[1],
                        type_from_size(op.inputs[2].size, false),
                    ) {
                        HirLValue::Var(slot_name)
                    } else {
                        HirLValue::Deref {
                            ptr: Box::new(self.lower_varnode(&op.inputs[1], &mut HashSet::new())?),
                            ty: type_from_size(op.inputs[2].size, false),
                        }
                    };
                    let rhs = self.lower_varnode(&op.inputs[2], &mut HashSet::new())?;
                    body.push(HirStmt::Assign { lhs, rhs });
                }
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                    if op.output.is_none() {
                        let expr = self.lower_call(op, &mut HashSet::new())?;
                        body.push(HirStmt::Expr(expr));
                    } else if let Some(stmt) =
                        self.maybe_materialize_output_stmt(block, op_idx, terminator_index, op)?
                    {
                        body.push(stmt);
                    }
                }
                _ => {
                    if let Some(stmt) =
                        self.maybe_materialize_output_stmt(block, op_idx, terminator_index, op)?
                    {
                        body.push(stmt);
                    }
                }
            }
        }
        Ok(body)
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if self.output_used_only_by_block_terminator(block, op_idx, terminator_index, output) {
            return Ok(None);
        }
        self.materialize_output_stmt(op)
    }

    fn materialize_output_stmt(
        &mut self,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let rhs = self.lower_def_op(op, &mut HashSet::new())?;
        let lhs = HirLValue::Var(self.ensure_temp_binding_for_output(output).name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn output_used_only_by_block_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let mut use_sites = block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter(|(_, candidate)| {
                candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
            })
            .map(|(idx, _)| idx);

        let Some(first_use) = use_sites.next() else {
            return false;
        };
        if use_sites.next().is_some() {
            return false;
        }
        Some(first_use) == terminator_index
    }

    fn block_terminator_index(&self, block: &crate::pcode::PcodeBasicBlock) -> Option<usize> {
        block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        let block = &self.pcode.blocks[idx];
        let Some(term_idx) = self.block_terminator_index(block) else {
            return Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx)));
        };
        let op = &block.ops[term_idx];
        match op.opcode {
            PcodeOpcode::Return => {
                let expr = op
                    .inputs
                    .last()
                    .map(|input| self.lower_varnode(input, &mut HashSet::new()))
                    .transpose()?;
                Ok(LoweredTerminator::Return(expr))
            }
            PcodeOpcode::Branch if op.inputs.len() == 1 => {
                let Some(target) = op.inputs.first().and_then(branch_target_address) else {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                };
                Ok(LoweredTerminator::Goto(target))
            }
            PcodeOpcode::CBranch | PcodeOpcode::Branch if op.inputs.len() >= 2 => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::LoweringFailed);
                }
                let Some(true_target) = branch_target_address(&op.inputs[0]) else {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                };
                let cond = self.lower_varnode(&op.inputs[1], &mut HashSet::new())?;
                Ok(LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target: self.next_block_address(idx),
                })
            }
            PcodeOpcode::BranchInd => Ok(LoweredTerminator::Unsupported),
            _ => Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx))),
        }
    }

    fn next_block_address(&self, idx: usize) -> Option<u64> {
        self.layout_fallthrough[idx].map(|next_idx| self.pcode.blocks[next_idx].start_address)
    }

    fn lower_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let target = if let Some(target) = op.inputs.first() {
            match self.lower_varnode(target, visiting)? {
                HirExpr::Const(val, _) => self
                    .type_context
                    .and_then(|ctx| ctx.call_targets.get(&(val as u64)).cloned())
                    .unwrap_or_else(|| format!("sub_{:x}", val as u64)),
                HirExpr::Var(name) => name,
                other => print_expr(&other),
            }
        } else {
            "callee".to_string()
        };
        let args = op
            .inputs
            .iter()
            .skip(1)
            .map(|input| self.lower_varnode(input, &mut HashSet::new()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HirExpr::Call {
            target,
            args,
            ty: op
                .output
                .as_ref()
                .map(|out| type_from_size(out.size, false))
                .unwrap_or(NirType::Unknown),
        })
    }

    fn lower_intrinsic_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
        target: &str,
        ty: NirType,
    ) -> Result<HirExpr, MlilPreviewError> {
        let args = op
            .inputs
            .iter()
            .map(|input| self.lower_varnode(input, visiting))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HirExpr::Call {
            target: target.to_string(),
            args,
            ty,
        })
    }

    fn lower_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if vn.is_constant {
            return Ok(HirExpr::Const(
                vn.constant_val,
                type_from_size(vn.size, false),
            ));
        }

        if let Some(param) = self.register_param(vn) {
            return Ok(HirExpr::Var(param));
        }

        if vn.space_id == REGISTER_SPACE_ID {
            return Ok(HirExpr::Var(register_name(vn.offset, vn.size).to_string()));
        }

        let key = VarnodeKey::from(vn);
        if let Some(name) = self.materialized_vns.get(&key) {
            return Ok(HirExpr::Var(name.clone()));
        }
        if !visiting.insert(key.clone()) {
            return Ok(HirExpr::Var(format!("tmp_{:x}", vn.offset)));
        }

        let result = match self.defs.get(&key).copied() {
            Some(op) => self.lower_def_op(op, visiting),
            None if vn.space_id == UNIQUE_SPACE_ID => {
                Ok(HirExpr::Var(format!("tmp_{:x}", vn.offset)))
            }
            None if self.options.is_mapped_global(vn.offset) => {
                Ok(HirExpr::Var(format!("DAT_{:x}", vn.offset)))
            }
            None => Ok(HirExpr::Var(format!("var_{:x}", vn.offset))),
        };
        visiting.remove(&key);
        result
    }

    fn lower_def_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match op.opcode {
            PcodeOpcode::Copy => self.lower_varnode(&op.inputs[0], visiting),
            PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(HirExpr::Cast {
                    ty: type_from_size(output.size, matches!(op.opcode, PcodeOpcode::IntSExt)),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Load => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::LoweringFailed);
                }
                let out = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                if let Some((slot_name, _)) =
                    self.try_stack_slot_lvalue(&op.inputs[1], type_from_size(out.size, false))
                {
                    Ok(HirExpr::Var(slot_name))
                } else {
                    Ok(HirExpr::Load {
                        ptr: Box::new(self.lower_varnode(&op.inputs[1], visiting)?),
                        ty: type_from_size(out.size, false),
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
            | PcodeOpcode::BoolXor => self.lower_binary_op(op, visiting),
            PcodeOpcode::IntNegate | PcodeOpcode::BoolNegate | PcodeOpcode::Int2Comp => {
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                let ty = type_from_size(output.size, false);
                let op = match op.opcode {
                    PcodeOpcode::IntNegate => HirUnaryOp::BitNot,
                    PcodeOpcode::BoolNegate => HirUnaryOp::Not,
                    PcodeOpcode::Int2Comp => HirUnaryOp::Neg,
                    _ => return Err(MlilPreviewError::LoweringFailed),
                };
                Ok(HirExpr::Unary {
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
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__popcount",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                self.lower_call(op, visiting)
            }
            PcodeOpcode::Piece => self.lower_piece_op(op, visiting),
            PcodeOpcode::SubPiece => self.lower_subpiece_op(op, visiting),
            PcodeOpcode::MultiEqual => self.lower_multiequal(op, visiting),
            PcodeOpcode::Indirect => {
                if let Some(input) = op.inputs.first() {
                    self.lower_varnode(input, visiting)
                } else {
                    Err(MlilPreviewError::LoweringFailed)
                }
            }
            _ => Err(MlilPreviewError::UnsupportedPattern("opcode")),
        }
    }

    fn lower_multiequal(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let mut lowered = Vec::new();
        for input in &op.inputs {
            lowered.push(self.lower_varnode(input, visiting)?);
        }
        if let Some(first) = lowered.first() {
            let canonical = strip_casts(first);
            if lowered.iter().all(|expr| strip_casts(expr) == canonical) {
                return Ok(first.clone());
            }
        }
        Err(MlilPreviewError::UnsupportedControlFlow)
    }

    fn lower_ptr_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let base = self.lower_varnode(&op.inputs[0], visiting)?;
        let offset = if op.inputs.len() > 1 && op.inputs[1].is_constant {
            op.inputs[1].constant_val
        } else {
            0
        };
        if op.opcode == PcodeOpcode::PtrAdd && op.inputs.len() > 2 && op.inputs[2].is_constant {
            let index = op.inputs[1].constant_val as usize;
            let elem_ty = type_from_size(op.inputs[2].constant_val as u32, false);
            return Ok(HirExpr::Index {
                base: Box::new(base),
                index,
                elem_ty,
            });
        }
        Ok(HirExpr::PtrOffset {
            base: Box::new(base),
            offset,
        })
    }

    fn lower_binary_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::LoweringFailed);
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
        let ty = if is_comparison(op.opcode) {
            NirType::Bool
        } else {
            type_from_size(
                output.size,
                matches!(
                    op.opcode,
                    PcodeOpcode::IntSDiv
                        | PcodeOpcode::IntSRem
                        | PcodeOpcode::IntSLess
                        | PcodeOpcode::IntSLessEqual
                ),
            )
        };
        Ok(HirExpr::Binary {
            op: map_binary_op(op.opcode)?,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        })
    }

    fn register_param(&mut self, vn: &Varnode) -> Option<String> {
        if vn.space_id != REGISTER_SPACE_ID {
            return None;
        }
        let (name, index) = register_name_with_param(vn.offset, vn.size)?;
        if let Some(index) = index {
            self.params.entry(index).or_insert_with(|| NirBinding {
                name: name.to_string(),
                ty: type_from_size(vn.size, false),
                surface_type_name: None,
            });
        }
        Some(name.to_string())
    }

    fn try_stack_slot_lvalue(&mut self, ptr: &Varnode, ty: NirType) -> Option<(String, NirType)> {
        let (base, offset) = self.resolve_stack_address(ptr)?;
        let kind_name = match base {
            StackBase::Rbp if offset > 0 => format!("param_{:x}", offset),
            StackBase::Rbp => format!("local_{:x}", offset.unsigned_abs()),
            StackBase::Rsp => format!("local_{:x}", offset.unsigned_abs()),
        };

        let entry = self.locals.entry(offset).or_insert_with(|| {
            let id = self.locals_next_id;
            self.locals_next_id += 1;
            StackSlot {
                id,
                name: kind_name.clone(),
                ty: ty.clone(),
            }
        });
        if entry.ty == NirType::Unknown {
            entry.ty = ty.clone();
        }
        Some((entry.name.clone(), entry.ty.clone()))
    }

    fn resolve_stack_address(&self, ptr: &Varnode) -> Option<(StackBase, i64)> {
        self.resolve_stack_address_inner(ptr, &mut HashSet::new())
    }

    fn resolve_stack_address_inner(
        &self,
        ptr: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<(StackBase, i64)> {
        if ptr.space_id == REGISTER_SPACE_ID {
            return match ptr.offset {
                0x20 => Some((StackBase::Rsp, 0)),
                0x28 => Some((StackBase::Rbp, 0)),
                _ => None,
            };
        }

        let key = VarnodeKey::from(ptr);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let resolved = match self.defs.get(&key).copied() {
            Some(op) => match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => self.resolve_stack_address_inner(&op.inputs[0], visiting),
                PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset + delta))
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[1], visiting)
                    {
                        const_offset(&op.inputs[0]).map(|delta| (base, offset + delta))
                    } else {
                        None
                    }
                }
                PcodeOpcode::IntSub => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset - delta))
                    } else {
                        None
                    }
                }
                PcodeOpcode::PtrSub => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset + delta))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            None => None,
        };
        visiting.remove(&key);
        resolved
    }

    fn ensure_temp_binding_for_output(&mut self, output: &Varnode) -> NirBinding {
        let key = VarnodeKey::from(output);
        if let Some(name) = self.materialized_vns.get(&key)
            && let Some(binding) = self.temps.get(name)
        {
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        let name = next_temp_name(&ty, &mut self.temp_next_id);
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
        };
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }
}

fn apply_preview_type_hints(func: &mut HirFunction, context: &PreviewTypeContext) {
    let mut pointer_hints: HashMap<String, PreviewCallParamRule> = HashMap::new();
    collect_call_type_hints(&func.body, context, &mut pointer_hints);

    for (var_name, hint) in &pointer_hints {
        if let Some(binding) = find_binding_mut(func, var_name)
            && binding.surface_type_name.is_none()
            && binding_byte_size(&binding.ty) == Some(hint.pointer_size)
        {
            binding.surface_type_name = Some(hint.pointer_alias.clone());
        }
    }

    let mut local_hints: HashMap<String, String> = HashMap::new();
    collect_local_surface_hints(&func.body, &pointer_hints, func, &mut local_hints);
    for (var_name, surface_type_name) in local_hints {
        if let Some(binding) = func.locals.iter_mut().find(|binding| binding.name == var_name)
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name);
        }
    }
}

fn collect_call_type_hints(
    body: &[HirStmt],
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) => {
                collect_call_hints_from_expr(rhs, context, pointer_hints);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                collect_call_type_hints(stmts, context, pointer_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_call_type_hints(&case.body, context, pointer_hints);
                }
                collect_call_type_hints(default, context, pointer_hints);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_call_hints_from_expr(cond, context, pointer_hints);
                collect_call_type_hints(then_body, context, pointer_hints);
                collect_call_type_hints(else_body, context, pointer_hints);
            }
            HirStmt::Return(Some(expr)) => {
                collect_call_hints_from_expr(expr, context, pointer_hints);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_call_hints_from_expr(
    expr: &HirExpr,
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            for rule in &context.call_param_rules {
                if rule.callee_name != *target {
                    continue;
                }
                let Some(var_name) = args
                    .get(rule.arg_index)
                    .and_then(peel_surface_var_name_from_expr)
                else {
                    continue;
                };
                pointer_hints
                    .entry(var_name.to_string())
                    .or_insert_with(|| rule.clone());
            }
            for arg in args {
                collect_call_hints_from_expr(arg, context, pointer_hints);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_call_hints_from_expr(expr, context, pointer_hints);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_call_hints_from_expr(lhs, context, pointer_hints);
            collect_call_hints_from_expr(rhs, context, pointer_hints);
        }
        HirExpr::Index { base, .. } => collect_call_hints_from_expr(base, context, pointer_hints),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    local_hints: &mut HashMap<String, String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref {
                    ptr,
                    ty: NirType::Aggregate { .. } | NirType::Unknown | NirType::Ptr(_),
                } = lhs
                    && let Some(param_name) = peel_surface_var_name_from_expr(ptr)
                    && let Some(local_name) = peel_local_surface_name(rhs)
                    && let Some(rule) = pointer_hints.get(param_name)
                    && let Some(local_binding) =
                        func.locals.iter().find(|binding| binding.name == local_name)
                    && let Some(local_size) = binding_byte_size(&local_binding.ty)
                    && rule.pointee_sizes.contains(&local_size)
                {
                    local_hints
                        .entry(local_name.to_string())
                        .or_insert_with(|| rule.pointee_alias.clone());
                }
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                collect_local_surface_hints(stmts, pointer_hints, func, local_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_local_surface_hints(&case.body, pointer_hints, func, local_hints);
                }
                collect_local_surface_hints(default, pointer_hints, func, local_hints);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_local_surface_hints(then_body, pointer_hints, func, local_hints);
                collect_local_surface_hints(else_body, pointer_hints, func, local_hints);
            }
            HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn peel_surface_var_name_from_expr(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => peel_surface_var_name_from_expr(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => peel_surface_var_name_from_expr(base),
        HirExpr::Index { base, index, .. } if *index == 0 => peel_surface_var_name_from_expr(base),
        _ => None,
    }
}

fn peel_local_surface_name(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name),
        HirExpr::Cast { expr, .. } | HirExpr::AggregateCopy { src: expr, .. } => {
            peel_local_surface_name(expr)
        }
        _ => None,
    }
}

fn find_binding_mut<'a>(func: &'a mut HirFunction, name: &str) -> Option<&'a mut NirBinding> {
    if let Some(param) = func.params.iter_mut().find(|binding| binding.name == name) {
        return Some(param);
    }
    func.locals.iter_mut().find(|binding| binding.name == name)
}

fn binding_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

fn is_comparison(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
    )
}

fn map_binary_op(opcode: PcodeOpcode) -> Result<HirBinaryOp, MlilPreviewError> {
    match opcode {
        PcodeOpcode::IntAdd => Ok(HirBinaryOp::Add),
        PcodeOpcode::IntSub => Ok(HirBinaryOp::Sub),
        PcodeOpcode::IntMult => Ok(HirBinaryOp::Mul),
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => Ok(HirBinaryOp::Div),
        PcodeOpcode::IntRem | PcodeOpcode::IntSRem => Ok(HirBinaryOp::Mod),
        PcodeOpcode::IntAnd => Ok(HirBinaryOp::And),
        PcodeOpcode::BoolAnd => Ok(HirBinaryOp::LogicalAnd),
        PcodeOpcode::IntOr => Ok(HirBinaryOp::Or),
        PcodeOpcode::BoolOr => Ok(HirBinaryOp::LogicalOr),
        PcodeOpcode::IntXor | PcodeOpcode::BoolXor => Ok(HirBinaryOp::Xor),
        PcodeOpcode::IntLeft => Ok(HirBinaryOp::Shl),
        PcodeOpcode::IntRight => Ok(HirBinaryOp::Shr),
        PcodeOpcode::IntSRight => Ok(HirBinaryOp::Sar),
        PcodeOpcode::IntEqual => Ok(HirBinaryOp::Eq),
        PcodeOpcode::IntNotEqual => Ok(HirBinaryOp::Ne),
        PcodeOpcode::IntLess => Ok(HirBinaryOp::Lt),
        PcodeOpcode::IntLessEqual => Ok(HirBinaryOp::Le),
        PcodeOpcode::IntSLess => Ok(HirBinaryOp::SLt),
        PcodeOpcode::IntSLessEqual => Ok(HirBinaryOp::SLe),
        _ => Err(MlilPreviewError::UnsupportedPattern("binary op")),
    }
}

fn type_from_size(size: u32, signed: bool) -> NirType {
    match size {
        1 => NirType::Int { bits: 8, signed },
        2 => NirType::Int { bits: 16, signed },
        4 => NirType::Int { bits: 32, signed },
        8 => NirType::Int { bits: 64, signed },
        16 | 24 | 32 => NirType::Aggregate { size },
        _ => NirType::Unknown,
    }
}

fn is_materializable_output_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::Load
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::IntAdd
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
            | PcodeOpcode::IntNegate
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow
            | PcodeOpcode::PopCount
            | PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Piece
            | PcodeOpcode::SubPiece
            | PcodeOpcode::MultiEqual
            | PcodeOpcode::Indirect
    )
}

fn next_temp_name(ty: &NirType, next_id: &mut u32) -> String {
    let prefix = match ty {
        NirType::Bool => "bVar",
        NirType::Int { bits: 32, signed: true } => "iVar",
        NirType::Int { bits: 32, signed: false } => "uVar",
        _ => "xVar",
    };
    let name = format!("{prefix}{}", *next_id);
    *next_id += 1;
    name
}

fn register_name_with_param(offset: u64, _size: u32) -> Option<(&'static str, Option<usize>)> {
    match offset {
        0x08 => Some(("param_1", Some(0))),
        0x10 => Some(("param_2", Some(1))),
        0x80 => Some(("param_3", Some(2))),
        0x88 => Some(("param_4", Some(3))),
        0x00 => Some(("rax", None)),
        0x18 => Some(("rbx", None)),
        0x20 => Some(("rsp", None)),
        0x28 => Some(("rbp", None)),
        0x30 => Some(("rsi", None)),
        0x38 => Some(("rdi", None)),
        0x90 => Some(("r10", None)),
        0x98 => Some(("r11", None)),
        0xa0 => Some(("r12", None)),
        0xa8 => Some(("r13", None)),
        0xb0 => Some(("r14", None)),
        0xb8 => Some(("r15", None)),
        _ => None,
    }
}

fn register_name(offset: u64, size: u32) -> &'static str {
    register_name_with_param(offset, size)
        .map(|(name, _)| name)
        .unwrap_or("reg")
}

fn expr_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::Var(_) => NirType::Unknown,
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate { size: *size },
    }
}
