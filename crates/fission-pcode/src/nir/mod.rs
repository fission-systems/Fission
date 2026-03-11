use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_loader::loader::LoadedBinary;
use std::collections::{BTreeMap, HashMap, HashSet};
use thiserror::Error;

pub type NirValueId = u32;
pub type StackSlotId = u32;

const UNIQUE_SPACE_ID: u64 = 1;
const REGISTER_SPACE_ID: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirType {
    Unknown,
    Bool,
    Int { bits: u32, signed: bool },
    Ptr(Box<NirType>),
    Aggregate { size: u32 },
    Float { bits: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBinding {
    pub name: String,
    pub ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirFunction {
    pub name: String,
    pub address: u64,
    pub blocks: Vec<NirBlock>,
    pub locals: Vec<NirBinding>,
    pub params: Vec<NirBinding>,
    pub return_type: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBlock {
    pub id: u32,
    pub phis: Vec<String>,
    pub stmts: Vec<HirStmt>,
    pub terminator: NirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirTerminator {
    Fallthrough(Option<u32>),
    Goto(u32),
    Branch {
        cond: HirExpr,
        true_target: u32,
        false_target: Option<u32>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<NirBinding>,
    pub locals: Vec<NirBinding>,
    pub return_type: NirType,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Assign { lhs: HirLValue, rhs: HirExpr },
    Expr(HirExpr),
    Block(Vec<HirStmt>),
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    Label(String),
    Goto(String),
    Return(Option<HirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirLValue {
    Var(String),
    Deref { ptr: Box<HirExpr>, ty: NirType },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExpr {
    Var(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<HirExpr>,
    },
    Unary {
        op: HirUnaryOp,
        expr: Box<HirExpr>,
        ty: NirType,
    },
    Binary {
        op: HirBinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        ty: NirType,
    },
    Call {
        target: String,
        args: Vec<HirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<HirExpr>,
        offset: i64,
    },
    Index {
        base: Box<HirExpr>,
        index: usize,
        elem_ty: NirType,
    },
    AggregateCopy {
        src: Box<HirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LogicalAnd,
    LogicalOr,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    Eq,
    Ne,
    Lt,
    Le,
    SLt,
    SLe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlilPreviewOptions {
    pub pe_x64_only: bool,
    pub is_64bit: bool,
    pub format: String,
    pub image_base: u64,
    pub sections: Vec<(u64, u64)>,
}

impl MlilPreviewOptions {
    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let sections = binary
            .inner()
            .sections
            .iter()
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size as u64,
                )
            })
            .collect();
        Self {
            pe_x64_only: true,
            is_64bit: binary.is_64bit,
            format: binary.format.clone(),
            image_base: binary.inner().image_base,
            sections,
        }
    }

    fn is_pe_x64(&self) -> bool {
        self.is_64bit && self.format.to_ascii_uppercase().starts_with("PE")
    }

    fn is_mapped_global(&self, address: u64) -> bool {
        self.sections
            .iter()
            .any(|(start, end)| address >= *start && address < *end)
    }
}

#[derive(Debug, Error)]
pub enum MlilPreviewError {
    #[error("mlil-preview currently supports PE x64 only")]
    UnsupportedArchitecture,
    #[error("unsupported control flow in mlil-preview")]
    UnsupportedControlFlow,
    #[error("unsupported pcode pattern: {0}")]
    UnsupportedPattern(&'static str),
    #[error("value lowering failed")]
    LoweringFailed,
}

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
    defs: HashMap<VarnodeKey, &'a PcodeOp>,
    address_to_index: HashMap<u64, usize>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    params: BTreeMap<usize, NirBinding>,
    locals: BTreeMap<i64, StackSlot>,
    locals_next_id: StackSlotId,
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
    if options.pe_x64_only && !options.is_pe_x64() {
        return Err(MlilPreviewError::UnsupportedArchitecture);
    }

    let mut builder = PreviewBuilder::new(pcode, options);
    let mut hir = builder.build_hir(name, address)?;
    for stmt in &mut hir.body {
        normalize_stmt(stmt);
    }
    Ok(print_hir_function(&hir))
}

impl<'a> PreviewBuilder<'a> {
    fn new(pcode: &'a PcodeFunction, options: &'a MlilPreviewOptions) -> Self {
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
        let successors = build_successor_index_map(pcode, &address_to_index);
        let predecessors = build_predecessor_index_map(&successors);
        Self {
            pcode,
            options,
            defs,
            address_to_index,
            successors,
            predecessors,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
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
                })
                .collect(),
            return_type,
            body,
        })
    }

    fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if let Some((stmt, skip_to)) = self.try_lower_dowhile(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_while(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_short_circuit_if(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_if_else(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_if(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }

            let block = &self.pcode.blocks[idx];
            if idx == 0 || targeted.contains(&block.start_address) {
                body.push(HirStmt::Label(block_label(block.start_address)));
            }
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                }
            }
            idx += 1;
        }
        Ok(body)
    }

    fn try_lower_short_circuit_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if let Some(lowered) = self.try_lower_short_circuit_and(idx)? {
            return Ok(Some(lowered));
        }
        if let Some(lowered) = self.try_lower_short_circuit_or(idx)? {
            return Ok(Some(lowered));
        }
        Ok(None)
    }

    fn try_lower_short_circuit_and(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let mut conds = Vec::new();
        let mut current_idx = idx;
        let mut join_idx: Option<usize> = None;

        loop {
            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(current_idx)?
            else {
                return Ok(None);
            };
            if false_target != Some(self.pcode.blocks[next_idx].start_address) {
                return Ok(None);
            }
            let current_join_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            if let Some(join_idx) = join_idx {
                if join_idx != current_join_idx {
                    return Ok(None);
                }
            } else {
                join_idx = Some(current_join_idx);
            }
            conds.push(negate_expr(cond));

            let next_is_conditional = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { .. }
            );
            if next_is_conditional {
                current_idx = next_idx;
                continue;
            }

            let Some(join_idx) = join_idx else {
                return Ok(None);
            };
            let Some((then_body, skip_to)) =
                self.lower_linear_body(next_idx, LinearExit::Join(join_idx))?
            else {
                return Ok(None);
            };
            if conds.len() < 2 {
                return Ok(None);
            }
            return Ok(Some((
                HirStmt::If {
                    cond: fold_logical_chain(conds, HirBinaryOp::LogicalAnd),
                    then_body,
                    else_body: Vec::new(),
                },
                skip_to,
            )));
        }
    }

    fn try_lower_short_circuit_or(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let Some(mut next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        if false_target != Some(self.pcode.blocks[next_idx].start_address) {
            return Ok(None);
        }
        let body_idx = self
            .find_block_index_by_address(true_target)
            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
        if body_idx <= idx {
            return Ok(None);
        }

        let mut conds = vec![cond];
        loop {
            let is_conditional_chain = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { true_target, .. }
                    if self.find_block_index_by_address(true_target) == Some(body_idx)
            );
            if !is_conditional_chain {
                let false_entry_idx = next_idx;
                let Some(exit) = self.shared_linear_exit(body_idx, false_entry_idx)? else {
                    return Ok(None);
                };
                let Some((false_body, false_skip)) =
                    self.lower_linear_body(false_entry_idx, exit)?
                else {
                    return Ok(None);
                };
                if !false_body.is_empty() {
                    return Ok(None);
                }
                let Some((then_body, then_skip)) =
                    self.lower_linear_body(body_idx, exit)?
                else {
                    return Ok(None);
                };
                if conds.len() < 2 {
                    return Ok(None);
                }
                let skip_to = match exit {
                    LinearExit::Join(join_idx) => join_idx,
                    LinearExit::Return | LinearExit::End => then_skip.max(false_skip),
                };
                return Ok(Some((
                    HirStmt::If {
                        cond: fold_logical_chain(conds, HirBinaryOp::LogicalOr),
                        then_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            let LoweredTerminator::Cond {
                cond,
                false_target,
                ..
            } = self.lower_block_terminator(next_idx)?
            else {
                return Ok(None);
            };
            conds.push(cond);
            let Some(chain_next_idx) = self.fallthrough_index(next_idx) else {
                return Ok(None);
            };
            if false_target != Some(self.pcode.blocks[chain_next_idx].start_address) {
                return Ok(None);
            }
            next_idx = chain_next_idx;
        }
    }

    fn try_lower_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some(next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let next_addr = self.pcode.blocks[next_idx].start_address;

        let (cond, body_idx, exit) = if true_target == next_addr {
            let exit = if let Some(join_addr) = false_target {
                let join_idx = self
                    .find_block_index_by_address(join_addr)
                    .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                LinearExit::Join(join_idx)
            } else {
                self.linear_exit(next_idx)?
                    .ok_or(MlilPreviewError::UnsupportedControlFlow)?
            };
            (cond, next_idx, exit)
        } else if false_target == Some(next_addr) {
            let join_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (negate_expr(cond), next_idx, LinearExit::Join(join_idx))
        } else {
            return Ok(None);
        };

        let Some((body, skip_to)) = self.lower_linear_body(body_idx, exit)? else {
            return Ok(None);
        };
        Ok(Some((
            HirStmt::If {
                cond,
                then_body: body,
                else_body: Vec::new(),
            },
            skip_to,
        )))
    }

    fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if idx + 2 >= self.pcode.blocks.len() {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target: Some(false_target),
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let Some(next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let next_addr = self.pcode.blocks[next_idx].start_address;

        let (cond, then_idx, else_idx) = if true_target == next_addr {
            let Some(else_idx) = self.find_block_index_by_address(false_target) else {
                return Ok(None);
            };
            (cond, next_idx, else_idx)
        } else if false_target == next_addr {
            let Some(then_idx) = self.find_block_index_by_address(true_target) else {
                return Ok(None);
            };
            (negate_expr(cond), then_idx, next_idx)
        } else {
            return Ok(None);
        };

        let Some(exit) = self.shared_linear_exit(then_idx, else_idx)? else {
            return Ok(None);
        };
        let Some((then_body, then_skip)) = self.lower_linear_body(then_idx, exit)? else {
            return Ok(None);
        };
        let Some((else_body, else_skip)) = self.lower_linear_body(else_idx, exit)? else {
            return Ok(None);
        };
        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
        };
        Ok(Some((
            HirStmt::If {
                cond,
                then_body,
                else_body,
            },
            skip_to,
        )))
    }

    fn try_lower_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some((body, cond, skip_to)) = self.lower_do_while_region(idx)? else {
            return Ok(None);
        };
        Ok(Some((HirStmt::DoWhile { body, cond }, skip_to)))
    }

    fn try_lower_while(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_block = &self.pcode.blocks[idx];
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        if !self.lower_block_stmts(cond_block)?.is_empty() {
            return Ok(None);
        }

        let Some(body_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let body_addr = self.pcode.blocks[body_idx].start_address;

        let (cond, exit_idx) = if false_target == Some(body_addr) {
            let exit_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (negate_expr(cond), exit_idx)
        } else if true_target == body_addr {
            let Some(exit_addr) = false_target else {
                return Ok(None);
            };
            let exit_idx = self
                .find_block_index_by_address(exit_addr)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (cond, exit_idx)
        } else {
            return Ok(None);
        };

        let Some((body, loop_join_idx)) = self.lower_linear_body(body_idx, LinearExit::Join(idx))?
        else {
            return Ok(None);
        };
        if loop_join_idx != idx {
            return Ok(None);
        }
        Ok(Some((HirStmt::While { cond, body }, exit_idx)))
    }

    fn lower_do_while_region(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<(Vec<HirStmt>, HirExpr, usize)>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let start_addr = self.pcode.blocks[start_idx].start_address;
                    if true_target == start_addr {
                        let Some(exit_addr) = false_target else {
                            return Ok(None);
                        };
                        let exit_idx = self
                            .find_block_index_by_address(exit_addr)
                            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                        return Ok(Some((body, cond, exit_idx)));
                    }
                    if false_target == Some(start_addr) {
                        let exit_idx = self
                            .find_block_index_by_address(true_target)
                            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                        return Ok(Some((body, negate_expr(cond), exit_idx)));
                    }
                    return Ok(None);
                }
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(None);
                }
                _ => return Ok(None),
            }
        }
    }

    fn lower_linear_body(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
                    if exit != LinearExit::Return {
                        return Ok(None);
                    }
                    body.push(HirStmt::Return(expr));
                    return Ok(Some((body, idx + 1)));
                }
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if exit == LinearExit::Join(next_idx) {
                        return Ok(Some((body, next_idx)));
                    }
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(None);
                }
                LoweredTerminator::Fallthrough(None) => {
                    if exit != LinearExit::End {
                        return Ok(None);
                    }
                    return Ok(Some((body, self.pcode.blocks.len())));
                }
                _ => return Ok(None),
            }
        }
    }

    fn shared_linear_exit(
        &mut self,
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let lhs = self.linear_exit(lhs_idx)?;
        let rhs = self.linear_exit(rhs_idx)?;
        if lhs.is_some() && lhs == rhs {
            Ok(lhs)
        } else {
            Ok(None)
        }
    }

    fn linear_exit(&mut self, start_idx: usize) -> Result<Option<LinearExit>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(_) => return Ok(Some(LinearExit::Return)),
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(Some(LinearExit::Join(next_idx)));
                }
                LoweredTerminator::Fallthrough(None) => return Ok(Some(LinearExit::End)),
                _ => return Ok(None),
            }
        }
    }

    fn can_inline_linear_successor(
        &self,
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool {
        next_idx > idx
            && self.predecessors[next_idx]
                .iter()
                .all(|pred| *pred == idx || visited.contains(pred))
    }

    fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        self.successors[idx]
            .iter()
            .copied()
            .find(|succ| *succ == idx + 1)
    }

    fn find_block_index_by_address(&self, address: u64) -> Option<usize> {
        self.address_to_index.get(&address).copied()
    }

    fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError> {
        let mut targets = HashSet::new();
        for idx in 0..self.pcode.blocks.len() {
            for succ in &self.successors[idx] {
                targets.insert(self.pcode.blocks[*succ].start_address);
            }
        }
        Ok(targets)
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
                    }
                }
                _ => {}
            }
        }
        Ok(body)
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
            PcodeOpcode::Branch => {
                let Some(target) = op.inputs.first().and_then(branch_target_address) else {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                };
                Ok(LoweredTerminator::Goto(target))
            }
            PcodeOpcode::CBranch => {
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
        self.pcode.blocks.get(idx + 1).map(|block| block.start_address)
    }

    fn lower_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let target = if let Some(target) = op.inputs.first() {
            match self.lower_varnode(target, visiting)? {
                HirExpr::Const(val, _) => format!("sub_{:x}", val as u64),
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

    fn lower_piece_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::LoweringFailed);
        }
        let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
        let output_ty = type_from_size(output.size, false);
        if let Some(expr) = self.try_recombine_piece(op, &output_ty, visiting)? {
            return Ok(expr);
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        let shift_bits = i64::from(op.inputs[1].size) * 8;
        let shifted = HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(HirExpr::Cast {
                ty: output_ty.clone(),
                expr: Box::new(lhs),
            }),
            rhs: Box::new(HirExpr::Const(
                shift_bits,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: output_ty.clone(),
        };
        Ok(HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs: Box::new(shifted),
            rhs: Box::new(HirExpr::Cast {
                ty: output_ty.clone(),
                expr: Box::new(rhs),
            }),
            ty: output_ty,
        })
    }

    fn try_recombine_piece(
        &mut self,
        op: &PcodeOp,
        _output_ty: &NirType,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Ok(None);
        }
        let Some(lhs_origin) = self.extract_subpiece_origin(&op.inputs[0]) else {
            return Ok(None);
        };
        let Some(rhs_origin) = self.extract_subpiece_origin(&op.inputs[1]) else {
            return Ok(None);
        };
        if lhs_origin.base != rhs_origin.base {
            return Ok(None);
        }
        if rhs_origin.byte_offset != 0 {
            return Ok(None);
        }
        if lhs_origin.byte_offset != i64::from(rhs_origin.piece_size) {
            return Ok(None);
        }
        if lhs_origin.base_size != op.output.as_ref().map(|out| out.size).unwrap_or(0) {
            return Ok(None);
        }
        let base_expr = self.lower_varnode(&lhs_origin.base_vn, visiting)?;
        Ok(Some(base_expr))
    }

    fn lower_subpiece_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::LoweringFailed);
        }
        let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
        let output_ty = type_from_size(output.size, false);
        let base = self.lower_varnode(&op.inputs[0], visiting)?;
        let byte_offset = const_offset(&op.inputs[1]).ok_or(MlilPreviewError::LoweringFailed)?;
        let shifted = if byte_offset == 0 {
            base
        } else {
            HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: Box::new(base),
                rhs: Box::new(HirExpr::Const(
                    byte_offset * 8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: type_from_size(op.inputs[0].size, false),
            }
        };
        Ok(HirExpr::Cast {
            ty: output_ty,
            expr: Box::new(shifted),
        })
    }

    fn extract_subpiece_origin(&self, vn: &Varnode) -> Option<SubpieceOrigin> {
        self.extract_subpiece_origin_inner(vn, &mut HashSet::new())
    }

    fn extract_subpiece_origin_inner(
        &self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<SubpieceOrigin> {
        let key = VarnodeKey::from(vn);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let result = match self.defs.get(&key).copied() {
            Some(op)
                if matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                ) && op.inputs.len() == 1
                    && op.inputs[0].size == vn.size =>
            {
                self.extract_subpiece_origin_inner(&op.inputs[0], visiting)
            }
            Some(op) if op.opcode == PcodeOpcode::SubPiece && op.inputs.len() >= 2 => {
                let base_vn = op.inputs[0].clone();
                Some(SubpieceOrigin {
                    base: VarnodeKey::from(&base_vn),
                    base_vn,
                    base_size: op.inputs[0].size,
                    byte_offset: const_offset(&op.inputs[1])?,
                    piece_size: vn.size,
                })
            }
            None => Some(SubpieceOrigin {
                base: VarnodeKey::from(vn),
                base_vn: vn.clone(),
                base_size: vn.size,
                byte_offset: 0,
                piece_size: vn.size,
            }),
            _ => None,
        };
        visiting.remove(&key);
        result
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
}

