//! Pseudocode rendering from structured IR (`render_nir`, layout helpers).
//!
//! Downstream of semantics; avoid fixing structuring failures by tweaking format-only paths.
//! Policy: `crates/fission-pcode/src/render/AGENTS.md`, ADR 0011.

mod calls;
mod collect;
mod ctx_expr;
mod ctx_stmt;
mod ops;
mod plain;
#[cfg(test)]
mod tests;
mod types;

use super::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, NirBinding,
    NirBindingOrigin, NirType, PrintProfile, expr_type,
};
use calls::*;
use collect::*;
use ctx_expr::*;
use ctx_stmt::*;
use fission_midend_core::ir::sanitize_c_identifier;
use fission_signatures::is_known_variadic_runtime_symbol;
use ops::*;
use plain::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use types::*;

// Paths outside `printer` name these as `printer::<fn>`; keep them stable.
pub(super) use calls::call_argument_type;
pub(crate) use plain::{print_expr, print_stmt};
pub(crate) use types::{print_type, surface_type_definition_source, type_byte_size};

struct LineTrace<'a> {
    definitions: &'a HashMap<String, u64>,
    counts: HashMap<&'a str, usize>,
    state: RefCell<(usize, usize, Vec<super::RecoveredLineMapping>)>,
}

impl LineTrace<'_> {
    fn definition(&self, name: &str, out: &str) {
        if self.counts.get(name) != Some(&1) {
            return;
        }
        let Some(address) = self.definitions.get(name) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let delta = out[state.0..].bytes().filter(|byte| *byte == b'\n').count();
        state.0 = out.len();
        state.1 += delta;
        let line_number = state.1 + 1;
        state.2.push(super::RecoveredLineMapping {
            line_number,
            addresses: vec![*address],
            defined_variables: vec![name.to_string()],
            call_site: None,
        });
    }
    fn call(&self, expr: &HirExpr, out: &str) {
        fn collect(
            expr: &HirExpr,
            sites: &mut Vec<Option<fission_midend_core::ir::CallOrigin>>,
            depth: usize,
        ) {
            if depth > MAX_PRINT_EXPR_DEPTH {
                sites.push(None);
                return;
            }
            match expr {
                HirExpr::Call { origin, args, .. } => {
                    sites.push(*origin);
                    for arg in args {
                        collect(arg, sites, depth + 1);
                    }
                }
                HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                    collect(expr, sites, depth + 1)
                }
                HirExpr::Load { ptr, .. } => collect(ptr, sites, depth + 1),
                HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
                    collect(base, sites, depth + 1)
                }
                HirExpr::Binary { lhs, rhs, .. } => {
                    collect(lhs, sites, depth + 1);
                    collect(rhs, sites, depth + 1);
                }
                HirExpr::Index { base, index, .. } => {
                    collect(base, sites, depth + 1);
                    collect(index, sites, depth + 1);
                }
                HirExpr::Select {
                    cond,
                    then_expr,
                    else_expr,
                    ..
                } => {
                    collect(cond, sites, depth + 1);
                    collect(then_expr, sites, depth + 1);
                    collect(else_expr, sites, depth + 1);
                }
                HirExpr::AggregateCopy { src, .. } => collect(src, sites, depth + 1),
                _ => {}
            }
        }
        let mut sites = Vec::new();
        collect(expr, &mut sites, 0);
        let [Some(site)] = sites.as_slice() else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let delta = out[state.0..].bytes().filter(|b| *b == b'\n').count();
        state.0 = out.len();
        state.1 += delta;
        let line_number = state.1 + 1;
        state.2.push(super::RecoveredLineMapping {
            line_number,
            addresses: vec![site.instruction_address],
            defined_variables: Vec::new(),
            call_site: Some(*site),
        });
    }
}

const MAX_PRINT_STMT_DEPTH: usize = 512;
const MAX_PRINT_EXPR_DEPTH: usize = 512;

