/// HIR-level def-use analysis and dataflow-based normalization passes.
///
/// These passes extend the existing name-pattern-based cleanup in `cleanup.rs`
/// with proper graph-theoretic analysis:
///
/// - [`DefUseMap`] counts every read of a named variable across the ENTIRE
///   function body (all nesting levels), without any name-pattern assumption.
/// - [`constant_folding_pass`] evaluates binary and unary expressions whose
///   operands are both compile-time constants.  Pure algebra, binary-independent.
/// - [`defuse_dead_assignment_pass`] removes flat-level assignments to any
///   variable whose use count is zero in the whole function body and whose
///   RHS has no observable side effects.
use super::super::cleanup::{expr_has_side_effects, prune_unused_temp_bindings};
use crate::prelude::*;
use crate::analysis::expr_key::pure_expr_key;
use crate::pipeline::normalize_expr;
use fission_midend_core::wave_stats;
use fission_midend_core::next_temp_name;
use fission_midend_core::util_dir::expr_type;
use crate::{HashMap, HashSet};

const WIDE_DEAD_ASSIGNMENT_RERUN_STMT_LIMIT: usize = 220;
const WIDE_DEAD_ASSIGNMENT_RERUN_LOCAL_LIMIT: usize = 160;

// ── DefUseMap ─────────────────────────────────────────────────────────────────

/// Function-level use-count map for named HIR variables.
///
/// Counts every `Var(name)` occurrence that is used as an *rvalue* anywhere in
/// the function body.  LHS variable names in direct Assign statements
/// (`Assign { lhs: Var(_), .. }`) are NOT counted — they are definition sites.
pub struct DefUseMap {
    /// Number of rvalue uses of each variable name across the whole body.
    pub use_count: HashMap<String, usize>,
}

impl DefUseMap {
    pub fn build(stmts: &[DirStmt]) -> Self {
        let mut map = Self {
            use_count: HashMap::default(),
        };
        for stmt in stmts {
            map.count_stmt(stmt);
        }
        map
    }

    fn count_stmt(&mut self, stmt: &DirStmt) {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                self.count_lvalue(lhs);
                self.count_expr(rhs);
            }
            DirStmt::VaStart { va_list, .. } => self.count_expr(va_list),
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => self.count_expr(expr),
            DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue
            | DirStmt::Label(_)
            | DirStmt::Goto(_) => {}
            DirStmt::Block(stmts) => {
                for s in stmts {
                    self.count_stmt(s);
                }
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.count_expr(cond);
                for s in then_body {
                    self.count_stmt(s);
                }
                for s in else_body {
                    self.count_stmt(s);
                }
            }
            DirStmt::While { cond, body } => {
                self.count_expr(cond);
                for s in body {
                    self.count_stmt(s);
                }
            }
            DirStmt::DoWhile { body, cond } => {
                for s in body {
                    self.count_stmt(s);
                }
                self.count_expr(cond);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.count_stmt(i);
                }
                if let Some(c) = cond {
                    self.count_expr(c);
                }
                if let Some(u) = update {
                    self.count_stmt(u);
                }
                for s in body {
                    self.count_stmt(s);
                }
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.count_expr(expr);
                for case in cases {
                    for s in &case.body {
                        self.count_stmt(s);
                    }
                }
                for s in default {
                    self.count_stmt(s);
                }
            }
        }
    }

    fn count_lvalue(&mut self, lhs: &DirLValue) {
        match lhs {
            // The defined name is a write site — not an rvalue use.
            DirLValue::Var(_) => {}
            DirLValue::Deref { ptr, .. } => self.count_expr(ptr),
            DirLValue::Index { base, index, .. } => {
                self.count_expr(base);
                self.count_expr(index);
            }
            DirLValue::FieldAccess { base, .. } => self.count_expr(base),
        }
    }

    fn count_expr(&mut self, expr: &DirExpr) {
        match expr {
            DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
                *self.use_count.entry(name.clone()).or_default() += 1;
            }
            DirExpr::Const(_, _) => {}
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::Load { ptr: expr, .. }
            | DirExpr::PtrOffset { base: expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => self.count_expr(expr),
            DirExpr::FieldAccess { base, .. } => self.count_expr(base),
            DirExpr::Binary { lhs, rhs, .. } => {
                self.count_expr(lhs);
                self.count_expr(rhs);
            }
            DirExpr::Call { args, .. } => {
                for a in args {
                    self.count_expr(a);
                }
            }
            DirExpr::Index { base, index, .. } => {
                self.count_expr(base);
                self.count_expr(index);
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.count_expr(cond);
                self.count_expr(then_expr);
                self.count_expr(else_expr);
            }
        }
    }
}

/// Conservative dependency graph across every definition of an HIR binding.
///
/// Unlike first-definition type maps, this graph retains contributors from
/// later redefinitions. Consumers can walk from an address binding back to a
/// constrained root set without depending on variable naming conventions.
pub struct DefinitionDependencyMap {
    dependencies: HashMap<String, HashSet<String>>,
    address_dependencies: HashMap<String, HashSet<String>>,
}

/// Proof that a dependency node has a path to one of a fixed set of roots.
///
/// Computing the reverse closure first makes the query stable for loop-carried
/// definitions. A recursive DFS cannot soundly reject a back-edge before it
/// has seen a root reached through another edge of the same SCC.
struct RootReachabilityProof<'a> {
    dependencies: &'a HashMap<String, HashSet<String>>,
    can_reach_roots: HashSet<String>,
}

impl<'a> RootReachabilityProof<'a> {
    fn build(dependencies: &'a HashMap<String, HashSet<String>>, roots: &HashSet<String>) -> Self {
        let mut reverse_dependencies: HashMap<String, HashSet<String>> = HashMap::default();
        for (dependent, sources) in dependencies {
            for source in sources {
                reverse_dependencies
                    .entry(source.clone())
                    .or_default()
                    .insert(dependent.clone());
            }
        }

        let mut can_reach_roots = roots.clone();
        let mut worklist: Vec<String> = roots.iter().cloned().collect();
        while let Some(source) = worklist.pop() {
            let Some(dependents) = reverse_dependencies.get(&source) else {
                continue;
            };
            for dependent in dependents {
                if can_reach_roots.insert(dependent.clone()) {
                    worklist.push(dependent.clone());
                }
            }
        }

        Self {
            dependencies,
            can_reach_roots,
        }
    }