fn build_successor_index_map(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
) -> Vec<Vec<usize>> {
    pcode.blocks
        .iter()
        .enumerate()
        .map(|(idx, block)| {
            let mut succs = Vec::new();
            match block_terminator_op(block) {
                Some(op) if op.opcode == PcodeOpcode::Return => {}
                Some(op) if op.opcode == PcodeOpcode::Branch => {
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
                    }
                }
                Some(op) if op.opcode == PcodeOpcode::CBranch => {
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
                    }
                    if idx + 1 < pcode.blocks.len() {
                        succs.push(idx + 1);
                    }
                }
                Some(op) if op.opcode == PcodeOpcode::BranchInd => {}
                _ => {
                    if idx + 1 < pcode.blocks.len() {
                        succs.push(idx + 1);
                    }
                }
            }
            succs.sort_unstable();
            succs.dedup();
            succs
        })
        .collect()
}

fn build_predecessor_index_map(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::new(); successors.len()];
    for (idx, succs) in successors.iter().enumerate() {
        for succ in succs {
            predecessors[*succ].push(idx);
        }
    }
    predecessors
}

fn block_terminator_op(block: &crate::pcode::PcodeBasicBlock) -> Option<&PcodeOp> {
    let idx = block.ops.iter().rposition(|op| {
        matches!(
            op.opcode,
            PcodeOpcode::Branch
                | PcodeOpcode::CBranch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Return
        )
    })?;
    block.ops.get(idx)
}

