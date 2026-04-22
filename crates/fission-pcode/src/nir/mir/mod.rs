#![allow(dead_code)]

use super::{HirExpr, HirFunction, HirLValue, HirStmt, HirSwitchCase, NirBuildStats, StorageClass};

pub(crate) type MirValueId = u32;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MirFunction {
    pub name: String,
    pub blocks: Vec<MirBlock>,
    pub values: Vec<MirValueKind>,
    pub memory_regions: Vec<MirMemoryRegion>,
    pub join_proofs: Vec<MirJoinProof>,
    pub region_proofs: Vec<MirRegionProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirBlock {
    pub id: u32,
    pub stmts: Vec<MirStmt>,
    pub terminator: MirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MirStmt {
    Assign {
        value: MirValueId,
    },
    Store {
        region: Option<u32>,
    },
    Call {
        target: String,
        output: Option<MirValueId>,
    },
    Effect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MirTerminator {
    Return(Option<MirValueId>),
    Goto(String),
    Branch {
        cond: MirValueId,
        then_label: String,
        else_label: Option<String>,
    },
    Switch {
        selector: MirValueId,
        case_count: usize,
    },
    Fallthrough,
    Unsupported,
}

impl Default for MirTerminator {
    fn default() -> Self {
        Self::Fallthrough
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MirValueKind {
    Binding(String),
    Const,
    Expr,
    CallResult,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirMemoryRegion {
    pub id: u32,
    pub storage_class: StorageClass,
    pub escaped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MirProofStatus {
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MirRegionKind {
    Sequence,
    If,
    Loop,
    Switch,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirJoinProof {
    pub label: String,
    pub status: MirProofStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirRegionProof {
    pub kind: MirRegionKind,
    pub status: MirProofStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MirLoweringStats {
    pub function_count: usize,
    pub block_count: usize,
    pub value_count: usize,
    pub memory_region_count: usize,
    pub join_proof_count: usize,
    pub region_proof_count: usize,
}

impl MirLoweringStats {
    pub(crate) fn apply_to_build_stats(
        self,
        build_stats: &mut NirBuildStats,
        projection_duration_ms: usize,
    ) {
        build_stats.mir_enabled_count += 1;
        build_stats.mir_function_count += self.function_count;
        build_stats.mir_block_count += self.block_count;
        build_stats.mir_value_count += self.value_count;
        build_stats.mir_memory_region_count += self.memory_region_count;
        build_stats.mir_join_proof_count += self.join_proof_count;
        build_stats.mir_region_proof_count += self.region_proof_count;
        build_stats.mir_projection_duration_ms += projection_duration_ms;
    }
}

pub(crate) fn project_hir_to_mir(func: &HirFunction) -> (MirFunction, MirLoweringStats) {
    let mut projector = MirProjector::default();
    let mut stmts = Vec::new();
    let mut terminator = MirTerminator::Fallthrough;
    projector.project_stmt_list(&func.body, &mut stmts, &mut terminator);
    let blocks = vec![MirBlock {
        id: 0,
        stmts,
        terminator,
    }];
    let function = MirFunction {
        name: func.name.clone(),
        blocks,
        values: projector.values,
        memory_regions: projector.memory_regions,
        join_proofs: projector.join_proofs,
        region_proofs: projector.region_proofs,
    };
    let stats = MirLoweringStats {
        function_count: 1,
        block_count: function.blocks.len(),
        value_count: function.values.len(),
        memory_region_count: function.memory_regions.len(),
        join_proof_count: function.join_proofs.len(),
        region_proof_count: function.region_proofs.len(),
    };
    (function, stats)
}

#[derive(Default)]
struct MirProjector {
    values: Vec<MirValueKind>,
    memory_regions: Vec<MirMemoryRegion>,
    join_proofs: Vec<MirJoinProof>,
    region_proofs: Vec<MirRegionProof>,
}

impl MirProjector {
    fn project_stmt_list(
        &mut self,
        body: &[HirStmt],
        out: &mut Vec<MirStmt>,
        terminator: &mut MirTerminator,
    ) {
        for stmt in body {
            self.project_stmt(stmt, out, terminator);
        }
    }

    fn project_stmt(
        &mut self,
        stmt: &HirStmt,
        out: &mut Vec<MirStmt>,
        terminator: &mut MirTerminator,
    ) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                self.project_lvalue(lhs);
                let value = self.project_expr(rhs);
                out.push(MirStmt::Assign { value });
            }
            HirStmt::Expr(expr) => match expr {
                HirExpr::Call { target, args, .. } => {
                    for arg in args {
                        self.project_expr(arg);
                    }
                    let output = Some(self.push_value(MirValueKind::CallResult));
                    out.push(MirStmt::Call {
                        target: target.clone(),
                        output,
                    });
                }
                _ => {
                    self.project_expr(expr);
                    out.push(MirStmt::Effect);
                }
            },
            HirStmt::VaStart {
                va_list,
                last_named_param,
            } => {
                self.project_expr(va_list);
                self.push_value(MirValueKind::Binding(last_named_param.clone()));
                out.push(MirStmt::Effect);
            }
            HirStmt::Block(stmts) => {
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::Sequence,
                    status: MirProofStatus::Incomplete,
                });
                self.project_stmt_list(stmts, out, terminator);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let selector = self.project_expr(expr);
                self.project_switch_cases(cases, out);
                self.project_stmt_list(default, out, terminator);
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::Switch,
                    status: MirProofStatus::Incomplete,
                });
                *terminator = MirTerminator::Switch {
                    selector,
                    case_count: cases.len(),
                };
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond = self.project_expr(cond);
                self.project_stmt_list(then_body, out, terminator);
                self.project_stmt_list(else_body, out, terminator);
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::If,
                    status: MirProofStatus::Incomplete,
                });
                *terminator = MirTerminator::Branch {
                    cond,
                    then_label: "then".to_string(),
                    else_label: (!else_body.is_empty()).then(|| "else".to_string()),
                };
            }
            HirStmt::While { cond, body } => {
                self.project_expr(cond);
                self.project_stmt_list(body, out, terminator);
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::Loop,
                    status: MirProofStatus::Incomplete,
                });
            }
            HirStmt::DoWhile { body, cond } => {
                self.project_stmt_list(body, out, terminator);
                self.project_expr(cond);
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::Loop,
                    status: MirProofStatus::Incomplete,
                });
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.project_stmt(init, out, terminator);
                }
                if let Some(cond) = cond {
                    self.project_expr(cond);
                }
                self.project_stmt_list(body, out, terminator);
                if let Some(update) = update {
                    self.project_stmt(update, out, terminator);
                }
                self.region_proofs.push(MirRegionProof {
                    kind: MirRegionKind::Loop,
                    status: MirProofStatus::Incomplete,
                });
            }
            HirStmt::Label(label) => {
                self.join_proofs.push(MirJoinProof {
                    label: label.clone(),
                    status: MirProofStatus::Incomplete,
                });
            }
            HirStmt::Goto(label) => {
                self.join_proofs.push(MirJoinProof {
                    label: label.clone(),
                    status: MirProofStatus::Incomplete,
                });
                *terminator = MirTerminator::Goto(label.clone());
            }
            HirStmt::Return(expr) => {
                let value = expr.as_ref().map(|expr| self.project_expr(expr));
                *terminator = MirTerminator::Return(value);
            }
            HirStmt::Break | HirStmt::Continue => {
                *terminator = MirTerminator::Unsupported;
            }
        }
    }

    fn project_switch_cases(&mut self, cases: &[HirSwitchCase], out: &mut Vec<MirStmt>) {
        let mut terminator = MirTerminator::Fallthrough;
        for case in cases {
            self.project_stmt_list(&case.body, out, &mut terminator);
        }
    }

    fn project_lvalue(&mut self, lvalue: &HirLValue) {
        match lvalue {
            HirLValue::Var(name) => {
                self.push_value(MirValueKind::Binding(name.clone()));
            }
            HirLValue::Deref { ptr, .. } => {
                self.project_expr(ptr);
                self.push_memory_region(StorageClass::Unknown, true);
            }
            HirLValue::Index { base, index, .. } => {
                self.project_expr(base);
                self.project_expr(index);
                self.push_memory_region(StorageClass::Aggregate, true);
            }
        }
    }

    fn project_expr(&mut self, expr: &HirExpr) -> MirValueId {
        match expr {
            HirExpr::Var(name) => self.push_value(MirValueKind::Binding(name.clone())),
            HirExpr::Const(..) => self.push_value(MirValueKind::Const),
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                self.project_expr(expr);
                self.push_value(MirValueKind::Expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                self.project_expr(lhs);
                self.project_expr(rhs);
                self.push_value(MirValueKind::Expr)
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    self.project_expr(arg);
                }
                self.push_value(MirValueKind::CallResult)
            }
            HirExpr::Load { ptr, .. } => {
                self.project_expr(ptr);
                self.push_memory_region(StorageClass::Unknown, false);
                self.push_value(MirValueKind::Expr)
            }
            HirExpr::PtrOffset { base, .. } => {
                self.project_expr(base);
                self.push_value(MirValueKind::Expr)
            }
            HirExpr::Index { base, index, .. } => {
                self.project_expr(base);
                self.project_expr(index);
                self.push_memory_region(StorageClass::Aggregate, false);
                self.push_value(MirValueKind::Expr)
            }
            HirExpr::AggregateCopy { src, .. } => {
                self.project_expr(src);
                self.push_memory_region(StorageClass::Aggregate, false);
                self.push_value(MirValueKind::Expr)
            }
        }
    }

    fn push_value(&mut self, kind: MirValueKind) -> MirValueId {
        let id = self.values.len() as MirValueId;
        self.values.push(kind);
        id
    }

    fn push_memory_region(&mut self, storage_class: StorageClass, escaped: bool) -> u32 {
        let id = self.memory_regions.len() as u32;
        self.memory_regions.push(MirMemoryRegion {
            id,
            storage_class,
            escaped,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::{HirUnaryOp, NirType};

    #[test]
    fn mir_shadow_projection_counts_simple_return() {
        let func = HirFunction {
            name: "mir_test".to_string(),
            body: vec![HirStmt::Return(Some(HirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )))],
            ..HirFunction::default()
        };

        let (mir, stats) = project_hir_to_mir(&func);

        assert_eq!(mir.name, "mir_test");
        assert_eq!(stats.function_count, 1);
        assert_eq!(stats.block_count, 1);
        assert_eq!(stats.value_count, 1);
        assert_eq!(mir.blocks[0].terminator, MirTerminator::Return(Some(0)));
    }

    #[test]
    fn mir_shadow_projection_records_labels_as_incomplete_join_proofs() {
        let func = HirFunction {
            name: "mir_join".to_string(),
            body: vec![
                HirStmt::Label("join_0".to_string()),
                HirStmt::Goto("join_0".to_string()),
            ],
            ..HirFunction::default()
        };

        let (mir, stats) = project_hir_to_mir(&func);

        assert_eq!(stats.join_proof_count, 2);
        assert_eq!(mir.join_proofs[0].label, "join_0");
        assert_eq!(mir.join_proofs[0].status, MirProofStatus::Incomplete);
        assert_eq!(
            mir.blocks[0].terminator,
            MirTerminator::Goto("join_0".to_string())
        );
    }

    #[test]
    fn mir_shadow_projection_does_not_mutate_hir() {
        let func = HirFunction {
            name: "mir_immutable".to_string(),
            body: vec![HirStmt::Expr(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("flag".to_string())),
                ty: NirType::Bool,
            })],
            ..HirFunction::default()
        };
        let before = func.clone();

        let _ = project_hir_to_mir(&func);

        assert_eq!(func, before);
    }
}