    fn nodes_reaching_from(&self, name: &str) -> HashSet<String> {
        if !self.can_reach_roots.contains(name) {
            return HashSet::default();
        }

        let mut reached = HashSet::default();
        let mut worklist = vec![name.to_string()];
        while let Some(candidate) = worklist.pop() {
            if !self.can_reach_roots.contains(&candidate) || !reached.insert(candidate.clone()) {
                continue;
            }
            if let Some(sources) = self.dependencies.get(&candidate) {
                worklist.extend(sources.iter().cloned());
            }
        }
        reached
    }
}

impl DefinitionDependencyMap {
    pub fn build(stmts: &[DirStmt]) -> Self {
        let mut map = Self {
            dependencies: HashMap::default(),
            address_dependencies: HashMap::default(),
        };
        map.collect_stmts(stmts);
        map
    }

    pub fn roots_reaching(&self, name: &str, roots: &HashSet<String>) -> HashSet<String> {
        let mut reached = HashSet::default();
        let mut visited = HashSet::default();
        self.collect_roots(name, roots, &mut visited, &mut reached);
        reached
    }

    pub fn address_roots_reaching(
        &self,
        name: &str,
        roots: &HashSet<String>,
    ) -> HashSet<String> {
        self.address_nodes_reaching_roots(name, roots)
            .into_iter()
            .filter(|candidate| roots.contains(candidate))
            .collect()
    }

    pub fn nodes_reaching_roots(
        &self,
        name: &str,
        roots: &HashSet<String>,
    ) -> HashSet<String> {
        RootReachabilityProof::build(&self.dependencies, roots).nodes_reaching_from(name)
    }

    pub fn address_contributors(
        &self,
        stmts: &[DirStmt],
        pointer_roots: &HashSet<String>,
    ) -> HashMap<String, NirType> {
        let mut contributors = HashMap::default();
        // Built once for the whole walk: `pointer_roots` is fixed for every
        // call site below, so the reverse-dependency closure this computes
        // doesn't need to be recomputed per address-provenance query (it
        // used to be rebuilt from scratch on every pointer dereference in
        // the function via `address_nodes_reaching_roots`).
        let proof = RootReachabilityProof::build(&self.address_dependencies, pointer_roots);
        collect_address_contributors_stmts(&proof, stmts, &mut contributors);
        contributors
    }

    fn address_nodes_reaching_roots(&self, name: &str, roots: &HashSet<String>) -> HashSet<String> {
        RootReachabilityProof::build(&self.address_dependencies, roots).nodes_reaching_from(name)
    }

    fn collect_roots(
        &self,
        name: &str,
        roots: &HashSet<String>,
        visited: &mut HashSet<String>,
        reached: &mut HashSet<String>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        if roots.contains(name) {
            reached.insert(name.to_string());
            return;
        }
        if let Some(dependencies) = self.dependencies.get(name) {
            for dependency in dependencies {
                self.collect_roots(dependency, roots, visited, reached);
            }
        }
    }

    fn collect_stmts(&mut self, stmts: &[DirStmt]) {
        for stmt in stmts {
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &DirStmt) {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } => {
                let dependencies = self.dependencies.entry(name.clone()).or_default();
                collect_expr_vars(rhs, dependencies);
                let address_dependencies =
                    self.address_dependencies.entry(name.clone()).or_default();
                collect_address_provenance_vars(rhs, address_dependencies);
            }
            DirStmt::Assign { .. }
            | DirStmt::Expr(_)
            | DirStmt::Return(_)
            | DirStmt::VaStart { .. }
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
            DirStmt::Block(body) | DirStmt::While { body, .. } => self.collect_stmts(body),
            DirStmt::DoWhile { body, .. } => self.collect_stmts(body),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                self.collect_stmts(then_body);
                self.collect_stmts(else_body);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    self.collect_stmt(init);
                }
                self.collect_stmts(body);
                if let Some(update) = update {
                    self.collect_stmt(update);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    self.collect_stmts(&case.body);
                }
                self.collect_stmts(default);
            }
        }
    }
}

/// Collect dependencies that can preserve pointer identity through a value
/// definition. Memory reads and call returns are provenance barriers: their
/// result does not inherit pointer identity from the address or arguments used
/// to produce it.
fn collect_address_provenance_vars(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            out.insert(name.clone());
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_address_provenance_vars(expr, out);
        }
        DirExpr::PtrOffset { base, .. } => collect_address_provenance_vars(base, out),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_address_provenance_vars(lhs, out);
            collect_address_provenance_vars(rhs, out);
        }
        DirExpr::Select {
            then_expr,
            else_expr,
            ..
        } => {
            collect_address_provenance_vars(then_expr, out);
            collect_address_provenance_vars(else_expr, out);
        }
        DirExpr::Load { .. }
        | DirExpr::Call { .. }
        | DirExpr::Index { .. }
        | DirExpr::FieldAccess { .. }
        | DirExpr::AggregateCopy { .. }
        | DirExpr::AddressOfGlobal(_)
        | DirExpr::Const(_, _) => {}
    }
}

pub fn collect_expr_vars(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            out.insert(name.clone());
        }
        DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => collect_expr_vars(expr, out),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_expr_vars(lhs, out);
            collect_expr_vars(rhs, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_expr_vars(arg, out);
            }
        }
        DirExpr::Index { base, index, .. } => {
            collect_expr_vars(base, out);
            collect_expr_vars(index, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_vars(cond, out);
            collect_expr_vars(then_expr, out);
            collect_expr_vars(else_expr, out);
        }
    }
}