/// Printing context: maps variable name → NirType for struct-member rendering.
struct PrintCtx<'a> {
    line_trace: Option<&'a LineTrace<'a>>,
    /// variable name → declared type
    var_types: HashMap<&'a str, &'a NirType>,
    pointer_decl_names: HashSet<&'a str>,
    /// The type text each binding is actually declared with, which is not
    /// always `print_type(&b.ty)` -- a `surface_type_name` wins there. Casts
    /// back to a variable's own type have to spell the declared one or they
    /// are a different type.
    decl_type_text: HashMap<&'a str, String>,
    return_type: &'a NirType,
    inline_guard_goto: bool,
    global_names: Option<&'a HashMap<u64, String>>,
    /// Standalone print helpers accept raw HIR symbols. The decompiler's
    /// layered path maps symbols when building HIR and must not encode them a
    /// second time.
    sanitize_symbols: bool,
    profile: PrintProfile,
    /// Variable -> Win32 enum group its value came from, for variables that
    /// hold a known API's return value. Lets a later comparison against a
    /// literal name that literal.
    return_enum_groups: HashMap<String, String>,
    /// Every label referenced by a `Goto` anywhere in the function body.
    /// A `switch` case/default's leading `Label` must be preserved (rendered
    /// as a C label alongside the `case`) when it appears here, since `case`
    /// values and identifier labels are different namespaces in C -- a
    /// `goto` cannot target a bare `case N:`. See ADR/#149.
    goto_targets: HashSet<&'a str>,
}

impl<'a> PrintCtx<'a> {
    /// Name for `constant` when `carrier` holds a known API's return value and
    /// the constant resolves in the group that return carries.
    fn name_compared_constant(&self, carrier: &HirExpr, constant: &HirExpr) -> Option<String> {
        let HirExpr::Var(name) = carrier else {
            return None;
        };
        let HirExpr::Const(value, _) = constant else {
            return None;
        };
        let group = self.return_enum_groups.get(name.as_str())?;
        fission_signatures::WIN_CONSTANTS_DB.resolve_in(group, *value as u64)
    }

    fn build(func: &'a HirFunction) -> Self {
        Self::build_with_profile(func, PrintProfile::Nir)
    }

    fn build_with_profile(func: &'a HirFunction, profile: PrintProfile) -> Self {
        let mut var_types = HashMap::new();
        let mut pointer_decl_names = HashSet::new();
        let mut decl_type_text = HashMap::new();
        for b in func.locals.iter().chain(func.params.iter()) {
            var_types.insert(b.name.as_str(), &b.ty);
            decl_type_text.insert(b.name.as_str(), print_binding_type(b));
            if matches!(b.ty, NirType::Ptr(_))
                || b.surface_type_name
                    .as_deref()
                    .is_some_and(|surface| surface.contains('*'))
            {
                pointer_decl_names.insert(b.name.as_str());
            }
        }
        let mut goto_targets = HashSet::new();
        collect_goto_targets(&func.body, &mut goto_targets);
        let mut return_enum_groups = HashMap::new();
        collect_return_enum_groups(func, &mut return_enum_groups);
        Self {
            line_trace: None,
            var_types,
            pointer_decl_names,
            decl_type_text,
            return_enum_groups,
            return_type: &func.return_type,
            inline_guard_goto: func.body.len() <= 6,
            global_names: None,
            sanitize_symbols: false,
            profile,
            goto_targets,
        }
    }

    fn constant_name(&self, value: i64) -> Option<&str> {
        self.global_names?.get(&(value as u64)).map(String::as_str)
    }

