//! Operation-edge type propagation for PreHIR bindings.
//!
//! This is the owner-local counterpart to Ghidra's `TypeOp::propagateType` /
//! `ActionInferTypes::propagateTypeEdge` design.  PreHIR is not full SSA, so the
//! graph is deliberately limited to operations whose C type relation is
//! invariant across lowering: COPY, LOAD, STORE, INDEX, and explicit CAST.

use crate::prelude::*;
use crate::{HashMap, HashSet};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EvidenceStrength {
    StorageWidth,
    Binding,
    Semantic,
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeFact {
    ty: NirType,
    strength: EvidenceStrength,
    locked: bool,
}

impl TypeFact {
    fn unknown() -> Self {
        Self {
            ty: NirType::Unknown,
            strength: EvidenceStrength::StorageWidth,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeFlowEdge {
    Copy {
        lhs: String,
        rhs: String,
    },
    Load {
        pointer: String,
        value: String,
        access_ty: NirType,
    },
    Store {
        pointer: String,
        value: String,
        access_ty: NirType,
    },
    PointerAccess {
        pointer: String,
        access_ty: NirType,
    },
    Cast {
        output: String,
        ty: NirType,
    },
}

impl TypeFlowEdge {
    fn endpoints(&self) -> Vec<&str> {
        match self {
            Self::Copy { lhs, rhs } => vec![lhs, rhs],
            Self::Load { pointer, value, .. } | Self::Store { pointer, value, .. } => {
                vec![pointer, value]
            }
            Self::PointerAccess { pointer, .. } => vec![pointer],
            Self::Cast { output, .. } => vec![output],
        }
    }
}

struct TypeFlowSolver {
    pointer_bits: u32,
    facts: HashMap<String, TypeFact>,
    edges: Vec<TypeFlowEdge>,
    incident: HashMap<String, Vec<usize>>,
    queue: VecDeque<String>,
    queued: HashSet<String>,
    address_nodes: HashSet<String>,
    definition_counts: HashMap<String, usize>,
}

impl TypeFlowSolver {
    fn from_function(func: &PreHirFunction) -> Self {
        let mut solver = Self {
            pointer_bits: if func.is_64bit { 64 } else { 32 },
            facts: HashMap::default(),
            edges: Vec::new(),
            incident: HashMap::default(),
            queue: VecDeque::new(),
            queued: HashSet::default(),
            address_nodes: HashSet::default(),
            definition_counts: HashMap::default(),
        };
        for binding in func.params.iter().chain(func.locals.iter()) {
            let locked = binding.surface_type_name.is_some();
            let strength = if locked {
                EvidenceStrength::Locked
            } else {
                EvidenceStrength::Binding
            };
            solver.facts.insert(
                binding.name.clone(),
                TypeFact {
                    ty: binding.ty.clone(),
                    strength,
                    locked,
                },
            );
        }
        collect_edges(&func.body, &mut solver.edges);
        collect_definition_counts(&func.body, &mut solver.definition_counts);
        for (edge_index, edge) in solver.edges.iter().enumerate() {
            match edge {
                TypeFlowEdge::Load { pointer, .. }
                | TypeFlowEdge::Store { pointer, .. }
                | TypeFlowEdge::PointerAccess { pointer, .. } => {
                    solver.address_nodes.insert(pointer.clone());
                }
                _ => {}
            }
            for endpoint in edge.endpoints() {
                solver
                    .incident
                    .entry(endpoint.to_owned())
                    .or_default()
                    .push(edge_index);
            }
        }
        let initial_nodes = solver.facts.keys().cloned().collect::<Vec<_>>();
        for node in initial_nodes {
            solver.enqueue(node);
        }
        solver
    }

    fn enqueue(&mut self, name: String) {
        if self.queued.insert(name.clone()) {
            self.queue.push_back(name);
        }
    }

    fn fact(&self, name: &str) -> TypeFact {
        self.facts
            .get(name)
            .cloned()
            .unwrap_or_else(TypeFact::unknown)
    }

    fn refine(&mut self, name: &str, candidate: TypeFact) -> bool {
        let current = self.fact(name);
        if self.address_nodes.contains(name)
            && matches!(current.ty, NirType::Ptr(_))
            && !matches!(candidate.ty, NirType::Ptr(_))
        {
            return false;
        }
        let Some(refined) = refine_fact(&current, &candidate, self.pointer_bits) else {
            return false;
        };
        self.facts.insert(name.to_owned(), refined);
        self.enqueue(name.to_owned());
        true
    }

    fn seed_storage_relation(&mut self, pointer: &str, value: &str, access_ty: &NirType) {
        let storage_value = TypeFact {
            ty: access_ty.clone(),
            strength: EvidenceStrength::StorageWidth,
            locked: false,
        };
        let storage_pointer = TypeFact {
            ty: NirType::Ptr(Box::new(access_ty.clone())),
            strength: EvidenceStrength::StorageWidth,
            locked: false,
        };
        self.refine(value, storage_value);
        self.refine(pointer, storage_pointer);
    }

    fn propagate_edge(&mut self, edge: &TypeFlowEdge) {
        match edge {
            TypeFlowEdge::Copy { lhs, rhs } => {
                let lhs_fact = self.fact(lhs);
                let rhs_fact = self.fact(rhs);
                // PreHIR bindings are not SSA varnodes. A name with multiple
                // definitions cannot model Ghidra's per-varnode COPY equality:
                // propagating a later scalar result backward through an earlier
                // pointer copy would corrupt the source parameter. Single-def
                // names retain the bidirectional equality used for spill and
                // alias chains.
                if self.definition_counts.get(lhs).copied().unwrap_or(0) <= 1 {
                    self.refine(
                        lhs,
                        TypeFact {
                            locked: false,
                            ..rhs_fact.clone()
                        },
                    );
                    self.refine(
                        rhs,
                        TypeFact {
                            locked: false,
                            ..lhs_fact
                        },
                    );
                }
            }
            TypeFlowEdge::Load {
                pointer,
                value,
                access_ty,
            }
            | TypeFlowEdge::Store {
                pointer,
                value,
                access_ty,
            } => {
                self.seed_storage_relation(pointer, value, access_ty);
                let pointer_fact = self.fact(pointer);
                let value_fact = self.fact(value);
                if let NirType::Ptr(pointee) = pointer_fact.ty {
                    self.refine(
                        value,
                        TypeFact {
                            ty: pointee.as_ref().clone(),
                            // A pointer declaration constrains the access
                            // width, but a conflicting semantic value needs a
                            // cast rather than having its own type overwritten.
                            // Lock strength protects the pointer node only and
                            // must not leak across the operation edge.
                            strength: pointer_fact.strength.min(EvidenceStrength::Binding),
                            locked: false,
                        },
                    );
                }
                // `pointer == value` is the in-place pointer-chase idiom
                // (`x = *x`, reusing one temp for both the address being
                // dereferenced and the loaded result). `value_fact` above is
                // then a snapshot of `pointer`'s own pre-load fact, not an
                // independent signal -- wrapping it back onto `pointer` as
                // `Ptr(value_fact.ty)` re-derives "x is a pointer to its own
                // prior type", which never converges: each solver pass adds
                // one more `Ptr(...)` layer around the same node forever.
                if pointer != value
                    && value_fact.ty != NirType::Unknown
                    && type_bits(&value_fact.ty, self.pointer_bits)
                        == type_bits(access_ty, self.pointer_bits)
                {
                    self.refine(
                        pointer,
                        TypeFact {
                            ty: NirType::Ptr(Box::new(value_fact.ty)),
                            strength: value_fact.strength.max(EvidenceStrength::Semantic),
                            locked: false,
                        },
                    );
                }
            }
            TypeFlowEdge::PointerAccess { pointer, access_ty } => {
                self.refine(
                    pointer,
                    TypeFact {
                        ty: NirType::Ptr(Box::new(access_ty.clone())),
                        strength: EvidenceStrength::StorageWidth,
                        locked: false,
                    },
                );
            }
            TypeFlowEdge::Cast { output, ty } => {
                self.refine(
                    output,
                    TypeFact {
                        ty: ty.clone(),
                        strength: EvidenceStrength::Semantic,
                        locked: false,
                    },
                );
            }
        }
    }

    fn solve(mut self) -> HashMap<String, TypeFact> {
        while let Some(node) = self.queue.pop_front() {
            self.queued.remove(&node);
            let incident = self.incident.get(&node).cloned().unwrap_or_default();
            for edge_index in incident {
                let edge = self.edges[edge_index].clone();
                self.propagate_edge(&edge);
            }
        }
        self.facts
    }
}

fn type_bits(ty: &NirType, pointer_bits: u32) -> Option<u32> {
    match ty {
        NirType::Bool => Some(8),
        NirType::Int { bits, .. } | NirType::Float { bits } => Some(*bits),
        NirType::Ptr(_) => Some(pointer_bits),
        NirType::Aggregate { size, .. } => Some(size.saturating_mul(8)),
        NirType::Unknown => None,
    }
}

fn type_specificity(ty: &NirType) -> u8 {
    match ty {
        NirType::Unknown => 0,
        NirType::Int { signed: false, .. } => 1,
        NirType::Int { signed: true, .. } => 2,
        NirType::Float { .. } | NirType::Bool => 3,
        NirType::Ptr(pointee) => 4 + type_specificity(pointee),
        NirType::Aggregate { .. } => 8,
    }
}

fn compatible_width(current: &NirType, candidate: &NirType, pointer_bits: u32) -> bool {
    match (
        type_bits(current, pointer_bits),
        type_bits(candidate, pointer_bits),
    ) {
        (Some(current), Some(candidate)) => current == candidate,
        _ => true,
    }
}

/// Real C pointer types are never nested this deep; anything beyond this is
/// the solver chasing a cycle in the type-flow graph (a Load/Store edge
/// pair, or a longer multi-variable chain, feeding a node's own fact back
/// into itself and re-wrapping it each solver pass). Rejecting the update
/// once the cap is hit stops the cycle from growing further without having
/// to special-case every graph shape that can produce one.
const MAX_PTR_NESTING_DEPTH: u32 = 8;

fn ptr_nesting_depth(ty: &NirType) -> u32 {
    match ty {
        NirType::Ptr(inner) => 1 + ptr_nesting_depth(inner),
        _ => 0,
    }
}

fn refine_fact(current: &TypeFact, candidate: &TypeFact, pointer_bits: u32) -> Option<TypeFact> {
    if current.locked || candidate.ty == NirType::Unknown {
        return None;
    }
    if ptr_nesting_depth(&candidate.ty) > MAX_PTR_NESTING_DEPTH {
        return None;
    }
    if current.ty == candidate.ty {
        if candidate.strength > current.strength {
            return Some(TypeFact {
                ty: current.ty.clone(),
                strength: candidate.strength,
                locked: current.locked,
            });
        }
        return None;
    }
    if current.ty == NirType::Unknown {
        return Some(TypeFact {
            ty: candidate.ty.clone(),
            strength: candidate.strength,
            locked: false,
        });
    }
    if !compatible_width(&current.ty, &candidate.ty, pointer_bits) {
        return None;
    }
    let stronger_evidence = candidate.strength > current.strength;
    let more_specific = candidate.strength == current.strength
        && type_specificity(&candidate.ty) > type_specificity(&current.ty);
    (stronger_evidence || more_specific).then(|| TypeFact {
        ty: candidate.ty.clone(),
        strength: candidate.strength,
        locked: false,
    })
}

fn direct_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        _ => None,
    }
}

fn direct_pointer_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr, .. } | PreHirExpr::PtrOffset { base: expr, .. } => {
            direct_pointer_var(expr)
        }
        _ => None,
    }
}