fn collect_address_contributors_stmts(
    proof: &RootReachabilityProof,
    stmts: &[DirStmt],
    out: &mut HashMap<String, NirType>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                collect_address_contributors_lvalue(proof, lhs, out);
                collect_address_contributors_expr(proof, rhs, out);
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_address_contributors_expr(proof, va_list, out);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                collect_address_contributors_expr(proof, expr, out);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_address_contributors_stmts(proof, body, out);
            }
            DirStmt::DoWhile { body, cond } => {
                collect_address_contributors_stmts(proof, body, out);
                collect_address_contributors_expr(proof, cond, out);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_address_contributors_expr(proof, cond, out);
                collect_address_contributors_stmts(proof, then_body, out);
                collect_address_contributors_stmts(proof, else_body, out);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_address_contributors_stmts(proof, std::slice::from_ref(init.as_ref()), out);
                }
                if let Some(cond) = cond {
                    collect_address_contributors_expr(proof, cond, out);
                }
                if let Some(update) = update {
                    collect_address_contributors_stmts(proof, std::slice::from_ref(update.as_ref()), out);
                }
                collect_address_contributors_stmts(proof, body, out);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_address_contributors_expr(proof, expr, out);
                for case in cases {
                    collect_address_contributors_stmts(proof, &case.body, out);
                }
                collect_address_contributors_stmts(proof, default, out);
            }
            DirStmt::Return(None)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn collect_address_contributors_lvalue(
    proof: &RootReachabilityProof,
    lhs: &DirLValue,
    out: &mut HashMap<String, NirType>,
) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, ty } => {
            record_address_contributors(proof, ptr, ty, out);
            collect_address_contributors_expr(proof, ptr, out);
        }
        DirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            record_address_contributors(proof, base, elem_ty, out);
            collect_address_contributors_expr(proof, base, out);
            collect_address_contributors_expr(proof, index, out);
        }
        DirLValue::FieldAccess { base, ty, .. } => {
            record_address_contributors(proof, base, ty, out);
            collect_address_contributors_expr(proof, base, out);
        }
    }
}

fn collect_address_contributors_expr(
    proof: &RootReachabilityProof,
    expr: &DirExpr,
    out: &mut HashMap<String, NirType>,
) {
    match expr {
        DirExpr::Load { ptr, ty } => {
            record_address_contributors(proof, ptr, ty, out);
            collect_address_contributors_expr(proof, ptr, out);
        }
        DirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            record_address_contributors(proof, base, elem_ty, out);
            collect_address_contributors_expr(proof, base, out);
            collect_address_contributors_expr(proof, index, out);
        }
        DirExpr::FieldAccess { base, ty, .. } => {
            record_address_contributors(proof, base, ty, out);
            collect_address_contributors_expr(proof, base, out);
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            collect_address_contributors_expr(proof, expr, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_address_contributors_expr(proof, lhs, out);
            collect_address_contributors_expr(proof, rhs, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_address_contributors_expr(proof, arg, out);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_address_contributors_expr(proof, cond, out);
            collect_address_contributors_expr(proof, then_expr, out);
            collect_address_contributors_expr(proof, else_expr, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn record_address_contributors(
    proof: &RootReachabilityProof,
    address: &DirExpr,
    pointee: &NirType,
    out: &mut HashMap<String, NirType>,
) {
    let mut address_names = HashSet::default();
    collect_address_provenance_vars(address, &mut address_names);
    for name in address_names {
        for contributor in proof.nodes_reaching_from(&name) {
            out.entry(contributor).or_insert_with(|| pointee.clone());
        }
    }
}

// ── Constant folding ──────────────────────────────────────────────────────────

/// Evaluate binary/unary/cast expressions whose operands are compile-time
/// constants.  Returns `true` if any rewrite was made.
///
/// Rules (all binary-independent, pure algebra):
/// - `Binary(op, Const(a), Const(b)) → Const(eval(op,a,b))`
/// - `Unary(Neg, Const(a)) → Const(-a)`, `Unary(Not|BitNot, Const(a)) → Const(~a)`
/// - `Cast(IntN, Const(a)) → Const(a & mask_N)`
///
/// Overflow uses wrapping arithmetic to match x86 semantics.
pub fn constant_folding_pass(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= fold_stmt(stmt);
    }
    changed
}

fn fold_stmt(stmt: &mut DirStmt) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            fold_lvalue(lhs);
            changed |= fold_expr(rhs);
        }
        DirStmt::VaStart { va_list, .. } => changed |= fold_expr(va_list),
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => changed |= fold_expr(expr),
        DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue
        | DirStmt::Label(_)
        | DirStmt::Goto(_) => {}
        DirStmt::Block(stmts) => changed |= constant_folding_pass(stmts),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= fold_expr(cond);
            changed |= constant_folding_pass(then_body);
            changed |= constant_folding_pass(else_body);
        }
        DirStmt::While { cond, body } => {
            changed |= fold_expr(cond);
            changed |= constant_folding_pass(body);
        }
        DirStmt::DoWhile { body, cond } => {
            changed |= constant_folding_pass(body);
            changed |= fold_expr(cond);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= fold_stmt(i);
            }
            if let Some(c) = cond {
                changed |= fold_expr(c);
            }
            if let Some(u) = update {
                changed |= fold_stmt(u);
            }
            changed |= constant_folding_pass(body);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= fold_expr(expr);
            for case in cases.iter_mut() {
                changed |= constant_folding_pass(&mut case.body);
            }
            changed |= constant_folding_pass(default);
        }
    }
    changed
}

fn fold_lvalue(lhs: &mut DirLValue) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => {
            fold_expr(ptr);
        }
        DirLValue::Index { base, index, .. } => {
            fold_expr(base);
            fold_expr(index);
        }
        DirLValue::FieldAccess { base, .. } => {
            fold_expr(base);
        }
    }
}

/// Recursively fold constant sub-expressions bottom-up.
fn fold_expr(expr: &mut DirExpr) -> bool {
    // Fold children first.
    let mut changed = false;
    match expr {
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= fold_expr(lhs);
            changed |= fold_expr(rhs);
        }
        DirExpr::Unary { expr: inner, .. } | DirExpr::Cast { expr: inner, .. } => {
            changed |= fold_expr(inner);
        }
        DirExpr::Load { ptr, .. } | DirExpr::PtrOffset { base: ptr, .. } => {
            changed |= fold_expr(ptr);
        }
        DirExpr::FieldAccess { base, .. } => {
            changed |= fold_expr(base);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= fold_expr(base);
            changed |= fold_expr(index);
        }
        DirExpr::AggregateCopy { src, .. } => {
            changed |= fold_expr(src);
        }
        DirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= fold_expr(a);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= fold_expr(cond);
            changed |= fold_expr(then_expr);
            changed |= fold_expr(else_expr);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    // Try to fold this node.
    if let Some(folded) = try_fold(expr) {
        *expr = folded;
        changed = true;
    }
    changed
}

/// Expose bottom-up constant folding for passes that rewrite expressions in place
/// (e.g. SCCP after substituting known variables).
pub fn fold_expr_hir(expr: &mut DirExpr) -> bool {
    fold_expr(expr)
}