fn const_offset(vn: &Varnode) -> Option<i64> {
    if vn.is_constant {
        Some(vn.constant_val)
    } else {
        None
    }
}

fn branch_target_address(vn: &Varnode) -> Option<u64> {
    if vn.is_constant {
        if vn.offset != 0 {
            Some(vn.offset)
        } else if vn.constant_val >= 0 {
            Some(vn.constant_val as u64)
        } else {
            None
        }
    } else {
        None
    }
}

fn block_label(address: u64) -> String {
    format!("block_{:x}", address)
}

fn fold_logical_chain(mut exprs: Vec<HirExpr>, op: HirBinaryOp) -> HirExpr {
    debug_assert!(matches!(op, HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr));
    let first = exprs.remove(0);
    exprs.into_iter().fold(first, |lhs, rhs| HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    })
}

fn negate_expr(expr: HirExpr) -> HirExpr {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

fn strip_casts(expr: &HirExpr) -> HirExpr {
    match expr {
        HirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other.clone(),
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

fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::Expr(expr) => normalize_expr(expr),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                normalize_stmt(stmt);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            normalize_expr(cond);
            for stmt in then_body {
                normalize_stmt(stmt);
            }
            for stmt in else_body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            normalize_expr(cond);
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                normalize_stmt(stmt);
            }
            normalize_expr(cond);
        }
        HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::Return(Some(expr)) => normalize_expr(expr),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn normalize_expr(expr: &mut HirExpr) {
    match expr {
        HirExpr::Cast { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Unary { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Binary { lhs, rhs, .. } => {
            normalize_expr(lhs);
            normalize_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                normalize_expr(arg);
            }
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => normalize_expr(ptr),
        HirExpr::Index { base, .. } => normalize_expr(base),
        HirExpr::AggregateCopy { src, .. } => normalize_expr(src),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_cast_expr(&current)
            .or_else(|| normalize_signed_power_of_two_mod(&current))
            .or_else(|| normalize_unsigned_power_of_two_mod(&current))
            .or_else(|| normalize_boolean_logic(&current))
            .or_else(|| collapse_zero_offset_cast(&current));
        match next {
            Some(next_expr) if next_expr != current => current = next_expr,
            _ => break,
        }
    }
    *expr = current;
}

fn canonicalize_cast_expr(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };

    if should_preserve_non_scalar_cast(ty) {
        if let HirExpr::Cast {
            ty: inner_ty,
            expr: inner_inner,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                return Some(HirExpr::Cast {
                    ty: ty.clone(),
                    expr: inner_inner.clone(),
                });
            }
        }
        return None;
    }

    let inner_ty = expr_type(inner);
    if inner_ty == *ty {
        return Some((**inner).clone());
    }

    let HirExpr::Cast {
        ty: inner_cast_ty,
        expr: inner_inner,
    } = inner.as_ref()
    else {
        return None;
    };

    if inner_cast_ty == ty {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    if should_drop_inner_scalar_cast(ty, inner_cast_ty, &expr_type(inner_inner)) {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    None
}

fn should_preserve_non_scalar_cast(ty: &NirType) -> bool {
    matches!(ty, NirType::Ptr(_) | NirType::Aggregate { .. } | NirType::Float { .. })
}

fn scalar_cast_signature(ty: &NirType) -> Option<(u32, bool)> {
    match ty {
        NirType::Bool => Some((1, false)),
        NirType::Int { bits, signed } => Some((*bits, *signed)),
        _ => None,
    }
}

fn source_is_scalarish(ty: &NirType) -> bool {
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. })
}

fn should_drop_inner_scalar_cast(
    outer_ty: &NirType,
    inner_ty: &NirType,
    source_ty: &NirType,
) -> bool {
    if should_preserve_non_scalar_cast(outer_ty) || should_preserve_non_scalar_cast(inner_ty) {
        return false;
    }
    let Some((outer_bits, outer_signed)) = scalar_cast_signature(outer_ty) else {
        return false;
    };
    let Some((inner_bits, inner_signed)) = scalar_cast_signature(inner_ty) else {
        return false;
    };
    if !source_is_scalarish(source_ty) {
        return false;
    }

    if outer_bits < inner_bits {
        return true;
    }

    outer_bits == inner_bits && outer_signed == inner_signed
}

fn normalize_boolean_logic(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: inner,
                ..
            } => Some((**inner).clone()),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            _ => None,
        },
        _ => None,
    }
}

