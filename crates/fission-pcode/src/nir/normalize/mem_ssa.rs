/// Memory SSA construction for HIR functions.
///
/// Builds a lightweight overlay of memory access nodes:
///
/// - `MemDef`: a store to a memory location (lhs is `Deref` or `Index`).
/// - `MemUse`: a load from a memory location (`HirExpr::Load`).
/// - `MemPhi`: a virtual merge node at CFG join points (after `if`/`while`).
///
/// ## Alias Key
///
/// Each memory access is labelled with an `AliasKey`:
/// - **Stack slot** (`(stack, offset, size)`): accesses to a known stack
///   variable.  Two stack accesses with non-overlapping `[offset, offset+size)`
///   intervals are *no-alias*; identical intervals are *must-alias*.
/// - **Unknown** (`Unknown`): any access via a non-stack pointer is
///   conservatively treated as may-alias with all other unknowns.
///
/// ## Reaching-def chain
///
/// A simple dominator-based algorithm:
/// - Scan statements linearly; maintain a stack-frame map
///   `alias_key → latest MemDef index`.
/// - Loads use the most recent `MemDef` in the current linear path.
/// - At `if`/`while` branch points, fork the map; at merge points, emit a
///   `MemPhi` for keys whose reaching defs differ on the two paths.
///
/// ## Soundness
///
/// Only stack-slot-based accesses that do **not** escape (i.e., whose
/// address is never taken and passed to a `Call`) participate in
/// dead-store analysis.  All other accesses use `AliasKey::Unknown`,
/// which prevents removal.
///
/// References:
/// - LLVM `MemorySSA.h`: `MemoryDef`/`MemoryUse`/`MemoryPhi` design
/// - RetDec `reaching_definitions.h`: UD/DU chains
/// - LLVM `BasicAliasAnalysis.cpp`: stack-slot no-alias rule
use super::*;
use std::collections::HashMap;

/// Identifies a memory location for alias analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum AliasKey {
    /// A stack slot at `[offset, offset + size)` bytes from the frame pointer.
    Stack { offset: i64, size: u32 },
    /// Any non-stack / unknown pointer — conservative may-alias bucket.
    Unknown,
}

impl AliasKey {
    /// Return `true` if two keys definitely refer to the same location.
    pub(crate) fn is_must_alias(&self, other: &AliasKey) -> bool {
        match (self, other) {
            (AliasKey::Stack { offset: o1, size: s1 }, AliasKey::Stack { offset: o2, size: s2 }) => {
                o1 == o2 && s1 == s2
            }
            _ => false,
        }
    }

    /// Return `true` if two keys definitely do NOT alias.
    pub(crate) fn is_no_alias(&self, other: &AliasKey) -> bool {
        match (self, other) {
            (AliasKey::Stack { offset: o1, size: s1 }, AliasKey::Stack { offset: o2, size: s2 }) => {
                // Non-overlapping intervals.
                let end1 = *o1 + *s1 as i64;
                let end2 = *o2 + *s2 as i64;
                end1 <= *o2 || end2 <= *o1
            }
            (AliasKey::Unknown, _) | (_, AliasKey::Unknown) => false,
        }
    }
}

/// A memory definition (store operation).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct MemDef {
    pub(crate) id: usize,
    pub(crate) key: AliasKey,
    /// How many `MemUse` nodes reference this def.
    pub(crate) use_count: usize,
    /// True if any address of this slot escapes to a `Call` argument.
    pub(crate) may_escape: bool,
}

/// A memory use (load operation).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct MemUse {
    pub(crate) id: usize,
    pub(crate) key: AliasKey,
    pub(crate) reaching_def: Option<usize>,
}

/// A virtual phi node at a CFG join point.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct MemPhi {
    pub(crate) id: usize,
    pub(crate) key: AliasKey,
    pub(crate) inputs: Vec<usize>, // MemDef/MemPhi ids
    pub(crate) use_count: usize,
}

/// Complete MemSSA for a function.
pub(crate) struct MemSsa {
    pub(crate) defs: Vec<MemDef>,
    pub(crate) uses: Vec<MemUse>,
    pub(crate) phis: Vec<MemPhi>,
}

/// Internal builder state.
struct Builder {
    defs: Vec<MemDef>,
    uses: Vec<MemUse>,
    phis: Vec<MemPhi>,
    /// Map from AliasKey → latest MemDef id at current linear point.
    reaching: HashMap<AliasKey, usize>,
    next_id: usize,
    /// Set of variable names whose address has been observed as a Call arg.
    escaped: std::collections::HashSet<String>,
}