/// Evaluate `expr` to a compile-time integer/bool constant using `env` for
/// `Var` bindings.  Returns `None` for `Load`/`Call`/non-constant leaves.
pub fn eval_hir_expr_with_const_env(
    expr: &DirExpr,
    env: &HashMap<String, (i64, NirType)>,
) -> Option<(i64, NirType)> {
    match expr {
        DirExpr::Const(v, ty) => Some((*v, ty.clone())),
        DirExpr::Var(name) => env.get(name).map(|(v, t)| (*v, t.clone())),
        DirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
            let (a, _) = eval_hir_expr_with_const_env(inner, env)?;
            let result = eval_unary(*op, a, ty)?;
            Some((result, ty.clone()))
        }
        DirExpr::Binary { op, lhs, rhs, ty } => {
            let (a, _) = eval_hir_expr_with_const_env(lhs, env)?;
            let (b, _) = eval_hir_expr_with_const_env(rhs, env)?;
            let result = eval_binary(*op, a, b, ty)?;
            Some((result, ty.clone()))
        }
        DirExpr::Cast { ty, expr: inner } => {
            let (a, _) = eval_hir_expr_with_const_env(inner, env)?;
            let result = truncate_const(a, ty)?;
            Some((result, ty.clone()))
        }
        DirExpr::AddressOfGlobal(_)
        | DirExpr::Load { .. }
        | DirExpr::Call { .. }
        | DirExpr::PtrOffset { .. }
        | DirExpr::Index { .. }
        | DirExpr::Select { .. }
        | DirExpr::FieldAccess { .. }
        | DirExpr::AggregateCopy { .. } => None,
    }
}

fn try_fold(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Binary { op, lhs, rhs, ty } => {
            let DirExpr::Const(a, _) = lhs.as_ref() else {
                return None;
            };
            let DirExpr::Const(b, _) = rhs.as_ref() else {
                return None;
            };
            let result = eval_binary(*op, *a, *b, ty)?;
            Some(DirExpr::Const(result, ty.clone()))
        }
        DirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
            let DirExpr::Const(a, _) = inner.as_ref() else {
                return None;
            };
            let result = eval_unary(*op, *a, ty)?;
            Some(DirExpr::Const(result, ty.clone()))
        }
        DirExpr::Cast { ty, expr: inner } => {
            let DirExpr::Const(a, _) = inner.as_ref() else {
                return None;
            };
            let result = truncate_const(*a, ty)?;
            Some(DirExpr::Const(result, ty.clone()))
        }
        _ => None,
    }
}

fn eval_binary(op: DirBinaryOp, a: i64, b: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    let result: i64 = match op {
        DirBinaryOp::Add => a.wrapping_add(b),
        DirBinaryOp::Sub => a.wrapping_sub(b),
        DirBinaryOp::Mul => a.wrapping_mul(b),
        DirBinaryOp::And => a & b,
        DirBinaryOp::Or => a | b,
        DirBinaryOp::Xor => a ^ b,
        DirBinaryOp::LogicalAnd => i64::from((a != 0) && (b != 0)),
        DirBinaryOp::LogicalOr => i64::from((a != 0) || (b != 0)),
        DirBinaryOp::Shl => {
            if b < 0 || b >= 64 {
                return None;
            }
            a.wrapping_shl(b as u32)
        }
        DirBinaryOp::Shr => {
            if b < 0 || b >= 64 {
                return None;
            }
            ((a as u64).wrapping_shr(b as u32)) as i64
        }
        DirBinaryOp::Sar => {
            if b < 0 || b >= 64 {
                return None;
            }
            a.wrapping_shr(b as u32)
        }
        DirBinaryOp::Eq => i64::from(a == b),
        DirBinaryOp::Ne => i64::from(a != b),
        DirBinaryOp::Lt => i64::from((a as u64) < (b as u64)),
        DirBinaryOp::Le => i64::from((a as u64) <= (b as u64)),
        DirBinaryOp::Gt => i64::from((a as u64) > (b as u64)),
        DirBinaryOp::Ge => i64::from((a as u64) >= (b as u64)),
        DirBinaryOp::SLt => i64::from(a < b),
        DirBinaryOp::SLe => i64::from(a <= b),
        DirBinaryOp::SGt => i64::from(a > b),
        DirBinaryOp::SGe => i64::from(a >= b),
        DirBinaryOp::Div => {
            let bu = b as u64;
            if bu == 0 {
                return None;
            }
            ((a as u64).wrapping_div(bu)) as i64
        }
        DirBinaryOp::Mod => {
            let bu = b as u64;
            if bu == 0 {
                return None;
            }
            ((a as u64).wrapping_rem(bu)) as i64
        }
    };
    Some(mask_to_bits(result, bits))
}

fn eval_unary(op: DirUnaryOp, a: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    let result = match op {
        DirUnaryOp::Neg => a.wrapping_neg(),
        DirUnaryOp::Not => i64::from(a == 0),
        DirUnaryOp::BitNot => !a,
    };
    Some(mask_to_bits(result, bits))
}

fn truncate_const(a: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    Some(mask_to_bits(a, bits))
}

fn int_or_bool_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

/// Truncate an i64 to the lower `bits` bits using the i64 sign-bit convention
/// used throughout the HIR constant representation.
fn mask_to_bits(value: i64, bits: u32) -> i64 {
    if bits == 0 || bits > 63 {
        return value;
    }
    let mask = (1_i64 << bits).wrapping_sub(1);
    value & mask
}

// ── Dead assignment pass ──────────────────────────────────────────────────────

/// Remove assignments `name = rhs` at any level of the body where
/// `use_count[name] == 0` (never read anywhere in the whole function) and the
/// RHS has no side effects.
///
/// This generalises [`super::cleanup::eliminate_dead_temp_assigns`] to ALL
/// variable names — not only trivially-named temps — using a function-level
/// traversal instead of a flat per-stmt-list scan.
///
/// Safety restriction: only removes assignments to **pure temporary** bindings
/// (those with a temp-like origin).  Stack slots and
/// other memory-backed locals must NOT be removed even when their name is never
/// read, because the write itself may be observable through aliased pointers.
pub fn defuse_dead_assignment_pass(func: &mut DirFunction) -> bool {
    // Collect pure-temp variable names (including builder-preserved temps).
    let mut temp_names: crate::HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like())
        .map(|b| b.name.clone())
        .collect();
    collect_temp_like_assignment_names(&func.body, &mut temp_names);
    if temp_names.is_empty() {
        return false;
    }

    let map = DefUseMap::build(&func.body);
    let mut changed = false;
    remove_dead_in_stmts(&mut func.body, &map, &temp_names, &mut changed);
    if changed {
        // Remove temp bindings that became unreferenced.
        prune_unused_temp_bindings(func);
    }
    changed
}