fn normalize_unsigned_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(
        mask,
        NirType::Int {
            bits,
            signed: false,
        },
    ) = rhs.as_ref()
    else {
        return None;
    };
    let divisor = (*mask as i128) + 1;
    if divisor <= 1 || (divisor & (divisor - 1)) != 0 {
        return None;
    }
    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor as i64,
            NirType::Int {
                bits: *bits,
                signed: false,
            },
        )),
        ty: NirType::Int {
            bits: *bits,
            signed: false,
        },
    })
}

fn normalize_signed_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Sar,
        lhs: sar_inner,
        rhs: sar_rhs,
        ..
    } = shl_inner.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(sar_shift, _) = sar_rhs.as_ref() else {
        return None;
    };
    if sar_shift != shift_amount {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = sar_inner.as_ref()
    else {
        return None;
    };
    if add_lhs.as_ref() != lhs.as_ref() {
        return None;
    }
    let (sign_source, sign_shift, mask) = match add_rhs.as_ref() {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *divisor - 1)
        }
        _ => return None,
    };
    if sign_source != lhs.as_ref() {
        return None;
    }

    let width = match ty {
        NirType::Int { bits, signed: true } => *bits,
        _ => 64,
    };
    let log2 = *shift_amount;
    let divisor = 1_i64 << log2;
    if sign_shift != i64::from(width.saturating_sub(1)) || mask != divisor - 1 {
        return None;
    }

    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: true,
            },
        )),
        ty: NirType::Int {
            bits: width,
            signed: true,
        },
    })
}