fn collect_assignment_edges(lhs: &PreHirLValue, rhs: &PreHirExpr, edges: &mut Vec<TypeFlowEdge>) {
    match lhs {
        PreHirLValue::Var(lhs_name) => match rhs {
            PreHirExpr::Var(rhs_name) => edges.push(TypeFlowEdge::Copy {
                lhs: lhs_name.clone(),
                rhs: rhs_name.clone(),
            }),
            PreHirExpr::Load { ptr, ty } => {
                if let Some(pointer) = direct_pointer_var(ptr) {
                    edges.push(TypeFlowEdge::Load {
                        pointer: pointer.to_owned(),
                        value: lhs_name.clone(),
                        access_ty: ty.clone(),
                    });
                }
            }
            PreHirExpr::Index { base, elem_ty, .. } => {
                if let Some(pointer) = direct_pointer_var(base) {
                    edges.push(TypeFlowEdge::Load {
                        pointer: pointer.to_owned(),
                        value: lhs_name.clone(),
                        access_ty: elem_ty.clone(),
                    });
                }
            }
            PreHirExpr::Cast { ty, .. } => edges.push(TypeFlowEdge::Cast {
                output: lhs_name.clone(),
                ty: ty.clone(),
            }),
            _ => {
                let ty = expr_type(rhs);
                if ty != NirType::Unknown {
                    edges.push(TypeFlowEdge::Cast {
                        output: lhs_name.clone(),
                        ty,
                    });
                }
            }
        },
        PreHirLValue::Deref { ptr, ty } => {
            if let Some(pointer) = direct_pointer_var(ptr) {
                if let Some(value) = direct_var(rhs) {
                    edges.push(TypeFlowEdge::Store {
                        pointer: pointer.to_owned(),
                        value: value.to_owned(),
                        access_ty: ty.clone(),
                    });
                } else {
                    edges.push(TypeFlowEdge::PointerAccess {
                        pointer: pointer.to_owned(),
                        access_ty: ty.clone(),
                    });
                }
            }
        }
        PreHirLValue::Index { base, elem_ty, .. } => {
            if let Some(pointer) = direct_pointer_var(base) {
                if let Some(value) = direct_var(rhs) {
                    edges.push(TypeFlowEdge::Store {
                        pointer: pointer.to_owned(),
                        value: value.to_owned(),
                        access_ty: elem_ty.clone(),
                    });
                } else {
                    edges.push(TypeFlowEdge::PointerAccess {
                        pointer: pointer.to_owned(),
                        access_ty: elem_ty.clone(),
                    });
                }
            }
        }
        PreHirLValue::FieldAccess { .. } => {}
    }
}