/// Fixed-point dead temp removal after SCCP/constant folding exposes cross-block dead temps.
pub fn defuse_dead_assignment_fixpoint_pass(func: &mut DirFunction) -> bool {
    let first_changed = defuse_dead_assignment_pass(func);
    if !first_changed {
        return false;
    }
    if !wide_dead_assignment_rerun_admitted(func) {
        if wide_dead_assignment_rerun_admission_enabled() {
            wave_stats::add_wide_dead_assignment_rerun_skipped_by_admission(1);
        }
        return true;
    }
    if wide_dead_assignment_rerun_admission_enabled() {
        wave_stats::add_wide_dead_assignment_rerun_admitted(1);
    }
    for _ in 1..6 {
        if !defuse_dead_assignment_pass(func) {
            break;
        }
    }
    true
}

/// Deprecated alias for [`defuse_dead_assignment_fixpoint_pass`].
pub fn apply_wide_dead_assignment_pass(func: &mut DirFunction) -> bool {
    defuse_dead_assignment_fixpoint_pass(func)
}

fn wide_dead_assignment_rerun_admission_enabled() -> bool {
    // NOT cached (unlike its siblings in this sweep): unit tests below
    // toggle this exact env var at runtime via `set_var`/`remove_var`
    // (see `WideDeadRerunAdmissionEnvGuard`) to exercise both the enabled
    // and disabled paths within the same test binary process. A `OnceLock`
    // here would freeze on whichever value the first caller observed.
    std::env::var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION")
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false)
}

fn wide_dead_assignment_rerun_admitted(func: &DirFunction) -> bool {
    if !wide_dead_assignment_rerun_admission_enabled() {
        return true;
    }
    count_hir_stmts_for_wide_dead_assignment(&func.body) <= WIDE_DEAD_ASSIGNMENT_RERUN_STMT_LIMIT
        && func.locals.len() <= WIDE_DEAD_ASSIGNMENT_RERUN_LOCAL_LIMIT
}

fn count_hir_stmts_for_wide_dead_assignment(stmts: &[DirStmt]) -> usize {
    fn count_stmt(stmt: &DirStmt) -> usize {
        match stmt {
            DirStmt::Block(stmts)
            | DirStmt::While { body: stmts, .. }
            | DirStmt::DoWhile { body: stmts, .. } => {
                1 + count_hir_stmts_for_wide_dead_assignment(stmts)
            }
            DirStmt::Switch { cases, default, .. } => {
                1 + cases
                    .iter()
                    .map(|case| count_hir_stmts_for_wide_dead_assignment(&case.body))
                    .sum::<usize>()
                    + count_hir_stmts_for_wide_dead_assignment(default)
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                1 + count_hir_stmts_for_wide_dead_assignment(then_body)
                    + count_hir_stmts_for_wide_dead_assignment(else_body)
            }
            _ => 1,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

fn remove_dead_in_stmts(
    stmts: &mut Vec<DirStmt>,
    map: &DefUseMap,
    temp_names: &crate::HashSet<String>,
    changed: &mut bool,
) {
    // First recurse into nested bodies.
    for stmt in stmts.iter_mut() {
        remove_dead_in_stmt_nested(stmt, map, temp_names, changed);
    }

    // Then remove flat-level dead assignments to pure temps.
    stmts.retain(|stmt| {
        if let DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } = stmt
        {
            if temp_names.contains(name.as_str()) {
                let uses = map.use_count.get(name.as_str()).copied().unwrap_or(0);
                if uses == 0 && !expr_has_side_effects(rhs) {
                    *changed = true;
                    return false;
                }
            }
        }
        true
    });
}

fn remove_dead_in_stmt_nested(
    stmt: &mut DirStmt,
    map: &DefUseMap,
    temp_names: &crate::HashSet<String>,
    changed: &mut bool,
) {
    match stmt {
        DirStmt::Block(stmts) => remove_dead_in_stmts(stmts, map, temp_names, changed),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_dead_in_stmts(then_body, map, temp_names, changed);
            remove_dead_in_stmts(else_body, map, temp_names, changed);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            remove_dead_in_stmts(body, map, temp_names, changed);
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_dead_in_stmt_nested(i, map, temp_names, changed);
            }
            if let Some(u) = update {
                remove_dead_in_stmt_nested(u, map, temp_names, changed);
            }
            remove_dead_in_stmts(body, map, temp_names, changed);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_dead_in_stmts(&mut case.body, map, temp_names, changed);
            }
            remove_dead_in_stmts(default, map, temp_names, changed);
        }
        _ => {}
    }
}

fn collect_temp_like_assignment_names(
    stmts: &[DirStmt],
    names: &mut crate::HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                ..
            } => {
                if is_temp_like_assignment_name(name) {
                    names.insert(name.clone());
                }
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_temp_like_assignment_names(body, names);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_temp_like_assignment_names(std::slice::from_ref(init.as_ref()), names);
                }
                if let Some(update) = update {
                    collect_temp_like_assignment_names(
                        std::slice::from_ref(update.as_ref()),
                        names,
                    );
                }
                collect_temp_like_assignment_names(body, names);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_temp_like_assignment_names(then_body, names);
                collect_temp_like_assignment_names(else_body, names);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_temp_like_assignment_names(&case.body, names);
                }
                collect_temp_like_assignment_names(default, names);
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn is_temp_like_assignment_name(name: &str) -> bool {
    name == "result" || name == "retval" || temp_name_suffix(name).is_some()
}

// ── Forward-scan fix helper (used by cleanup.rs callers) ─────────────────────

/// Returns `true` if the forward scan for a single-use temp may skip `stmt`
/// when the variable `name` has ZERO uses inside `stmt`.
///
/// This extends the existing `stmt_allows_forward_scan` logic to pass through
/// loops, switches, and blocks that simply don't mention the variable.
pub fn can_skip_stmt_for_var(stmt: &DirStmt, name: &str) -> bool {
    count_any_mention_in_stmt(stmt, name) == 0
}