    fn expr_is_pointer(&self, expr: &HirExpr) -> bool {
        match expr {
            HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) => true,
            HirExpr::Var(name) => self
                .var_types
                .get(name.as_str())
                .is_some_and(|ty| matches!(ty, NirType::Ptr(_))),
            HirExpr::Cast {
                ty: NirType::Ptr(_),
                ..
            } => true,
            HirExpr::PtrOffset { .. } => true,
            HirExpr::Index {
                elem_ty: NirType::Ptr(_),
                ..
            } => true,
            _ => false,
        }
    }

    /// Whether this operand *prints* as a pointer, for deciding what C will
    /// accept as an operand.
    ///
    /// Wider than [`Self::expr_is_pointer`] on purpose: a binding whose
    /// `surface_type_name` spells a pointer is declared as one in the emitted
    /// text even when its `ty` is not `Ptr`, and it is the emitted text gcc
    /// has to accept. Kept separate so the deref/index printing decisions that
    /// read `expr_is_pointer` keep the narrower meaning they were written for.
    fn operand_prints_as_pointer(&self, expr: &HirExpr) -> bool {
        self.operand_prints_as_pointer_at(expr, 0)
    }

    fn operand_prints_as_pointer_at(&self, expr: &HirExpr, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match expr {
            HirExpr::Var(name) => {
                self.pointer_decl_names.contains(name.as_str()) || self.expr_is_pointer(expr)
            }
            // Pointer arithmetic yields a pointer, so the result is an operand
            // of the next operator up. `local_8 - 1 + rax` is pointer plus
            // pointer even though neither `-` nor its operands say so on their
            // own.
            HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs,
                rhs,
                ..
            } => {
                self.operand_prints_as_pointer_at(lhs, depth + 1)
                    || self.operand_prints_as_pointer_at(rhs, depth + 1)
            }
            HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs,
                rhs,
                ..
            } => {
                self.operand_prints_as_pointer_at(lhs, depth + 1)
                    && !self.operand_prints_as_pointer_at(rhs, depth + 1)
            }
            HirExpr::Load { ty, .. } | HirExpr::Index { elem_ty: ty, .. } => {
                matches!(ty, NirType::Ptr(_))
            }
            HirExpr::PtrOffset { .. } => true,
            _ => self.expr_is_pointer(expr),
        }
    }

    fn expr_has_pointer_like_result(&self, expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs,
                rhs,
                ..
            } => self.expr_is_pointer(lhs) || self.expr_is_pointer(rhs),
            HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs,
                rhs,
                ..
            } => self.expr_is_pointer(lhs) && !self.expr_is_pointer(rhs),
            HirExpr::PtrOffset { .. } => true,
            HirExpr::Index {
                elem_ty: NirType::Ptr(_),
                ..
            } => true,
            other => self.expr_is_pointer(other),
        }
    }

    /// Whether `name` is declared `void *`, which cannot be dereferenced.
    ///
    /// `void` is not a type a value can have, so `*p` on one is an error
    /// rather than a warning -- the deref has to name the type it is reading.
    fn declared_pointee_is_void(&self, name: &str) -> bool {
        self.decl_type_text.get(name).is_some_and(|text| {
            let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            compact == "void*"
        })
    }

    fn simple_deref_target_is_declared_pointer(&self, expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(name) => {
                self.pointer_decl_names.contains(name.as_str())
                    && !self.declared_pointee_is_void(name.as_str())
            }
            HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) => true,
            HirExpr::Cast {
                ty: NirType::Ptr(_),
                expr,
            } => self.simple_deref_target_is_declared_pointer(expr),
            HirExpr::PtrOffset { base, offset } if *offset == 0 => {
                self.simple_deref_target_is_declared_pointer(base)
            }
            HirExpr::AggregateCopy { src, .. } => self.simple_deref_target_is_declared_pointer(src),
            _ => false,
        }
    }

    /// Whether `name`'s declaration cannot bear an access of `access_ty`.
    ///
    /// The bare `*p` and `p[i]` spellings only work when the declared type
    /// already says what the access needs; otherwise the deref has to name the
    /// type it is reading. The disagreement that matters is about
    /// aggregate-ness, not about spelling: `LPRECT` and `fission_agg16 *` are
    /// both pointers to a 16-byte aggregate, and treating the different text
    /// as a mismatch strips a recovered type off every Windows typedef.
    ///
    /// A binding declared through a `surface_type_name` is therefore trusted:
    /// `ty` is not the authority on what that name means, so it cannot be the
    /// evidence that the name is wrong.
    fn direct_var_has_aggregate_access_mismatch(&self, name: &str, access_ty: &NirType) -> bool {
        let Some(NirType::Ptr(pointee)) = self.var_types.get(name).copied() else {
            return false;
        };
        let compact =
            |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
        let declared_by_surface = self.decl_type_text.get(name).is_some_and(|declared| {
            compact(declared) != compact(&print_type(&NirType::Ptr(pointee.clone())))
        });
        if declared_by_surface {
            return false;
        }
        matches!(pointee.as_ref(), NirType::Aggregate { .. })
            != matches!(access_ty, NirType::Aggregate { .. })
    }

    /// Whether a scalar access is wider or narrower than the pointee named by
    /// the declaration. A surface type can intentionally be narrower than the
    /// observed internal pointer value: a packed machine load through an
    /// `int *` cursor still needs an explicit 64-bit C load to preserve the
    /// observed p-code width.
    fn direct_var_has_scalar_access_width_mismatch(&self, name: &str, access_ty: &NirType) -> bool {
        if !self.pointer_decl_names.contains(name) {
            return false;
        }
        let Some(declared_size) = self
            .decl_type_text
            .get(name)
            .and_then(|text| surface_pointee_byte_size(text))
        else {
            return false;
        };
        let Some(access_size) = type_byte_size(access_ty) else {
            return false;
        };
        declared_size != access_size
    }

    /// Whether a pointer RHS carries a machine-side pointee width that differs
    /// from the declaration selected for `name`. This keeps an internal cast
    /// such as `(unsigned long long *)...` from becoming an incompatible
    /// assignment to a recovered `int *` alias.
    fn pointer_assignment_needs_decl_cast(&self, name: &str, rhs: &HirExpr) -> bool {
        let Some(declared_size) = self
            .decl_type_text
            .get(name)
            .and_then(|text| surface_pointee_byte_size(text))
        else {
            return false;
        };
        if let HirExpr::Var(source) = rhs
            && self.decl_type_text.get(source.as_str()) == self.decl_type_text.get(name)
        {
            return false;
        }
        let rhs_size = self.pointer_expr_pointee_byte_size(rhs);
        rhs_size.is_some_and(|rhs_size| rhs_size != declared_size)
    }

    fn pointer_expr_pointee_byte_size(&self, expr: &HirExpr) -> Option<u32> {
        match expr {
            HirExpr::Var(name) => self.var_types.get(name.as_str()).and_then(|ty| {
                let NirType::Ptr(pointee) = ty else {
                    return None;
                };
                type_byte_size(pointee)
            }),
            HirExpr::Cast {
                ty: NirType::Ptr(pointee),
                ..
            }
            | HirExpr::Binary {
                ty: NirType::Ptr(pointee),
                ..
            }
            | HirExpr::Load {
                ty: NirType::Ptr(pointee),
                ..
            }
            | HirExpr::Index {
                elem_ty: NirType::Ptr(pointee),
                ..
            } => type_byte_size(pointee),
            HirExpr::PtrOffset { base, .. } => self.pointer_expr_pointee_byte_size(base),
            _ => None,
        }
    }

    fn simple_deref_target_matches_access(&self, expr: &HirExpr, access_ty: &NirType) -> bool {
        self.simple_deref_target_is_declared_pointer(expr)
            && peel_simple_deref_target(expr).is_some_and(|name| {
                !self
                    .decl_type_text
                    .get(name)
                    .is_some_and(|text| surface_array_declarator(Some(text)).is_some())
                    && !self.direct_var_has_aggregate_access_mismatch(name, access_ty)
                    && !self.direct_var_has_scalar_access_width_mismatch(name, access_ty)
            })
    }
}