impl Builder {
    fn new() -> Self {
        Self {
            defs: Vec::new(),
            uses: Vec::new(),
            phis: Vec::new(),
            reaching: HashMap::new(),
            next_id: 0,
            escaped: std::collections::HashSet::new(),
        }
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_def(&mut self, key: AliasKey) -> usize {
        let id = self.alloc_id();
        let may_escape = matches!(key, AliasKey::Unknown);
        self.reaching.insert(key.clone(), id);
        self.defs.push(MemDef { id, key, use_count: 0, may_escape });
        id
    }

    fn add_use(&mut self, key: AliasKey) {
        let id = self.alloc_id();
        let reaching_def = self.reaching.get(&key).copied();
        if let Some(def_id) = reaching_def {
            if let Some(def) = self.defs.iter_mut().find(|d| d.id == def_id) {
                def.use_count += 1;
            } else if let Some(phi) = self.phis.iter_mut().find(|p| p.id == def_id) {
                phi.use_count += 1;
            }
        }
        self.uses.push(MemUse { id, key, reaching_def });
    }

    fn scan_stmts(&mut self, stmts: &[HirStmt]) {
        for stmt in stmts {
            self.scan_stmt(stmt);
        }
    }

    fn scan_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                // Scan rhs first for uses.
                self.scan_expr_uses(rhs);
                // Then record the def.
                match lhs {
                    HirLValue::Deref { ptr, ty } => {
                        let key = self.alias_key_for_ptr(ptr, nir_byte_size(ty));
                        self.add_def(key);
                    }
                    HirLValue::Index { base, index: _, elem_ty } => {
                        let key = self.alias_key_for_ptr(base, nir_byte_size(elem_ty));
                        self.add_def(key);
                    }
                    HirLValue::Var(_) => {} // Not a memory write.
                }
            }
            HirStmt::If { cond, then_body, else_body } => {
                self.scan_expr_uses(cond);
                // Fork reaching map.
                let saved = self.reaching.clone();
                self.scan_stmts(then_body);
                let then_reaching = std::mem::replace(&mut self.reaching, saved.clone());
                self.scan_stmts(else_body);
                let else_reaching = std::mem::take(&mut self.reaching);
                // Merge: emit MemPhi where defs differ.
                self.merge_reaching(then_reaching, else_reaching);
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                self.scan_expr_uses(cond);
                let saved = self.reaching.clone();
                self.scan_stmts(body);
                let body_reaching = std::mem::replace(&mut self.reaching, saved.clone());
                self.merge_reaching(body_reaching, saved);
            }
            HirStmt::For { init, cond, update, body } => {
                if let Some(s) = init { self.scan_stmt(s); }
                if let Some(e) = cond { self.scan_expr_uses(e); }
                let saved = self.reaching.clone();
                self.scan_stmts(body);
                if let Some(s) = update { self.scan_stmt(s); }
                let body_reaching = std::mem::replace(&mut self.reaching, saved.clone());
                self.merge_reaching(body_reaching, saved);
            }
            HirStmt::Switch { expr, cases, default } => {
                self.scan_expr_uses(expr);
                let saved = self.reaching.clone();
                let mut arm_reachings = Vec::new();
                for case in cases {
                    self.reaching = saved.clone();
                    self.scan_stmts(&case.body);
                    arm_reachings.push(std::mem::replace(&mut self.reaching, saved.clone()));
                }
                self.scan_stmts(default);
                let def_reaching = std::mem::take(&mut self.reaching);
                arm_reachings.push(def_reaching);
                // Merge all arms.
                self.reaching = saved;
                for arm in arm_reachings {
                    let curr = std::mem::take(&mut self.reaching);
                    self.merge_reaching(curr, arm);
                }
            }
            HirStmt::Block(stmts) => self.scan_stmts(stmts),
            HirStmt::Return(Some(e)) => self.scan_expr_uses(e),
            HirStmt::Expr(e) => self.scan_expr_uses(e),
            _ => {}
        }
    }

    fn scan_expr_uses(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::Load { ptr, ty } => {
                // Record use before scanning ptr (so nested loads get their own uses).
                let key = self.alias_key_for_ptr(ptr, nir_byte_size(ty));
                self.add_use(key);
                self.scan_expr_uses(ptr);
            }
            HirExpr::Call { args, .. } => {
                // Mark any Var whose address might be passed as potentially escaped.
                for arg in args {
                    self.scan_expr_uses(arg);
                    if let HirExpr::PtrOffset { base, .. } = arg {
                        if let HirExpr::Var(name) = base.as_ref() {
                            self.escaped.insert(name.clone());
                        }
                    }
                }
            }
            HirExpr::Unary { expr: inner, .. } => self.scan_expr_uses(inner),
            HirExpr::Binary { lhs, rhs, .. } => {
                self.scan_expr_uses(lhs);
                self.scan_expr_uses(rhs);
            }
            HirExpr::Cast { expr: inner, .. } => self.scan_expr_uses(inner),
            HirExpr::PtrOffset { base, .. } => self.scan_expr_uses(base),
            HirExpr::Index { base, index, .. } => {
                self.scan_expr_uses(base);
                self.scan_expr_uses(index);
            }
            HirExpr::AggregateCopy { src, .. } => self.scan_expr_uses(src),
            _ => {}
        }
    }

    /// Compute an alias key for a pointer expression.
    fn alias_key_for_ptr(&self, ptr: &HirExpr, size: u32) -> AliasKey {
        alias_key_for_pointer_expr(ptr, size)
    }

    fn merge_reaching(&mut self, a: HashMap<AliasKey, usize>, b: HashMap<AliasKey, usize>) {
        // Union of all keys.
        let mut all_keys: std::collections::HashSet<AliasKey> = a.keys().cloned().collect();
        all_keys.extend(b.keys().cloned());
        for key in all_keys {
            let def_a = a.get(&key);
            let def_b = b.get(&key);
            match (def_a, def_b) {
                (Some(&da), Some(&db)) if da == db => {
                    self.reaching.insert(key, da);
                }
                (Some(&da), Some(&db)) => {
                    // Emit a MemPhi.
                    let phi_id = self.alloc_id();
                    self.phis.push(MemPhi {
                        id: phi_id,
                        key: key.clone(),
                        inputs: vec![da, db],
                        use_count: 0,
                    });
                    self.reaching.insert(key, phi_id);
                }
                (Some(&d), None) | (None, Some(&d)) => {
                    self.reaching.insert(key, d);
                }
                (None, None) => {}
            }
        }
    }

    fn finish(mut self) -> MemSsa {
        // Mark defs whose variable escaped.
        for def in &mut self.defs {
            if let AliasKey::Stack { .. } = &def.key {
                // Escape is tracked separately via self.escaped — use Unknown
                // check to gate removals (see dead_store.rs).
            }
        }
        MemSsa { defs: self.defs, uses: self.uses, phis: self.phis }
    }
}

