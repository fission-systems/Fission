//! Structural invariant validator for PreHIR functions and statement trees,
//! projecting into the shared [`StructuralValidationReport`] contract.

use super::{PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};
use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
use fission_midend_core::ir::{
    IrInvariant, IrInvariantSet, IrStage, StructuralValidationReport, StructuralViolation,
    ValidationSite, validate_callee_summaries, validate_nir_type,
};
use std::collections::BTreeSet;

/// Check mechanically verifiable structural invariants on a [`PreHirFunction`].
pub fn validate_prehir_function(
    func: &PreHirFunction,
    invariants: IrInvariantSet,
    pass: &str,
    revision: u64,
) -> Result<(), StructuralValidationReport> {
    let mut violations = Vec::new();
    let mut declared = BTreeSet::new();

    if invariants.contains(IrInvariant::BindingScope) {
        let mut seen_params = BTreeSet::new();
        for param in &func.params {
            if param.name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::PreHir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    ValidationSite::Function {
                        name: func.name.clone(),
                    },
                    "parameter binding has empty name",
                    &format!("fn={} params={}", func.name, func.params.len()),
                ));
            } else {
                if !seen_params.insert(param.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::PreHir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: param.name.clone(),
                        },
                        format!("duplicate parameter binding '{}'", param.name),
                        &format!("fn={} duplicate param {}", func.name, param.name),
                    ));
                }
                declared.insert(param.name.as_str());
            }
        }
        let mut seen_locals = BTreeSet::new();
        for local in &func.locals {
            if local.name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::PreHir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    ValidationSite::Function {
                        name: func.name.clone(),
                    },
                    "local binding has empty name",
                    &format!("fn={} locals={}", func.name, func.locals.len()),
                ));
            } else {
                if seen_params.contains(local.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::PreHir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: local.name.clone(),
                        },
                        format!("local binding '{}' shadows parameter", local.name),
                        &format!("fn={} shadow {}", func.name, local.name),
                    ));
                }
                if !seen_locals.insert(local.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::PreHir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: local.name.clone(),
                        },
                        format!("duplicate local binding '{}'", local.name),
                        &format!("fn={} duplicate local {}", func.name, local.name),
                    ));
                }
                declared.insert(local.name.as_str());
            }
        }
    }

    if invariants.contains(IrInvariant::EffectRepresentation) {
        let fn_site = ValidationSite::Function {
            name: func.name.clone(),
        };
        validate_nir_type(
            &func.return_type,
            IrStage::PreHir,
            pass,
            revision,
            &fn_site,
            &mut violations,
        );
        for param in &func.params {
            let site = ValidationSite::Binding {
                name: param.name.clone(),
            };
            validate_nir_type(
                &param.ty,
                IrStage::PreHir,
                pass,
                revision,
                &site,
                &mut violations,
            );
        }
        for local in &func.locals {
            let site = ValidationSite::Binding {
                name: local.name.clone(),
            };
            validate_nir_type(
                &local.ty,
                IrStage::PreHir,
                pass,
                revision,
                &site,
                &mut violations,
            );
        }
        validate_callee_summaries(
            &func.callee_summaries,
            IrStage::PreHir,
            pass,
            revision,
            &mut violations,
        );
    }

    let declared_ref = invariants
        .contains(IrInvariant::BindingScope)
        .then_some(&declared);
    if let Err(mut body_report) = validate_prehir_stmts(
        &func.body,
        invariants,
        declared_ref,
        IrStage::PreHir,
        pass,
        revision,
    ) {
        violations.append(&mut body_report.violations);
    }

    StructuralValidationReport::new(IrStage::PreHir, pass, revision, invariants, violations)
        .into_result()
}

/// Check mechanically verifiable structural invariants on a slice of [`PreHirStmt`].
pub fn validate_prehir_stmts(
    stmts: &[PreHirStmt],
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    stage: IrStage,
    pass: &str,
    revision: u64,
) -> Result<(), StructuralValidationReport> {
    let mut violations = Vec::new();
    let mut defined_labels = BTreeSet::new();
    if invariants.contains(IrInvariant::LabelGotoResolution) {
        let mut path = Vec::new();
        collect_prehir_labels(
            stmts,
            &mut path,
            &mut defined_labels,
            stage,
            pass,
            revision,
            &mut violations,
        );
    }

    let mut path = Vec::new();
    validate_prehir_stmt_list(
        stmts,
        &mut path,
        0,
        0,
        invariants,
        declared_bindings,
        &defined_labels,
        stage,
        pass,
        revision,
        &mut violations,
    );

    StructuralValidationReport::new(stage, pass, revision, invariants, violations).into_result()
}

