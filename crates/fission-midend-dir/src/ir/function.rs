use fission_core::CallingConvention;
use fission_midend_core::ir::{CallSummary, HirFunction, HirStmt, NirBinding, NirBindingOrigin, NirType};
use indexmap::IndexMap;

use crate::ir::{DirExpr, DirStmt, dir_expr_to_hir_expr};

/// The DIR-side counterpart to [`NirBinding`] -- identical except
/// `initializer`, which embeds a real AST expression and therefore must be
/// `DirExpr`-typed on the pre-structuring side. Everything else
/// (`name`/`ty`/`surface_type_name`/`origin`) is genuinely shared,
/// type-agnostic metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirBinding {
    pub name: String,
    pub ty: NirType,
    pub surface_type_name: Option<String>,
    pub origin: Option<NirBindingOrigin>,
    pub initializer: Option<DirExpr>,
}

impl DirBinding {
    pub fn is_temp_like(&self) -> bool {
        self.origin.is_some_and(NirBindingOrigin::is_temp_like)
    }

    pub fn preserves_materialization(&self) -> bool {
        self.origin
            .is_some_and(NirBindingOrigin::preserves_materialization)
    }
}

/// The function-level container `fission-pcode`'s builder produces directly
/// from p-code, and that normalize/structuring's own internal passes read
/// and mutate `body` on (via `&mut` in place, across many incremental
/// passes) until structuring's CFG-to-AST rewrite is done. Field-for-field
/// identical to [`HirFunction`] except `body`'s statement grammar -- see
/// this module's doc comment on [`HirFunction`] for the rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirFunction {
    pub name: String,
    pub params: Vec<DirBinding>,
    pub locals: Vec<DirBinding>,
    pub return_type: NirType,
    pub surface_return_type_name: Option<String>,
    pub body: Vec<DirStmt>,
    pub calling_convention: CallingConvention,
    pub int_param_offsets: Vec<u64>,
    pub is_64bit: bool,
    pub suppress_entry_register_params: bool,
    pub callee_observed_max_arity: IndexMap<String, usize>,
    pub callee_summaries: IndexMap<String, CallSummary>,
}

impl Default for DirFunction {
    fn default() -> Self {
        Self {
            name: String::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: Vec::new(),
            calling_convention: CallingConvention::default(),
            int_param_offsets: Vec::new(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: IndexMap::new(),
            callee_summaries: IndexMap::new(),
        }
    }
}

impl DirFunction {
    /// Convert to the final [`HirFunction`] via structuring's real
    /// `DirFunction -> HirFunction` boundary (`dir_stmts_to_hir_stmts`,
    /// `dir_binding_to_nir_binding`) -- ABI metadata carries over unchanged;
    /// `body`/`params`/`locals` are actually converted (the latter two only
    /// need converting because `DirBinding::initializer` is `DirExpr`-typed).
    pub fn into_hir_function(self, body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: self.name,
            params: self
                .params
                .into_iter()
                .map(dir_binding_to_nir_binding)
                .collect(),
            locals: self
                .locals
                .into_iter()
                .map(dir_binding_to_nir_binding)
                .collect(),
            return_type: self.return_type,
            surface_return_type_name: self.surface_return_type_name,
            body,
            calling_convention: self.calling_convention,
            int_param_offsets: self.int_param_offsets,
            is_64bit: self.is_64bit,
            suppress_entry_register_params: self.suppress_entry_register_params,
            callee_observed_max_arity: self.callee_observed_max_arity,
            callee_summaries: self.callee_summaries,
        }
    }
}

fn dir_binding_to_nir_binding(binding: DirBinding) -> NirBinding {
    NirBinding {
        name: binding.name,
        ty: binding.ty,
        surface_type_name: binding.surface_type_name,
        origin: binding.origin,
        initializer: binding.initializer.map(dir_expr_to_hir_expr),
    }
}