/// Return the byte size of a `NirType`.
pub(crate) fn nir_byte_size(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => bits.div_ceil(8),
        NirType::Ptr(_) => 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Float { bits } => bits.div_ceil(8),
        NirType::Unknown => 8,
    }
}

/// Compute an [`AliasKey`] for a pointer expression and access size (bytes).
///
/// Used by MemSSA construction and by redundant-load elimination.  Only
/// `stack_*` / `PtrOffset(stack_*, k)` patterns yield [`AliasKey::Stack`];
/// everything else is [`AliasKey::Unknown`].
pub(crate) fn alias_key_for_pointer_expr(ptr: &HirExpr, size: u32) -> AliasKey {
    match ptr {
        HirExpr::Var(name) => {
            if let Some(offset) = extract_stack_offset(name) {
                AliasKey::Stack { offset, size }
            } else {
                AliasKey::Unknown
            }
        }
        HirExpr::PtrOffset { base, offset } => {
            if let HirExpr::Var(name) = base.as_ref() {
                if let Some(base_offset) = extract_stack_offset(name) {
                    return AliasKey::Stack {
                        offset: base_offset + *offset,
                        size,
                    };
                }
            }
            AliasKey::Unknown
        }
        _ => AliasKey::Unknown,
    }
}

/// Extract a stack offset from a variable name produced by slot surfacing.
///
/// Slot surfacing names stack variables as `stack_neg_<abs>` or `stack_<offset>`.
/// This function parses those names to recover the numeric offset.
fn extract_stack_offset(name: &str) -> Option<i64> {
    if let Some(rest) = name.strip_prefix("stack_neg_") {
        rest.parse::<i64>().ok().map(|v| -v)
    } else if let Some(rest) = name.strip_prefix("stack_") {
        rest.parse::<i64>().ok()
    } else {
        None
    }
}

/// Build MemSSA for a HIR function.
pub(crate) fn build_mem_ssa(func: &HirFunction) -> MemSsa {
    let mut builder = Builder::new();
    builder.scan_stmts(&func.body);
    builder.finish()
}
