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
use super::super::memory::{PartitionKey, partition_key_for_pointer_expr};
use crate::prelude::*;
use crate::HashMap;

/// Identifies a memory location for alias analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AliasKey {
    /// A partitioned alias class rooted in canonical memory partitioning.
    Partition(PartitionKey),
    /// Any non-stack / unknown pointer — conservative may-alias bucket.
    Unknown,
}

impl AliasKey {
    /// Return `true` if two keys definitely refer to the same location.
    pub fn is_must_alias(&self, other: &AliasKey) -> bool {
        match (self, other) {
            (AliasKey::Partition(a), AliasKey::Partition(b)) => {
                a.base_object == b.base_object && a.offset_interval == b.offset_interval
            }
            _ => false,
        }
    }

    /// Return `true` if two keys definitely do NOT alias.
    pub fn is_no_alias(&self, other: &AliasKey) -> bool {
        match (self, other) {
            (AliasKey::Partition(a), AliasKey::Partition(b)) if a.base_object == b.base_object => {
                // Non-overlapping intervals.
                a.offset_interval.1 <= b.offset_interval.0
                    || b.offset_interval.1 <= a.offset_interval.0
            }
            (AliasKey::Unknown, _) | (_, AliasKey::Unknown) => false,
            _ => false,
        }
    }
}

/// A memory definition (store operation).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemDef {
    pub id: usize,
    pub key: AliasKey,
    /// How many `MemUse` nodes reference this def.
    pub use_count: usize,
    /// True if any address of this slot escapes to a `Call` argument.
    pub may_escape: bool,
}

/// A memory use (load operation).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemUse {
    pub id: usize,
    pub key: AliasKey,
    pub reaching_def: Option<usize>,
}

/// A virtual phi node at a CFG join point.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemPhi {
    pub id: usize,
    pub key: AliasKey,
    pub inputs: Vec<usize>, // MemDef/MemPhi ids
    pub use_count: usize,
}

/// Complete MemSSA for a function.
pub struct MemSsa {
    pub defs: Vec<MemDef>,
    pub uses: Vec<MemUse>,
    pub phis: Vec<MemPhi>,
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
            reaching: HashMap::default(),
            next_id: 0,
            escaped: std::collections::HashSet::default(),
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
        self.defs.push(MemDef {
            id,
            key,
            use_count: 0,
            may_escape,
        });
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
        self.uses.push(MemUse {
            id,
            key,
            reaching_def,
        });
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
                    HirLValue::Index {
                        base,
                        index: _,
                        elem_ty,
                    } => {
                        let key = self.alias_key_for_ptr(base, nir_byte_size(elem_ty));
                        self.add_def(key);
                    }
                    HirLValue::Var(_) => {} // Not a memory write.
                    HirLValue::FieldAccess { base, ty, .. } => {
                        let key = self.alias_key_for_ptr(base, nir_byte_size(ty));
                        self.add_def(key);
                    }
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
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
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.scan_stmt(s);
                }
                if let Some(e) = cond {
                    self.scan_expr_uses(e);
                }
                let saved = self.reaching.clone();
                self.scan_stmts(body);
                if let Some(s) = update {
                    self.scan_stmt(s);
                }
                let body_reaching = std::mem::replace(&mut self.reaching, saved.clone());
                self.merge_reaching(body_reaching, saved);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
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
            HirExpr::FieldAccess { base, .. } => self.scan_expr_uses(base),
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
            if let AliasKey::Partition(key) = &def.key {
                def.may_escape = !key.is_promotable_stack_like();
            }
        }
        MemSsa {
            defs: self.defs,
            uses: self.uses,
            phis: self.phis,
        }
    }
}

/// Return the byte size of a `NirType`.
pub fn nir_byte_size(ty: &NirType) -> u32 {
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
/// Used by MemSSA construction and by redundant-load elimination. Precision
/// comes from the canonical partition collector; everything else is conservatively
/// collapsed to [`AliasKey::Unknown`].
pub fn alias_key_for_pointer_expr(ptr: &HirExpr, size: u32) -> AliasKey {
    let access_ty = NirType::Aggregate {
        size,
        fields: vec![],
    };
    partition_key_for_pointer_expr(ptr, &access_ty)
        .map(AliasKey::Partition)
        .unwrap_or(AliasKey::Unknown)
}

/// Build MemSSA for a HIR function.
pub fn build_mem_ssa(func: &HirFunction) -> MemSsa {
    let mut builder = Builder::new();
    builder.scan_stmts(&func.body);
    builder.finish()
}