pub(crate) fn print_hir_function(func: &HirFunction) -> String {
    let mut ctx = PrintCtx::build_with_profile(func, PrintProfile::Nir);
    ctx.sanitize_symbols = true;
    print_hir_function_impl(func, ctx)
}

pub(crate) fn print_hir_function_with_global_names(
    func: &HirFunction,
    global_names: &HashMap<u64, String>,
) -> String {
    print_hir_function_with_profile(func, Some(global_names), PrintProfile::Nir)
}

pub(crate) fn print_hir_function_with_profile(
    func: &HirFunction,
    global_names: Option<&HashMap<u64, String>>,
    profile: PrintProfile,
) -> String {
    let mut ctx = PrintCtx::build_with_profile(func, profile);
    ctx.global_names = global_names;
    print_hir_function_impl(func, ctx)
}

pub(crate) fn print_hir_function_with_provenance(
    func: &HirFunction,
    global_names: Option<&HashMap<u64, String>>,
    profile: PrintProfile,
    definitions: &HashMap<String, u64>,
) -> (String, Vec<super::RecoveredLineMapping>) {
    let mut counts = HashMap::new();
    count_var_assignments(&func.body, &mut counts);
    for binding in &func.locals {
        if binding.initializer.is_some() {
            *counts.entry(binding.name.as_str()).or_default() += 1;
        }
    }
    counts.retain(|name, _| func.locals.iter().any(|binding| binding.name == *name));
    let trace = LineTrace {
        definitions,
        counts,
        state: RefCell::new((0, 0, Vec::new())),
    };
    let mut ctx = PrintCtx::build_with_profile(func, profile);
    ctx.global_names = global_names;
    ctx.line_trace = Some(&trace);
    let code = print_hir_function_impl(func, ctx);
    let (_, _, mappings) = trace.state.into_inner();
    (code, mappings)
}

/// Whether the callable can be declared but its forwarding body cannot be
/// defined because its parameters are unknown.
pub(super) fn has_unknown_arity_declaration(func: &HirFunction) -> bool {
    let variadic_fixed_arity = func
        .variadic_fixed_arity
        .or_else(|| is_known_variadic_runtime_symbol(&func.name).then_some(func.params.len()));
    func.params.is_empty() && (func.has_unknown_arity_prototype || variadic_fixed_arity == Some(0))
}