/// Count all occurrences of `name` in a statement (both reads and the LHS).
fn count_any_mention_in_stmt(stmt: &DirStmt, name: &str) -> usize {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            count_mention_lhs(lhs, name) + count_mention_expr(rhs, name)
        }
        DirStmt::VaStart { va_list, .. } => count_mention_expr(va_list, name),
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => count_mention_expr(expr, name),
        DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue
        | DirStmt::Label(_)
        | DirStmt::Goto(_) => 0,
        DirStmt::Block(stmts) => stmts
            .iter()
            .map(|s| count_any_mention_in_stmt(s, name))
            .sum(),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_mention_expr(cond, name)
                + then_body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
        DirStmt::While { cond, body } => {
            count_mention_expr(cond, name)
                + body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
        DirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|s| count_any_mention_in_stmt(s, name))
                .sum::<usize>()
                + count_mention_expr(cond, name)
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut total = 0;
            if let Some(i) = init {
                total += count_any_mention_in_stmt(i, name);
            }
            if let Some(c) = cond {
                total += count_mention_expr(c, name);
            }
            if let Some(u) = update {
                total += count_any_mention_in_stmt(u, name);
            }
            total += body
                .iter()
                .map(|s| count_any_mention_in_stmt(s, name))
                .sum::<usize>();
            total
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_mention_expr(expr, name)
                + cases
                    .iter()
                    .map(|c| {
                        c.body
                            .iter()
                            .map(|s| count_any_mention_in_stmt(s, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use std::sync::{Mutex, MutexGuard};

    /// Process-wide env is shared across parallel tests; serialize mutations that toggle admission.
    static WIDE_DEAD_RERUN_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct WideDeadRerunAdmissionEnvGuard(MutexGuard<'static, ()>);

    impl WideDeadRerunAdmissionEnvGuard {
        fn set_enabled() -> Self {
            let guard = WIDE_DEAD_RERUN_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            unsafe {
                std::env::set_var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION", "1");
            }
            Self(guard)
        }
    }

    impl Drop for WideDeadRerunAdmissionEnvGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION");
            }
        }
    }

    fn temp_binding(name: &str) -> DirBinding {
        DirBinding {
            name: name.to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn assign_dead_temp(name: &str, value: i64) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name.to_string()),
            rhs: DirExpr::Const(
                value,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }
    }

    fn test_func(stmt_count: usize, local_count: usize) -> DirFunction {
        let body = (0..stmt_count)
            .map(|idx| assign_dead_temp(&format!("xVar{idx}"), idx as i64))
            .collect();
        let locals = (0..local_count)
            .map(|idx| temp_binding(&format!("xVar{idx}")))
            .collect();
        DirFunction {
            name: "wide_dead_assignment_test".to_string(),
            params: Vec::new(),
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            calling_convention: Default::default(),
            int_param_offsets: Vec::new(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        }
    }

    #[test]
    fn definition_dependencies_keep_later_address_contributors() {
        let uint = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("base_alias".into()),
                rhs: DirExpr::Var("base_param".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".into()),
                rhs: DirExpr::Var("index".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".into()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("cursor".into())),
                    rhs: Box::new(DirExpr::Var("base_alias".into())),
                    ty: uint,
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("value".into()),
                rhs: DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("cursor".into())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
        ];
        let dependencies = DefinitionDependencyMap::build(&body);
        let roots = ["base_param".to_string(), "limit_param".to_string()].into_iter().collect::<HashSet<_>>();

        assert_eq!(
            dependencies.roots_reaching("cursor", &roots),
            ["base_param".to_string()].into_iter().collect::<HashSet<_>>()
        );
        let contributors = dependencies.address_contributors(&body, &roots);
        assert!(contributors.contains_key("cursor"));
        assert!(contributors.contains_key("base_alias"));
        assert!(contributors.contains_key("base_param"));
        assert!(!contributors.contains_key("index"));
    }

    #[test]
    fn root_reachability_proof_keeps_loop_carried_scc_members() {
        let uint = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".into()),
                rhs: DirExpr::Var("buffer_param".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor_word".into()),
                rhs: DirExpr::Var("cursor".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor_word".into()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("cursor_word".into())),
                    rhs: Box::new(DirExpr::Const(1, uint)),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".into()),
                rhs: DirExpr::Var("cursor_word".into()),
            },
        ];
        let dependencies = DefinitionDependencyMap::build(&body);
        let roots = ["buffer_param".to_string()].into_iter().collect::<HashSet<_>>();

        assert_eq!(
            dependencies.nodes_reaching_roots("cursor", &roots),
            [
                "buffer_param".to_string(),
                "cursor".to_string(),
                "cursor_word".to_string(),
            ].into_iter().collect::<HashSet<_>>()
        );
    }

    #[test]
    fn memory_load_value_does_not_inherit_address_provenance() {
        let uint = NirType::Int {
            bits: 64,
            signed: false,
        };
        let byte = NirType::Int {
            bits: 8,
            signed: false,
        };
        let add = |lhs: &str, rhs: &str| DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var(lhs.to_string())),
            rhs: Box::new(DirExpr::Var(rhs.to_string())),
            ty: uint.clone(),
        };
        let body = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("base_alias".into()),
                rhs: DirExpr::Var("base_param".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("loaded_value".into()),
                rhs: DirExpr::Load {
                    ptr: Box::new(add("base_alias", "index")),
                    ty: byte.clone(),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("accumulator".into()),
                rhs: add("accumulator", "loaded_value"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("result".into()),
                rhs: DirExpr::Load {
                    ptr: Box::new(add("base_alias", "accumulator")),
                    ty: byte,
                },
            },
        ];
        let dependencies = DefinitionDependencyMap::build(&body);
        let roots = ["base_param".to_string()].into_iter().collect::<HashSet<_>>();
        let contributors = dependencies.address_contributors(&body, &roots);

        assert!(contributors.contains_key("base_param"));
        assert!(contributors.contains_key("base_alias"));
        assert!(!contributors.contains_key("index"));
        assert!(!contributors.contains_key("loaded_value"));
        assert!(!contributors.contains_key("accumulator"));
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_allows_small_function() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(10, 10);
        assert!(wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_skips_large_stmt_budget() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(221, 10);
        assert!(!wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_skips_large_local_budget() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(10, 161);
        assert!(!wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_first_pass_still_runs_when_admission_skips() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let mut func = test_func(221, 221);
        assert!(apply_wide_dead_assignment_pass(&mut func));
        assert!(func.body.is_empty());
    }
}

fn count_mention_lhs(lhs: &DirLValue, name: &str) -> usize {
    match lhs {
        // The direct write to name counts as a mention (redefinition guard).
        DirLValue::Var(n) => usize::from(n == name),
        DirLValue::Deref { ptr, .. } => count_mention_expr(ptr, name),
        DirLValue::Index { base, index, .. } => {
            count_mention_expr(base, name) + count_mention_expr(index, name)
        }
        DirLValue::FieldAccess { base, .. } => count_mention_expr(base, name),
    }
}

fn count_mention_expr(expr: &DirExpr, name: &str) -> usize {
    match expr {
        DirExpr::Var(n) | DirExpr::AddressOfGlobal(n) => usize::from(n.as_str() == name),
        DirExpr::Const(_, _) => 0,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => count_mention_expr(expr, name),
        DirExpr::FieldAccess { base, .. } => count_mention_expr(base, name),
        DirExpr::Binary { lhs, rhs, .. } => {
            count_mention_expr(lhs, name) + count_mention_expr(rhs, name)
        }
        DirExpr::Call { args, .. } => args.iter().map(|a| count_mention_expr(a, name)).sum(),
        DirExpr::Index { base, index, .. } => {
            count_mention_expr(base, name) + count_mention_expr(index, name)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_mention_expr(cond, name)
                + count_mention_expr(then_expr, name)
                + count_mention_expr(else_expr, name)
        }
    }
}

pub fn stabilize_repeated_pure_exprs(func: &mut DirFunction) -> usize {
    let mut next_temp_id = next_temp_name_seed(&func.locals);
    stabilize_repeated_pure_exprs_in_stmts(&mut func.body, &mut func.locals, &mut next_temp_id)
}

fn stabilize_repeated_pure_exprs_in_stmts(
    stmts: &mut Vec<DirStmt>,
    locals: &mut Vec<DirBinding>,
    next_temp_id: &mut u32,
) -> usize {
    let mut changed = 0usize;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for mut stmt in stmts.drain(..) {
        match &mut stmt {
            DirStmt::Block(body) => {
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed += stabilize_repeated_pure_exprs_in_stmts(then_body, locals, next_temp_id);
                changed += stabilize_repeated_pure_exprs_in_stmts(else_body, locals, next_temp_id);
                if let Some((temp_stmt, stabilized_cond)) =
                    stabilize_expr_with_temp(cond, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *cond = stabilized_cond;
                    changed += 1;
                }
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                for case in cases.iter_mut() {
                    changed += stabilize_repeated_pure_exprs_in_stmts(
                        &mut case.body,
                        locals,
                        next_temp_id,
                    );
                }
                changed += stabilize_repeated_pure_exprs_in_stmts(default, locals, next_temp_id);
                if let Some((temp_stmt, stabilized_expr)) =
                    stabilize_expr_with_temp(expr, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *expr = stabilized_expr;
                    changed += 1;
                }
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                if let Some((temp_stmt, stabilized_expr)) =
                    stabilize_expr_with_temp(expr, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *expr = stabilized_expr;
                    changed += 1;
                }
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_mut() {
                    let mut nested = vec![(*init.clone())];
                    changed +=
                        stabilize_repeated_pure_exprs_in_stmts(&mut nested, locals, next_temp_id);
                    if let Some(updated) = nested.into_iter().next() {
                        *init = Box::new(updated);
                    }
                }
                if let Some(update) = update.as_mut() {
                    let mut nested = vec![(*update.clone())];
                    changed +=
                        stabilize_repeated_pure_exprs_in_stmts(&mut nested, locals, next_temp_id);
                    if let Some(updated) = nested.into_iter().next() {
                        *update = Box::new(updated);
                    }
                }
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue
            | DirStmt::Label(_)
            | DirStmt::Goto(_) => {}
        }
        rewritten.push(stmt);
    }

    *stmts = rewritten;
    changed
}

fn stabilize_expr_with_temp(
    expr: &DirExpr,
    locals: &mut Vec<DirBinding>,
    next_temp_id: &mut u32,
) -> Option<(DirStmt, DirExpr)> {
    let best = best_repeated_pure_expr(expr)?;
    let temp_ty = expr_type(&best);
    let temp_name = next_temp_name(&temp_ty, next_temp_id);
    locals.push(DirBinding {
        name: temp_name.clone(),
        ty: temp_ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });
    let mut temp_rhs = best.clone();
    normalize_expr(&mut temp_rhs);
    let replacement = DirExpr::Var(temp_name.clone());
    let mut stabilized_expr = replace_matching_pure_expr(expr, &best, &replacement);
    normalize_expr(&mut stabilized_expr);
    Some((
        DirStmt::Assign {
            lhs: DirLValue::Var(temp_name),
            rhs: temp_rhs,
        },
        stabilized_expr,
    ))
}

fn best_repeated_pure_expr(expr: &DirExpr) -> Option<DirExpr> {
    let mut counts: HashMap<String, (usize, usize, DirExpr)> = HashMap::default();
    collect_repeated_pure_exprs(expr, &mut counts);
    let mut candidates = counts
        .into_values()
        .filter(|(count, nodes, repr)| {
            *count > 1
                && *nodes >= 3
                && count_nonconst_leaf_inputs(repr) >= 2
                && is_stabilization_candidate_expr(repr)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.0.cmp(&a.0))
            .then_with(|| format_expr_key(&a.2).cmp(&format_expr_key(&b.2)))
    });
    candidates.into_iter().next().map(|(_, _, expr)| expr)
}

fn collect_repeated_pure_exprs(
    expr: &DirExpr,
    counts: &mut HashMap<String, (usize, usize, DirExpr)>,
) {
    if let Some(key) = pure_expr_key(expr) {
        let nodes = expr_node_count(expr);
        let entry = counts
            .entry(key)
            .or_insert_with(|| (0, nodes, expr.clone()));
        entry.0 += 1;
        if nodes > entry.1 {
            entry.1 = nodes;
            entry.2 = expr.clone();
        }
    }

    match expr {
        DirExpr::Const(_, _) | DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => {}
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => collect_repeated_pure_exprs(expr, counts),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_repeated_pure_exprs(lhs, counts);
            collect_repeated_pure_exprs(rhs, counts);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_repeated_pure_exprs(arg, counts);
            }
        }
        DirExpr::Index { base, index, .. } => {
            collect_repeated_pure_exprs(base, counts);
            collect_repeated_pure_exprs(index, counts);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_repeated_pure_exprs(cond, counts);
            collect_repeated_pure_exprs(then_expr, counts);
            collect_repeated_pure_exprs(else_expr, counts);
        }
    }
}

fn replace_matching_pure_expr(expr: &DirExpr, needle: &DirExpr, replacement: &DirExpr) -> DirExpr {
    if pure_expr_key(expr) == pure_expr_key(needle) {
        return replacement.clone();
    }

    match expr {
        DirExpr::Const(_, _) | DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => expr.clone(),
        DirExpr::Cast { ty, expr: inner } => DirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(replace_matching_pure_expr(inner, needle, replacement)),
        },
        DirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => DirExpr::Unary {
            op: *op,
            expr: Box::new(replace_matching_pure_expr(inner, needle, replacement)),
            ty: ty.clone(),
        },
        DirExpr::Binary { op, lhs, rhs, ty } => DirExpr::Binary {
            op: *op,
            lhs: Box::new(replace_matching_pure_expr(lhs, needle, replacement)),
            rhs: Box::new(replace_matching_pure_expr(rhs, needle, replacement)),
            ty: ty.clone(),
        },
        DirExpr::Call { target, args, ty } => DirExpr::Call {
            target: target.clone(),
            args: args
                .iter()
                .map(|arg| replace_matching_pure_expr(arg, needle, replacement))
                .collect(),
            ty: ty.clone(),
        },
        DirExpr::Load { ptr, ty } => DirExpr::Load {
            ptr: Box::new(replace_matching_pure_expr(ptr, needle, replacement)),
            ty: ty.clone(),
        },
        DirExpr::PtrOffset { base, offset } => DirExpr::PtrOffset {
            base: Box::new(replace_matching_pure_expr(base, needle, replacement)),
            offset: *offset,
        },
        DirExpr::AggregateCopy { src, size } => DirExpr::AggregateCopy {
            src: Box::new(replace_matching_pure_expr(src, needle, replacement)),
            size: *size,
        },
        DirExpr::Index {
            base,
            index,
            elem_ty,
        } => DirExpr::Index {
            base: Box::new(replace_matching_pure_expr(base, needle, replacement)),
            index: Box::new(replace_matching_pure_expr(index, needle, replacement)),
            elem_ty: elem_ty.clone(),
        },
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => DirExpr::Select {
            cond: Box::new(replace_matching_pure_expr(cond, needle, replacement)),
            then_expr: Box::new(replace_matching_pure_expr(then_expr, needle, replacement)),
            else_expr: Box::new(replace_matching_pure_expr(else_expr, needle, replacement)),
            ty: ty.clone(),
        },
        DirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => DirExpr::FieldAccess {
            base: Box::new(replace_matching_pure_expr(base, needle, replacement)),
            field_name: field_name.clone(),
            offset: *offset,
            ty: ty.clone(),
        },
    }
}

fn is_stabilization_candidate_expr(expr: &DirExpr) -> bool {
    matches!(
        expr,
        DirExpr::Binary {
            op: DirBinaryOp::Add
                | DirBinaryOp::Sub
                | DirBinaryOp::Mul
                | DirBinaryOp::And
                | DirBinaryOp::Or
                | DirBinaryOp::Xor
                | DirBinaryOp::Eq
                | DirBinaryOp::Ne
                | DirBinaryOp::Lt
                | DirBinaryOp::Le
                | DirBinaryOp::Gt
                | DirBinaryOp::Ge
                | DirBinaryOp::SLt
                | DirBinaryOp::SLe
                | DirBinaryOp::SGt
                | DirBinaryOp::SGe
                | DirBinaryOp::Shl
                | DirBinaryOp::Shr
                | DirBinaryOp::Sar,
            ..
        } | DirExpr::Unary { .. }
            | DirExpr::Cast { .. }
    )
}
fn count_nonconst_leaf_inputs(expr: &DirExpr) -> usize {
    match expr {
        DirExpr::Const(_, _) => 0,
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => 1,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => count_nonconst_leaf_inputs(expr),
        DirExpr::Binary { lhs, rhs, .. } => {
            count_nonconst_leaf_inputs(lhs) + count_nonconst_leaf_inputs(rhs)
        }
        DirExpr::Call { args, .. } => args.iter().map(count_nonconst_leaf_inputs).sum(),
        DirExpr::Index { base, index, .. } => {
            count_nonconst_leaf_inputs(base) + count_nonconst_leaf_inputs(index)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_nonconst_leaf_inputs(cond)
                + count_nonconst_leaf_inputs(then_expr)
                + count_nonconst_leaf_inputs(else_expr)
        }
    }
}

fn expr_node_count(expr: &DirExpr) -> usize {
    match expr {
        DirExpr::Const(_, _) | DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => 1,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => 1 + expr_node_count(expr),
        DirExpr::Binary { lhs, rhs, .. } => 1 + expr_node_count(lhs) + expr_node_count(rhs),
        DirExpr::Call { args, .. } => 1 + args.iter().map(expr_node_count).sum::<usize>(),
        DirExpr::Index { base, index, .. } => 1 + expr_node_count(base) + expr_node_count(index),
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => 1 + expr_node_count(cond) + expr_node_count(then_expr) + expr_node_count(else_expr),
    }
}

fn next_temp_name_seed(locals: &[DirBinding]) -> u32 {
    locals
        .iter()
        .filter_map(|binding| temp_name_suffix(&binding.name))
        .max()
        .map_or(0, |suffix| suffix.saturating_add(1))
}

fn temp_name_suffix(name: &str) -> Option<u32> {
    let digit_start = name.find(|ch: char| ch.is_ascii_digit())?;
    let prefix = &name[..digit_start];
    matches!(prefix, "bVar" | "iVar" | "uVar" | "xVar")
        .then(|| name[digit_start..].parse::<u32>().ok())
        .flatten()
}