fn collect_edges(stmts: &[PreHirStmt], edges: &mut Vec<TypeFlowEdge>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => collect_assignment_edges(lhs, rhs, edges),
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_edges(body, edges)
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_edges(std::slice::from_ref(init.as_ref()), edges);
                }
                if let Some(update) = update {
                    collect_edges(std::slice::from_ref(update.as_ref()), edges);
                }
                collect_edges(body, edges);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_edges(then_body, edges);
                collect_edges(else_body, edges);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_edges(&case.body, edges);
                }
                collect_edges(default, edges);
            }
            _ => {}
        }
    }
}

fn collect_definition_counts(stmts: &[PreHirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } => *counts.entry(name.clone()).or_default() += 1,
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_definition_counts(body, counts)
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_definition_counts(std::slice::from_ref(init.as_ref()), counts);
                }
                if let Some(update) = update {
                    collect_definition_counts(std::slice::from_ref(update.as_ref()), counts);
                }
                collect_definition_counts(body, counts);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_definition_counts(then_body, counts);
                collect_definition_counts(else_body, counts);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_definition_counts(&case.body, counts);
                }
                collect_definition_counts(default, counts);
            }
            _ => {}
        }
    }
}

pub(super) fn apply_type_flow_pass(func: &mut PreHirFunction) -> bool {
    let solved = TypeFlowSolver::from_function(func).solve();
    let mut changed = false;
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        let Some(fact) = solved.get(&binding.name) else {
            continue;
        };
        if binding.surface_type_name.is_none()
            && fact.ty != NirType::Unknown
            && binding.ty != fact.ty
        {
            binding.ty = fact.ty.clone();
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    fn function(
        params: Vec<PreHirBinding>,
        locals: Vec<PreHirBinding>,
        body: Vec<PreHirStmt>,
    ) -> PreHirFunction {
        PreHirFunction {
            name: "type_flow".to_owned(),
            params,
            locals,
            body,
            is_64bit: true,
            ..Default::default()
        }
    }

    #[test]
    fn semantic_store_refines_long_pointer_alias_chain_without_round_cap() {
        let uint = NirType::Int {
            bits: 32,
            signed: false,
        };
        let float = NirType::Float { bits: 32 };
        let uint_ptr = NirType::Ptr(Box::new(uint.clone()));
        let mut locals = (0..8)
            .map(|index| {
                binding(
                    &format!("alias_{index}"),
                    uint_ptr.clone(),
                    NirBindingOrigin::Temp,
                )
            })
            .collect::<Vec<_>>();
        locals.push(binding("value", float.clone(), NirBindingOrigin::Temp));
        let mut body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("alias_0".to_owned()),
            rhs: PreHirExpr::Var("out".to_owned()),
        }];
        for index in 1..8 {
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(format!("alias_{index}")),
                rhs: PreHirExpr::Var(format!("alias_{}", index - 1)),
            });
        }
        body.push(PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("alias_7".to_owned())),
                ty: uint,
            },
            rhs: PreHirExpr::Var("value".to_owned()),
        });
        let mut func = function(
            vec![binding("out", uint_ptr, NirBindingOrigin::ParamIndex(0))],
            locals,
            body,
        );

        assert!(apply_type_flow_pass(&mut func));
        let expected = NirType::Ptr(Box::new(float));
        assert_eq!(func.params[0].ty, expected);
        assert!(
            func.locals
                .iter()
                .filter(|binding| binding.name.starts_with("alias_"))
                .all(|binding| binding.ty == expected)
        );
    }

    #[test]
    fn refine_fact_rejects_pointer_nesting_beyond_cap() {
        let mut deep = NirType::Unknown;
        for _ in 0..(MAX_PTR_NESTING_DEPTH + 2) {
            deep = NirType::Ptr(Box::new(deep));
        }
        let current = TypeFact::unknown();
        let candidate = TypeFact {
            ty: deep,
            strength: EvidenceStrength::Semantic,
            locked: false,
        };
        assert!(refine_fact(&current, &candidate, 64).is_none());
    }

    #[test]
    fn apply_type_flow_pass_does_not_grow_pointer_depth_on_self_referential_load() {
        // `x = *x`: reuses one temp for both the address being dereferenced
        // and the loaded result (the in-place pointer-chase idiom). Before
        // the fix, each solver pass re-derived "x is a pointer to its own
        // prior type" and wrapped another `Ptr(...)` layer around it,
        // growing without bound across repeated fixed-point rounds.
        let int32 = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_int32 = NirType::Ptr(Box::new(int32));
        let mut func = function(
            vec![],
            vec![binding("x", ptr_int32.clone(), NirBindingOrigin::Temp)],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_owned()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("x".to_owned())),
                    ty: ptr_int32,
                },
            }],
        );

        for _ in 0..20 {
            apply_type_flow_pass(&mut func);
        }

        assert!(
            ptr_nesting_depth(&func.locals[0].ty) <= MAX_PTR_NESTING_DEPTH,
            "x grew to {:?}",
            func.locals[0].ty
        );
    }

    #[test]
    fn load_propagates_pointer_pointee_to_value() {
        let signed = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = function(
            vec![binding(
                "input",
                NirType::Ptr(Box::new(signed.clone())),
                NirBindingOrigin::ParamIndex(0),
            )],
            vec![binding("value", NirType::Unknown, NirBindingOrigin::Temp)],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("value".to_owned()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("input".to_owned())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            }],
        );

        assert!(apply_type_flow_pass(&mut func));
        assert_eq!(func.locals[0].ty, signed);
    }

    #[test]
    fn store_transfer_is_generic_for_pointer_values() {
        let uint64 = NirType::Int {
            bits: 64,
            signed: false,
        };
        let value_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 32,
            signed: true,
        }));
        let mut func = function(
            vec![binding(
                "out",
                NirType::Ptr(Box::new(uint64.clone())),
                NirBindingOrigin::ParamIndex(0),
            )],
            vec![binding("value", value_ty.clone(), NirBindingOrigin::Temp)],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("out".to_owned())),
                    ty: uint64,
                },
                rhs: PreHirExpr::Var("value".to_owned()),
            }],
        );

        assert!(apply_type_flow_pass(&mut func));
        assert_eq!(func.params[0].ty, NirType::Ptr(Box::new(value_ty)));
    }

    #[test]
    fn surface_locked_binding_is_not_overwritten() {
        let mut out = binding(
            "out",
            NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            NirBindingOrigin::ParamIndex(0),
        );
        out.surface_type_name = Some("DWORD *".to_owned());
        let mut func = function(
            vec![out],
            vec![binding(
                "value",
                NirType::Float { bits: 32 },
                NirBindingOrigin::Temp,
            )],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("out".to_owned())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Var("value".to_owned()),
            }],
        );

        assert!(!apply_type_flow_pass(&mut func));
        assert_eq!(func.params[0].surface_type_name.as_deref(), Some("DWORD *"));
        assert!(matches!(
            func.params[0].ty,
            NirType::Ptr(ref pointee)
                if matches!(pointee.as_ref(), NirType::Int { bits: 32, signed: false })
        ));
    }

    #[test]
    fn multi_definition_copy_does_not_back_propagate_later_scalar_result() {
        let float_ptr = NirType::Ptr(Box::new(NirType::Float { bits: 32 }));
        let uint64 = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = function(
            vec![binding(
                "input",
                float_ptr.clone(),
                NirBindingOrigin::ParamIndex(0),
            )],
            vec![binding("reuse", uint64.clone(), NirBindingOrigin::Temp)],
            vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("reuse".to_owned()),
                    rhs: PreHirExpr::Var("input".to_owned()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("reuse".to_owned()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Const(1, uint64.clone())),
                        rhs: Box::new(PreHirExpr::Const(2, uint64.clone())),
                        ty: uint64,
                    },
                },
            ],
        );

        assert!(!apply_type_flow_pass(&mut func));
        assert_eq!(func.params[0].ty, float_ptr);
    }
}