/// The comment that marks a callee prototype whose parameter list was read off
/// this function's call sites rather than declared anywhere.
///
/// It is part of the output contract: a consumer that merges several
/// functions' declarations (the CLI's `--project` unit) has to know that two
/// different inferred lists for one callee are two partial views, not a
/// declaration to keep and a mistake to drop.
pub const INFERRED_PROTOTYPE_MARKER: &str = "/* inferred from calls */";

/// `extern T name(params);` for a parameter list inferred from call sites.
/// An empty list is a prototype with no parameters, `(void)`.
pub fn print_inferred_extern(return_type: &str, name: &str, params: &[String]) -> String {
    let params = if params.is_empty() {
        "void".to_string()
    } else {
        params.join(", ")
    };
    format!("extern {return_type} {name}({params}); {INFERRED_PROTOTYPE_MARKER}\n")
}

/// The declaration of a callable whose parameters are unknown, valid in both
/// C17 and C23. It is a preprocessor group, not one line: a consumer that
/// takes declarations apart has to keep `#if` .. `#endif` together.
pub fn print_unknown_arity_extern(return_type: &str, name: &str) -> String {
    // C23 gives an empty parameter list the meaning of (void) and allows
    // ellipsis without a fixed parameter. GCC 14 implements both rules in
    // its C23 mode, which still uses the draft __STDC_VERSION__ of 202000L.
    // C11 and C17 use the empty list for this.
    format!(
        "/* inferred return type; parameters unknown */\n\
         #if defined(__STDC_VERSION__) && __STDC_VERSION__ > 201710L\n\
         extern {return_type} {name}(...);\n\
         #else\n\
         extern {return_type} {name}();\n\
         #endif\n"
    )
}

fn print_hir_function_impl(func: &HirFunction, ctx: PrintCtx<'_>) -> String {
    let return_type = func
        .surface_return_type_name
        .clone()
        .unwrap_or_else(|| print_return_type(&func.return_type));
    let function_name = if ctx.sanitize_symbols {
        sanitize_c_identifier(&func.name)
    } else {
        func.name.clone()
    };
    if has_unknown_arity_declaration(func) {
        return print_unknown_arity_extern(&return_type, &function_name);
    }

    let mut out = String::new();
    out.push_str(&format!("{return_type} {function_name}("));
    let variadic_fixed_arity = func
        .variadic_fixed_arity
        .or_else(|| is_known_variadic_runtime_symbol(&func.name).then_some(func.params.len()));
    if func.params.is_empty() {
        out.push_str("void");
    } else {
        let fixed_arity = variadic_fixed_arity.unwrap_or(func.params.len());
        for (idx, param) in func.params.iter().take(fixed_arity).enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(&print_binding_declaration(param));
        }
        if variadic_fixed_arity.is_some() {
            out.push_str(if fixed_arity.min(func.params.len()) > 0 {
                ", ..."
            } else {
                "..."
            });
        }
    }
    out.push_str(")\n{\n");
    for local in &func.locals {
        if let Some(initializer) = &local.initializer {
            if let Some(trace) = ctx.line_trace {
                trace.definition(&local.name, &out);
            }
            out.push_str(&format!(
                "    {} = {};\n",
                print_binding_declaration(local),
                print_expr_with_ctx(initializer, &ctx)
            ));
        } else {
            out.push_str(&format!("    {};\n", print_binding_declaration(local)));
        }
    }
    if !func.locals.is_empty() {
        out.push('\n');
    }
    for stmt in &func.body {
        print_stmt_with_indent_ctx(stmt, 1, 0, &ctx, &mut out);
    }
    out.push_str("}\n");
    out
}

pub fn render_contracted_wrapper_summary(
    name: &str,
    summary: &crate::midend::ProcedureSummary,
) -> String {
    let target = summary
        .wrapper_contraction
        .as_ref()
        .map(|proof| proof.target.symbol.clone())
        .unwrap_or_else(|| "unknown_target".to_string());
    let mut hir = HirFunction {
        name: name.to_string(),
        int_param_offsets: Vec::new(),
        return_type: NirType::Unknown,
        ..HirFunction::default()
    };
    hir.body = vec![HirStmt::Return(Some(HirExpr::Call {
        origin: None,
        target,
        args: Vec::new(),
        ty: NirType::Unknown,
    }))];
    print_hir_function(&hir)
}