fn collapse_zero_offset_cast(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Load { ptr, ty } => {
            let HirExpr::PtrOffset { base, offset } = ptr.as_ref() else {
                return None;
            };
            if *offset != 0 {
                return None;
            }
            Some(HirExpr::Load {
                ptr: base.clone(),
                ty: ty.clone(),
            })
        }
        HirExpr::PtrOffset { base, offset } if *offset == 0 => Some((**base).clone()),
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } if *index == 0 => Some(HirExpr::Load {
            ptr: base.clone(),
            ty: elem_ty.clone(),
        }),
        _ => None,
    }
}

fn print_hir_function(func: &HirFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} {}(", print_type(&func.return_type), func.name));
    for (idx, param) in func.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{} {}", print_type(&param.ty), param.name));
    }
    out.push_str(")\n{\n");
    for local in &func.locals {
        out.push_str(&format!("    {} {};\n", print_type(&local.ty), local.name));
    }
    if !func.locals.is_empty() {
        out.push('\n');
    }
    for stmt in &func.body {
        print_stmt_with_indent(stmt, 1, &mut out);
    }
    out.push_str("}\n");
    out
}

fn print_stmt(stmt: &HirStmt) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => format!("{} = {};", print_lvalue(lhs), print_expr(rhs)),
        HirStmt::Expr(expr) => format!("{};", print_expr(expr)),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) => format!("goto {};", label),
        HirStmt::Block(_) => "{ ... }".to_string(),
        HirStmt::If { .. } => "if (...) { ... }".to_string(),
        HirStmt::While { .. } => "while (...) { ... }".to_string(),
        HirStmt::DoWhile { .. } => "do { ... } while (...);".to_string(),
        HirStmt::Return(Some(expr)) => format!("return {};", print_expr(expr)),
        HirStmt::Return(None) => "return;".to_string(),
        HirStmt::Break => "break;".to_string(),
        HirStmt::Continue => "continue;".to_string(),
    }
}

