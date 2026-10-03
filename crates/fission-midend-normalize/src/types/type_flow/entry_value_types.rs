//! Entry-value pointer constraints through exact COPYs in bounded regions.
//!
//! Local declarations are deliberately not seeds: one PreHIR name can carry
//! an address and later an unrelated integer. A use constrains the unchanged
//! entry origin, not every value ever assigned to its current carrier.

use super::*;

#[derive(Clone, Debug)]
struct Value {
    ty: NirType,
    input: Option<String>,
}

struct EntryUses {
    seeds: HashMap<String, Value>,
    evidence: HashMap<String, NirType>,
    rejected: HashSet<String>,
    escaped: HashSet<String>,
    pointer_bits: u32,
}

pub(super) fn pointer_input_uses(
    func: &PreHirFunction,
    definitions: &HashMap<String, usize>,
    metatypes: &HashSet<String>,
) -> HashMap<String, NirType> {
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    let escaped = super::super::super::analysis::defuse::collect_address_taken_locals(&func.body);
    let seeds = func
        .params
        .iter()
        .filter(|binding| {
            !definitions.contains_key(&binding.name)
                && !escaped.contains(&binding.name)
                && binding.initializer.is_none()
        })
        .map(|binding| {
            let eligible = binding.surface_type_name.is_none()
                && !metatypes.contains(&binding.name)
                && matches!(binding.ty, NirType::Unknown | NirType::Int { .. })
                && type_bits(&binding.ty, pointer_bits).is_none_or(|bits| bits == pointer_bits);
            (
                binding.name.clone(),
                Value {
                    ty: binding.ty.clone(),
                    input: eligible.then(|| binding.name.clone()),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut uses = EntryUses {
        seeds,
        evidence: HashMap::default(),
        rejected: HashSet::default(),
        escaped,
        pointer_bits,
    };
    uses.statements(&func.body, &mut uses.seeds.clone());
    uses.evidence
        .retain(|input, _| !uses.rejected.contains(input));
    uses.evidence
}

impl EntryUses {
    fn evidence(&mut self, value: &Value, ty: NirType) {
        let Some(input) = &value.input else {
            return;
        };
        if let Some(previous) = self.evidence.get(input) {
            if previous != &ty {
                self.rejected.insert(input.clone());
            }
        } else {
            self.evidence.insert(input.clone(), ty);
        }
    }

    fn scalar(&mut self, value: &Value) {
        if let Some(input) = &value.input {
            self.rejected.insert(input.clone());
        }
    }

    fn expression(&mut self, expr: &PreHirExpr, values: &HashMap<String, Value>) -> Value {
        let mut result = Value {
            ty: expr_type(expr),
            input: None,
        };
        match expr {
            PreHirExpr::Var(name) => {
                if let Some(value) = values.get(name) {
                    result = value.clone();
                }
            }
            PreHirExpr::Cast { ty, expr } => {
                let inner = self.expression(expr, values);
                if matches!(ty, NirType::Ptr(_))
                    && compatible_width(&inner.ty, ty, self.pointer_bits)
                {
                    // A conversion alone (including a dead converted COPY)
                    // is not an address use. Preserve the exact origin until
                    // a memory operation or typed comparison constrains it.
                    result.input = inner.input;
                } else {
                    self.scalar(&inner);
                }
            }
            PreHirExpr::Unary { op, expr, .. } => {
                let inner = self.expression(expr, values);
                if *op != PreHirUnaryOp::Not {
                    self.scalar(&inner);
                }
            }
            PreHirExpr::Binary { op, lhs, rhs, .. } => {
                let lhs_value = self.expression(lhs, values);
                let rhs_value = self.expression(rhs, values);
                match op {
                    PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::Gt
                    | PreHirBinaryOp::Ge => {
                        if matches!(lhs_value.ty, NirType::Ptr(_)) {
                            self.evidence(&rhs_value, lhs_value.ty.clone());
                        }
                        if matches!(rhs_value.ty, NirType::Ptr(_)) {
                            self.evidence(&lhs_value, rhs_value.ty.clone());
                        }
                    }
                    // Truth tests and address differences do not independently
                    // establish a pointee, but are not scalar counterevidence.
                    // Do not transport origins through arithmetic here.
                    PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Add
                    | PreHirBinaryOp::Sub
                    | PreHirBinaryOp::LogicalAnd
                    | PreHirBinaryOp::LogicalOr => {}
                    _ => {
                        self.scalar(&lhs_value);
                        self.scalar(&rhs_value);
                    }
                }
            }
            PreHirExpr::Load { ptr, ty } => {
                let pointer = self.expression(ptr, values);
                self.evidence(&pointer, NirType::Ptr(Box::new(ty.clone())));
                // The loaded value is a new value, not the address origin.
            }
            PreHirExpr::Index {
                base,
                index,
                elem_ty,
            } => {
                let pointer = self.expression(base, values);
                self.evidence(&pointer, NirType::Ptr(Box::new(elem_ty.clone())));
                let index = self.expression(index, values);
                self.scalar(&index);
            }
            PreHirExpr::FieldAccess { base, .. } => {
                self.expression(base, values);
            }
            PreHirExpr::AggregateCopy { src, .. } => {
                self.expression(src, values);
            }
            PreHirExpr::PtrOffset { base, .. } => {
                // Exact COPY provenance intentionally stops at arithmetic.
                self.expression(base, values);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.expression(cond, values);
                self.expression(then_expr, values);
                self.expression(else_expr, values);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    self.expression(arg, values);
                }
            }
            PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::Const(_, _) => {}
        }
        result
    }

    fn statements(&mut self, body: &[PreHirStmt], values: &mut HashMap<String, Value>) {
        for stmt in body {
            match stmt {
                PreHirStmt::Assign { lhs, rhs } => {
                    let value = self.expression(rhs, values);
                    match lhs {
                        PreHirLValue::Var(name) => {
                            if self.escaped.contains(name) {
                                values.remove(name);
                            } else {
                                // Every definition replaces this value's exact
                                // COPY origin, including a load into itself.
                                values.insert(name.clone(), value);
                            }
                        }
                        PreHirLValue::Deref { ptr, ty } => {
                            let pointer = self.expression(ptr, values);
                            self.evidence(&pointer, NirType::Ptr(Box::new(ty.clone())));
                        }
                        PreHirLValue::Index {
                            base,
                            index,
                            elem_ty,
                        } => {
                            let pointer = self.expression(base, values);
                            self.evidence(&pointer, NirType::Ptr(Box::new(elem_ty.clone())));
                            let index = self.expression(index, values);
                            self.scalar(&index);
                        }
                        PreHirLValue::FieldAccess { base, .. } => {
                            self.expression(base, values);
                        }
                    }
                }
                PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                    self.expression(expr, values);
                    if matches!(stmt, PreHirStmt::Return(_)) {
                        *values = self.seeds.clone();
                    }
                }
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    self.expression(cond, values);
                    self.statements(then_body, &mut values.clone());
                    self.statements(else_body, &mut values.clone());
                    *values = self.seeds.clone();
                }
                PreHirStmt::Block(body) => {
                    self.statements(body, &mut values.clone());
                    *values = self.seeds.clone();
                }
                PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
                    // A loop may begin with a value from a preceding iteration.
                    let mut loop_values = self.seeds.clone();
                    self.expression(cond, &loop_values);
                    self.statements(body, &mut loop_values);
                    *values = self.seeds.clone();
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    if let Some(init) = init {
                        self.statements(std::slice::from_ref(init.as_ref()), &mut values.clone());
                    }
                    if let Some(cond) = cond {
                        self.expression(cond, &self.seeds.clone());
                    }
                    self.statements(body, &mut self.seeds.clone());
                    if let Some(update) = update {
                        self.statements(
                            std::slice::from_ref(update.as_ref()),
                            &mut self.seeds.clone(),
                        );
                    }
                    *values = self.seeds.clone();
                }
                PreHirStmt::Switch {
                    expr,
                    cases,
                    default,
                } => {
                    self.expression(expr, values);
                    for case in cases {
                        self.statements(&case.body, &mut self.seeds.clone());
                    }
                    self.statements(default, &mut self.seeds.clone());
                    *values = self.seeds.clone();
                }
                PreHirStmt::VaStart { va_list, .. } => {
                    self.expression(va_list, values);
                }
                PreHirStmt::Label(_)
                | PreHirStmt::Goto(_)
                | PreHirStmt::Break
                | PreHirStmt::Continue
                | PreHirStmt::Return(None) => *values = self.seeds.clone(),
            }
        }
    }
}