fn collect_prehir_labels<'a>(
    stmts: &'a [PreHirStmt],
    path: &mut Vec<u32>,
    defined_labels: &mut BTreeSet<&'a str>,
    stage: IrStage,
    pass: &str,
    revision: u64,
    violations: &mut Vec<StructuralViolation>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        path.push(idx as u32);
        match stmt {
            PreHirStmt::Label(label) => {
                if label.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::LabelGotoResolution,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Label",
                        },
                        "empty label name",
                        "Label(\"\")",
                    ));
                } else if !defined_labels.insert(label.as_str()) {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::LabelGotoResolution,
                        ValidationSite::Label {
                            name: label.clone(),
                        },
                        format!("duplicate label definition '{label}'"),
                        &format!("duplicate label {label}"),
                    ));
                }
            }
            PreHirStmt::Block(body) => {
                collect_prehir_labels(
                    body,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_prehir_labels(
                        &case.body,
                        path,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                collect_prehir_labels(
                    default,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_prehir_labels(
                    then_body,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
                collect_prehir_labels(
                    else_body,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_prehir_labels(
                    body,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_prehir_labels(
                        std::slice::from_ref(init.as_ref()),
                        path,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                if let Some(update) = update {
                    collect_prehir_labels(
                        std::slice::from_ref(update.as_ref()),
                        path,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                collect_prehir_labels(
                    body,
                    path,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
        path.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_prehir_stmt_list(
    stmts: &[PreHirStmt],
    path: &mut Vec<u32>,
    loop_depth: usize,
    switch_depth: usize,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    defined_labels: &BTreeSet<&str>,
    stage: IrStage,
    pass: &str,
    revision: u64,
    violations: &mut Vec<StructuralViolation>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        path.push(idx as u32);
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Assign",
                };
                validate_prehir_lvalue(
                    lhs,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_prehir_expr(
                    rhs,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            PreHirStmt::Expr(expr) => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Expr",
                };
                validate_prehir_expr(
                    expr,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            PreHirStmt::VaStart {
                va_list,
                last_named_param,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "VaStart",
                };
                if invariants.contains(IrInvariant::BindingScope) && last_named_param.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        site.clone(),
                        "va_start has empty last_named_param",
                        "VaStart { last_named_param: \"\" }",
                    ));
                }
                validate_prehir_expr(
                    va_list,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            PreHirStmt::Block(body) => {
                validate_prehir_stmt_list(
                    body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Switch",
                };
                validate_prehir_expr(
                    expr,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                let mut seen_case_values = BTreeSet::new();
                for (case_idx, case) in cases.iter().enumerate() {
                    if invariants.contains(IrInvariant::StructuredControlExits) {
                        if case.values.is_empty() {
                            violations.push(StructuralViolation::new(
                                stage,
                                pass,
                                revision,
                                IrInvariant::StructuredControlExits,
                                site.clone(),
                                format!("switch case #{case_idx} has no discriminator values"),
                                &format!("Switch case[{case_idx}].values = []"),
                            ));
                        }
                        for &val in &case.values {
                            if !seen_case_values.insert(val) {
                                violations.push(StructuralViolation::new(
                                    stage,
                                    pass,
                                    revision,
                                    IrInvariant::StructuredControlExits,
                                    site.clone(),
                                    format!("duplicate switch case value {val}"),
                                    &format!("Switch duplicate case {val}"),
                                ));
                            }
                        }
                    }
                    validate_prehir_stmt_list(
                        &case.body,
                        path,
                        loop_depth,
                        switch_depth + 1,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                validate_prehir_stmt_list(
                    default,
                    path,
                    loop_depth,
                    switch_depth + 1,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "If",
                };
                validate_prehir_expr(
                    cond,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_prehir_stmt_list(
                    then_body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
                validate_prehir_stmt_list(
                    else_body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Loop",
                };
                validate_prehir_expr(
                    cond,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_prehir_stmt_list(
                    body,
                    path,
                    loop_depth + 1,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "For",
                };
                if let Some(init) = init {
                    validate_prehir_stmt_list(
                        std::slice::from_ref(init.as_ref()),
                        path,
                        loop_depth,
                        switch_depth,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                if let Some(cond) = cond {
                    validate_prehir_expr(
                        cond,
                        invariants,
                        declared_bindings,
                        stage,
                        pass,
                        revision,
                        &site,
                        violations,
                    );
                }
                if let Some(update) = update {
                    validate_prehir_stmt_list(
                        std::slice::from_ref(update.as_ref()),
                        path,
                        loop_depth,
                        switch_depth,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        stage,
                        pass,
                        revision,
                        violations,
                    );
                }
                validate_prehir_stmt_list(
                    body,
                    path,
                    loop_depth + 1,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    stage,
                    pass,
                    revision,
                    violations,
                );
            }
            PreHirStmt::Label(_) => {}
            PreHirStmt::Goto(target) => {
                if invariants.contains(IrInvariant::LabelGotoResolution) {
                    if target.is_empty() {
                        violations.push(StructuralViolation::new(
                            stage,
                            pass,
                            revision,
                            IrInvariant::LabelGotoResolution,
                            ValidationSite::StmtPath {
                                path: path.clone(),
                                kind: "Goto",
                            },
                            "goto has empty target label",
                            "Goto(\"\")",
                        ));
                    } else if target == SWITCH_FALLTHROUGH_SENTINEL {
                        if switch_depth == 0 {
                            violations.push(StructuralViolation::new(
                                stage,
                                pass,
                                revision,
                                IrInvariant::LabelGotoResolution,
                                ValidationSite::Label {
                                    name: target.clone(),
                                },
                                "switch fallthrough sentinel outside switch",
                                "Goto(__fallthrough) at switch_depth=0",
                            ));
                        }
                    } else if !defined_labels.contains(target.as_str()) {
                        violations.push(StructuralViolation::new(
                            stage,
                            pass,
                            revision,
                            IrInvariant::LabelGotoResolution,
                            ValidationSite::Label {
                                name: target.clone(),
                            },
                            format!("goto references undefined label '{target}'"),
                            &format!("Goto({target})"),
                        ));
                    }
                }
            }
            PreHirStmt::Return(expr) => {
                if let Some(expr) = expr {
                    let site = ValidationSite::StmtPath {
                        path: path.clone(),
                        kind: "Return",
                    };
                    validate_prehir_expr(
                        expr,
                        invariants,
                        declared_bindings,
                        stage,
                        pass,
                        revision,
                        &site,
                        violations,
                    );
                }
            }
            PreHirStmt::Break => {
                if invariants.contains(IrInvariant::StructuredControlExits)
                    && loop_depth == 0
                    && switch_depth == 0
                {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::StructuredControlExits,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Break",
                        },
                        "`break` outside loop or switch",
                        "Break at loop_depth=0, switch_depth=0",
                    ));
                }
            }
            PreHirStmt::Continue => {
                if invariants.contains(IrInvariant::StructuredControlExits) && loop_depth == 0 {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::StructuredControlExits,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Continue",
                        },
                        "`continue` outside loop",
                        &format!("Continue at loop_depth=0, switch_depth={switch_depth}"),
                    ));
                }
            }
        }
        path.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_prehir_lvalue(
    lhs: &PreHirLValue,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    stage: IrStage,
    pass: &str,
    revision: u64,
    site: &ValidationSite,
    violations: &mut Vec<StructuralViolation>,
) {
    match lhs {
        PreHirLValue::Var(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "lvalue variable has empty name",
                    "PreHirLValue::Var(\"\")",
                ));
            }
        }
        PreHirLValue::Deref { ptr, ty } => {
            validate_prehir_expr(
                ptr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
        }
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            validate_prehir_expr(
                base,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            validate_prehir_expr(
                index,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(elem_ty, stage, pass, revision, site, violations);
            }
        }
        PreHirLValue::FieldAccess {
            base,
            field_name,
            ty,
            ..
        } => {
            validate_prehir_expr(
                base,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if field_name.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "field lvalue has empty field_name",
                        "PreHirLValue::FieldAccess { field_name: \"\" }",
                    ));
                }
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_prehir_expr(
    expr: &PreHirExpr,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    stage: IrStage,
    pass: &str,
    revision: u64,
    site: &ValidationSite,
    violations: &mut Vec<StructuralViolation>,
) {
    match expr {
        PreHirExpr::Var(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "expression variable has empty name",
                    "PreHirExpr::Var(\"\")",
                ));
            }
        }
        PreHirExpr::AddressOfGlobal(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "address-of-global has empty name",
                    "PreHirExpr::AddressOfGlobal(\"\")",
                ));
            }
        }
        PreHirExpr::AddressOfLocal(name) => {
            if invariants.contains(IrInvariant::BindingScope) {
                if name.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        site.clone(),
                        "address-of-local has empty name",
                        "PreHirExpr::AddressOfLocal(\"\")",
                    ));
                } else if let Some(declared) = declared_bindings
                    && !declared.contains(name.as_str())
                {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding { name: name.clone() },
                        format!("address-of-local references undeclared binding '{name}'"),
                        &format!("PreHirExpr::AddressOfLocal({name})"),
                    ));
                }
            }
        }
        PreHirExpr::Const(_, ty) => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
        }
        PreHirExpr::Cast { ty, expr } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                expr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::Unary { expr, ty, .. } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                expr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::Binary { lhs, rhs, ty, .. } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                lhs,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            validate_prehir_expr(
                rhs,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                cond,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            validate_prehir_expr(
                then_expr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            validate_prehir_expr(
                else_expr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::Call {
            target, args, ty, ..
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if target.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "call expression has empty target",
                        "PreHirExpr::Call { target: \"\" }",
                    ));
                }
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            for arg in args {
                validate_prehir_expr(
                    arg,
                    invariants,
                    declared_bindings,
                    stage,
                    pass,
                    revision,
                    site,
                    violations,
                );
            }
        }
        PreHirExpr::Load { ptr, ty } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                ptr,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::PtrOffset { base, .. } => {
            validate_prehir_expr(
                base,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(elem_ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                base,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
            validate_prehir_expr(
                index,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::FieldAccess {
            base,
            field_name,
            ty,
            ..
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if field_name.is_empty() {
                    violations.push(StructuralViolation::new(
                        stage,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "field access has empty field_name",
                        "PreHirExpr::FieldAccess { field_name: \"\" }",
                    ));
                }
                validate_nir_type(ty, stage, pass, revision, site, violations);
            }
            validate_prehir_expr(
                base,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
        PreHirExpr::AggregateCopy { src, size } => {
            if invariants.contains(IrInvariant::EffectRepresentation) && *size == 0 {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::EffectRepresentation,
                    site.clone(),
                    "aggregate copy has zero byte size",
                    "PreHirExpr::AggregateCopy { size: 0 }",
                ));
            }
            validate_prehir_expr(
                src,
                invariants,
                declared_bindings,
                stage,
                pass,
                revision,
                site,
                violations,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::PreHirBinding;
    use fission_midend_core::ir::NirType;

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    #[test]
    fn validates_prehir_mutants_and_valid_control_flow() {
        let mut func = PreHirFunction {
            name: "prehir_fn".to_string(),
            params: vec![PreHirBinding {
                name: "a0".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            locals: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            return_type: i32_ty(),
            body: vec![
                PreHirStmt::While {
                    cond: PreHirExpr::Var("a0".to_string()),
                    body: vec![PreHirStmt::Break].into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::AddressOfLocal("x".to_string()))),
            ],
            ..Default::default()
        };

        assert!(validate_prehir_function(&func, IrInvariantSet::PREHIR_STAGE, "init", 0).is_ok());

        func.body.push(PreHirStmt::Goto("dangling".to_string()));
        let err = validate_prehir_function(&func, IrInvariantSet::PREHIR_STAGE, "bad_pass", 2)
            .unwrap_err();
        assert!(!err.proves_semantic_equivalence());
        assert_eq!(err.stage, IrStage::PreHir);
        assert_eq!(err.pass, "bad_pass");
        assert_eq!(err.revision, 2);
        assert_eq!(
            err.violations[0].invariant,
            IrInvariant::LabelGotoResolution
        );
    }
}