fn print_stmt_with_indent(stmt: &HirStmt, indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    match stmt {
        HirStmt::Assign { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Goto(_) => {
            out.push_str(&pad);
            out.push_str(&print_stmt(stmt));
            out.push('\n');
        }
        HirStmt::Label(label) => {
            out.push_str(label);
            out.push_str(":\n");
        }
        HirStmt::Block(stmts) => {
            out.push_str(&pad);
            out.push_str("{\n");
            for stmt in stmts {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            out.push_str(&pad);
            out.push_str(&format!("if ({}) {{\n", print_expr(cond)));
            for stmt in then_body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push('}');
            if else_body.is_empty() {
                out.push('\n');
            } else {
                out.push_str(" else {\n");
                for stmt in else_body {
                    print_stmt_with_indent(stmt, indent + 1, out);
                }
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
        HirStmt::While { cond, body } => {
            out.push_str(&pad);
            out.push_str(&format!("while ({}) {{\n", print_expr(cond)));
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::DoWhile { body, cond } => {
            out.push_str(&pad);
            out.push_str("do {\n");
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str(&format!("}} while ({});\n", print_expr(cond)));
        }
    }
}

fn print_lvalue(lhs: &HirLValue) -> String {
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::Deref { ptr, ty } => format!("*({} *)({})", print_type(ty), print_expr(ptr)),
    }
}

fn print_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Var(name) => name.clone(),
        HirExpr::Const(value, _) => value.to_string(),
        HirExpr::Cast { ty, expr } => format!("({})({})", print_type(ty), print_expr(expr)),
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            format!("{}({})", symbol, print_expr(expr))
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            format!(
                "({} {} {})",
                print_expr(lhs),
                print_binary_op(*op),
                print_expr(rhs)
            )
        }
        HirExpr::Call { target, args, .. } => {
            let args = args.iter().map(print_expr).collect::<Vec<_>>().join(", ");
            format!("{target}({args})")
        }
        HirExpr::Load { ptr, ty } => format!("*({} *)({})", print_type(ty), print_expr(ptr)),
        HirExpr::PtrOffset { base, offset } => {
            if *offset == 0 {
                print_expr(base)
            } else if *offset > 0 {
                format!("((uint8_t *)({}) + {})", print_expr(base), offset)
            } else {
                format!(
                    "((uint8_t *)({}) - {})",
                    print_expr(base),
                    offset.unsigned_abs()
                )
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => format!(
            "(({} *)({}))[{}]",
            print_type(elem_ty),
            print_expr(base),
            index
        ),
        HirExpr::AggregateCopy { src, size } => {
            format!("*(fission_agg{} *)({})", size, print_expr(src))
        }
    }
}

fn print_binary_op(op: HirBinaryOp) -> &'static str {
    match op {
        HirBinaryOp::Add => "+",
        HirBinaryOp::Sub => "-",
        HirBinaryOp::Mul => "*",
        HirBinaryOp::Div => "/",
        HirBinaryOp::Mod => "%",
        HirBinaryOp::LogicalAnd => "&&",
        HirBinaryOp::LogicalOr => "||",
        HirBinaryOp::And => "&",
        HirBinaryOp::Or => "|",
        HirBinaryOp::Xor => "^",
        HirBinaryOp::Shl => "<<",
        HirBinaryOp::Shr | HirBinaryOp::Sar => ">>",
        HirBinaryOp::Eq => "==",
        HirBinaryOp::Ne => "!=",
        HirBinaryOp::Lt | HirBinaryOp::SLt => "<",
        HirBinaryOp::Le | HirBinaryOp::SLe => "<=",
    }
}

fn print_type(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "undefined".to_string(),
        NirType::Bool => "bool".to_string(),
        NirType::Int { bits, signed } => match (*bits, *signed) {
            (8, false) => "uchar".to_string(),
            (8, true) => "char".to_string(),
            (16, false) => "ushort".to_string(),
            (16, true) => "short".to_string(),
            (32, false) => "uint".to_string(),
            (32, true) => "int".to_string(),
            (64, false) => "ulonglong".to_string(),
            (64, true) => "longlong".to_string(),
            _ => format!("int{}", bits),
        },
        NirType::Ptr(inner) => format!("{} *", print_type(inner)),
        NirType::Aggregate { size } => format!("fission_agg{}", size),
        NirType::Float { bits } => match *bits {
            32 => "float".to_string(),
            64 => "double".to_string(),
            _ => format!("float{}", bits),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::{PcodeBasicBlock, PcodeOp};

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn uniq(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn cst(value: i64, size: u32) -> Varnode {
        Varnode::constant(value, size)
    }

    fn preview_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            format: "PE".to_string(),
            image_base: 0x1400_0000,
            sections: vec![(0x1400_1000, 0x1400_2000)],
        }
    }

    #[test]
    fn stack_slot_recovery_names_locals() {
        let ptr = uniq(0x100, 8);
        let load = uniq(0x110, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1000,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x1001,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(7, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Load,
                        address: 0x1002,
                        output: Some(load.clone()),
                        inputs: vec![cst(0, 4), ptr],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x1003,
                        output: None,
                        inputs: vec![cst(0, 8), load],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "stack_fn", 0x1000, &preview_options())
            .expect("preview render");
        assert!(code.contains("local_10"));
        assert!(code.contains("return local_10;"));
    }

    #[test]
    fn preview_prints_direct_srem_as_mod() {
        let result = uniq(0x200, 8);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x2000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSRem,
                        address: 0x2000,
                        output: Some(result.clone()),
                        inputs: vec![reg(0x08, 8), cst(2, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x2001,
                        output: None,
                        inputs: vec![cst(0, 8), result],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "mod_ll", 0x2000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return (param_1 % 2);"));
    }

    #[test]
    fn signed_mod_idiom_recognition_collapses_to_percent() {
        let base = HirExpr::Var("param_1".to_string());
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(base.clone()),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Sar,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(base.clone()),
                        rhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::And,
                            lhs: Box::new(HirExpr::Binary {
                                op: HirBinaryOp::Shr,
                                lhs: Box::new(base.clone()),
                                rhs: Box::new(HirExpr::Const(
                                    63,
                                    NirType::Int {
                                        bits: 64,
                                        signed: false,
                                    },
                                )),
                                ty: NirType::Int {
                                    bits: 64,
                                    signed: false,
                                },
                            }),
                            rhs: Box::new(HirExpr::Const(
                                1,
                                NirType::Int {
                                    bits: 64,
                                    signed: false,
                                },
                            )),
                            ty: NirType::Int {
                                bits: 64,
                                signed: true,
                            },
                        }),
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                    }),
                    rhs: Box::new(HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                }),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
            }),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
        };
        let mut stmt = HirStmt::Return(Some(expr));
        normalize_stmt(&mut stmt);
        let rendered = print_stmt(&stmt);
        assert_eq!(rendered, "return (param_1 % 2);");
    }

    #[test]
    fn cast_canonicalizer_removes_duplicate_same_type_cast() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Var("uVar1".to_string())),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (uint)(uVar1);");
    }

    #[test]
    fn cast_canonicalizer_drops_redundant_widen_before_narrow() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Cast {
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    expr: Box::new(HirExpr::Var("var1".to_string())),
                }),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (longlong)((uint)(var1));");
    }

    #[test]
    fn cast_canonicalizer_preserves_sign_extension_chain() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
                expr: Box::new(HirExpr::Var("iVar1".to_string())),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (longlong)((int)(iVar1));");
    }

    #[test]
    fn multi_block_preview_lowers_simple_if_without_failing() {
        let cond = uniq(0x300, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3000,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3001,
                            output: None,
                            inputs: vec![cst(0x3020, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3010,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3010,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3020,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3020,
                        output: None,
                        inputs: vec![cst(0, 8), cst(1, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "branchy", 0x3000, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!(param_1)) {"));
        assert!(code.contains("return 0;"));
        assert!(code.contains("return 1;"));
    }

    #[test]
    fn multi_block_preview_lowers_canonical_if_else() {
        let cond = uniq(0x350, 1);
        let ptr = uniq(0x360, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3500,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3500,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3501,
                            output: None,
                            inputs: vec![cst(0x3520, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3510,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3510,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3511,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3512,
                            output: None,
                            inputs: vec![cst(0x3530, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3520,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3520,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3521,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(2, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3530,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3530,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "if_else_fn", 0x3500, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!(param_1)) {") || code.contains("if (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("} else {"));
        assert!(code.contains("local_10 = 2;"));
        assert!(!code.contains("goto block_3510;"));
        assert!(!code.contains("goto block_3520;"));
    }

    #[test]
    fn multi_block_preview_lowers_if_else_with_multi_block_then_region() {
        let cond = uniq(0x370, 1);
        let ptr = uniq(0x380, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3600,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3600,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3601,
                            output: None,
                            inputs: vec![cst(0x3630, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3610,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3610,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3611,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3620,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3620,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3621,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(2, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3622,
                            output: None,
                            inputs: vec![cst(0x3640, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3630,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3630,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3631,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(3, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3632,
                            output: None,
                            inputs: vec![cst(0x3640, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 4,
                    start_address: 0x3640,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3640,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "if_else_chain_fn", 0x3600, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!(param_1)) {") || code.contains("if (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("local_10 = 2;"));
        assert!(code.contains("} else {"));
        assert!(code.contains("local_10 = 3;"));
        assert!(!code.contains("goto block_3620;"));
        assert!(!code.contains("goto block_3630;"));
    }

    #[test]
    fn multi_block_preview_folds_short_circuit_and() {
        let cond_a = uniq(0x390, 1);
        let cond_b = uniq(0x391, 1);
        let ptr = uniq(0x392, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3700,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3700,
                            output: Some(cond_a.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3701,
                            output: None,
                            inputs: vec![cst(0x3730, 8), cond_a],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3710,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3710,
                            output: Some(cond_b.clone()),
                            inputs: vec![reg(0x10, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3711,
                            output: None,
                            inputs: vec![cst(0x3730, 8), cond_b],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3720,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3720,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3721,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3722,
                            output: None,
                            inputs: vec![cst(0x3730, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3730,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3730,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "short_and_fn", 0x3700, &preview_options())
            .expect("preview render");
        assert!(code.contains("&&"));
        assert!(code.contains("local_10 = 7;"));
        assert!(!code.contains("goto block_3730;"));
    }

    #[test]
    fn multi_block_preview_folds_short_circuit_or() {
        let cond_a = uniq(0x3a0, 1);
        let cond_b = uniq(0x3a1, 1);
        let ptr = uniq(0x3a2, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3800,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3800,
                            output: Some(cond_a.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3801,
                            output: None,
                            inputs: vec![cst(0x3830, 8), cond_a],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3810,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3810,
                            output: Some(cond_b.clone()),
                            inputs: vec![reg(0x10, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3811,
                            output: None,
                            inputs: vec![cst(0x3830, 8), cond_b],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3820,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3820,
                        output: None,
                        inputs: vec![cst(0x3840, 8)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3830,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3830,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3831,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(9, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3832,
                            output: None,
                            inputs: vec![cst(0x3840, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 4,
                    start_address: 0x3840,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3840,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "short_or_fn", 0x3800, &preview_options())
            .expect("preview render");
        assert!(code.contains("||"));
        assert!(code.contains("local_10 = 9;"));
        assert!(!code.contains("goto block_3830;"));
    }

    #[test]
    fn multiequal_with_identical_inputs_does_not_fail_preview() {
        let phi = uniq(0x500, 8);
        let copy = uniq(0x508, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x5000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::MultiEqual,
                            address: 0x5000,
                            output: Some(phi.clone()),
                            inputs: vec![reg(0x08, 8), reg(0x08, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Copy,
                            address: 0x5001,
                            output: Some(copy.clone()),
                            inputs: vec![phi],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Return,
                            address: 0x5002,
                            output: None,
                            inputs: vec![cst(0, 8), copy],
                            asm_mnemonic: None,
                        },
                    ],
                },
            ],
        };

        let code = render_mlil_preview(&func, "phi_fn", 0x5000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return param_1;"));
    }

    #[test]
    fn piece_and_subpiece_lower_without_preview_failure() {
        let piece = uniq(0x600, 8);
        let sub = uniq(0x608, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Piece,
                        address: 0x6000,
                        output: Some(piece.clone()),
                        inputs: vec![reg(0x08, 4), reg(0x10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6001,
                        output: Some(sub.clone()),
                        inputs: vec![piece, cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x6002,
                        output: None,
                        inputs: vec![cst(0, 8), sub],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "piece_fn", 0x6000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return"));
        assert!(!code.contains("goto"));
    }

    #[test]
    fn piece_recombines_matching_subpieces_back_to_source_value() {
        let whole = reg(0x08, 8);
        let hi = uniq(0x610, 4);
        let lo = uniq(0x614, 4);
        let recombined = uniq(0x618, 8);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6100,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6100,
                        output: Some(hi.clone()),
                        inputs: vec![whole.clone(), cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6101,
                        output: Some(lo.clone()),
                        inputs: vec![whole.clone(), cst(0, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Piece,
                        address: 0x6102,
                        output: Some(recombined.clone()),
                        inputs: vec![hi, lo],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x6103,
                        output: None,
                        inputs: vec![cst(0, 8), recombined],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "piece_recombine_fn", 0x6100, &preview_options())
            .expect("preview render");
        assert!(code.contains("return param_1;"));
    }

    #[test]
    fn subpieces_inline_directly_into_call_arguments() {
        let whole = reg(0x08, 8);
        let hi = uniq(0x620, 4);
        let lo = uniq(0x624, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6200,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6200,
                        output: Some(hi.clone()),
                        inputs: vec![whole.clone(), cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6201,
                        output: Some(lo.clone()),
                        inputs: vec![whole, cst(0, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Call,
                        address: 0x6202,
                        output: None,
                        inputs: vec![cst(0x140001000, 8), lo, hi],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x6203,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "subpiece_call_fn", 0x6200, &preview_options())
            .expect("preview render");
        assert!(code.contains("sub_140001000"));
        assert!(code.contains("(uint)(param_1)"));
        assert!(code.contains("(uint)((param_1 >> 32))"));
        assert!(!code.contains("tmp_"));
    }

    #[test]
    fn do_while_preview_is_lowered_without_ghidra_fallback() {
        let ptr = uniq(0x400, 8);
        let cond = uniq(0x410, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4000,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4001,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4002,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 3,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4003,
                            output: None,
                            inputs: vec![cst(0x4000, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4010,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4010,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "loop_fn", 0x4000, &preview_options())
            .expect("preview render");
        assert!(code.contains("do {"));
        assert!(code.contains("local_10 = 7;"));
        assert!(code.contains("} while (param_1);"));
    }

    #[test]
    fn while_preview_lowers_multi_block_body() {
        let cond = uniq(0x420, 1);
        let ptr1 = uniq(0x421, 8);
        let ptr2 = uniq(0x422, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4100,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4100,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4101,
                            output: None,
                            inputs: vec![cst(0x4140, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4110,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4110,
                            output: Some(ptr1.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4111,
                            output: None,
                            inputs: vec![cst(0, 4), ptr1, cst(1, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x4120,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4120,
                            output: Some(ptr2.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4121,
                            output: None,
                            inputs: vec![cst(0, 4), ptr2, cst(2, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x4122,
                            output: None,
                            inputs: vec![cst(0x4100, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x4140,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4140,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "while_fn", 0x4100, &preview_options())
            .expect("preview render");
        assert!(code.contains("while (!(param_1)) {") || code.contains("while (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("local_14 = 2;"));
        assert!(!code.contains("goto block_4100;"));
    }

    #[test]
    fn do_while_preview_lowers_multi_block_body() {
        let cond = uniq(0x430, 1);
        let ptr1 = uniq(0x431, 8);
        let ptr2 = uniq(0x432, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4200,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4200,
                            output: Some(ptr1.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4201,
                            output: None,
                            inputs: vec![cst(0, 4), ptr1, cst(5, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4210,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4210,
                            output: Some(ptr2.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4211,
                            output: None,
                            inputs: vec![cst(0, 4), ptr2, cst(6, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x4220,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4220,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4221,
                            output: None,
                            inputs: vec![cst(0x4200, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x4230,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4230,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "do_while_chain_fn", 0x4200, &preview_options())
            .expect("preview render");
        assert!(code.contains("do {"));
        assert!(code.contains("local_10 = 5;"));
        assert!(code.contains("local_14 = 6;"));
        assert!(code.contains("} while (param_1);"));
    }
}
