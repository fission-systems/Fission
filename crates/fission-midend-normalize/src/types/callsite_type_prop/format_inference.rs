//! Variadic format-string type propagation.
//!
//! This module owns printf-family format parsing, translated-format flow,
//! and copy-chain propagation. The parent call-site pass supplies the
//! binding refinement helper and keeps the public pass entrypoint stable.

use super::*;

/// Ghidra `FormatStringAnalyzer` scorecard item: types printf-family
/// variadic arguments from their own format string's `%`-conversion
/// specifiers, e.g. `printf("%d %s", x, y)` types `x` as `int` and `y` as
/// `char*` -- previously only the fixed leading parameter (if any) was
/// ever typed for a variadic call, per [`is_known_variadic_runtime_symbol`]'s
/// role elsewhere in this file (arity pruning only).
///
/// The format string's *text* is trivially available here already: `lower_
/// varnode_inner` (`fission-pcode/src/midend/builder/expr/lower_expr.rs`)
/// resolves a constant matching `options.global_names` -- which
/// `NirRenderOptions::from_loaded_binary` (`fission-midend-core/src/ir/
/// options.rs`) pre-populates with every extracted `.rdata` string,
/// already wrapped in quotes and escaped -- to `PreHirExpr::AddressOfGlobal(
/// "\"...\"")`. [`arg_var_name`] (used by [`collect_callsites_stmts`]
/// already, for the unrelated existing per-parameter typing above) already
/// captures `AddressOfGlobal` names verbatim, so the quoted format-string
/// text is already sitting in `arg_vars` by the time this runs -- no new
/// binary access or HIR traversal needed.
///
/// Deliberately scoped to the unambiguous ANSI narrow-string printf family
/// (`printf`/`fprintf`/`sprintf`/`snprintf`/their `_s` secure-CRT
/// variants). Two families are intentionally excluded, not overlooked:
/// scanf-family functions take *pointers* to write into (a different
/// typing rule -- `%d` there means `int*`, not `int`), and the wide-
/// character `wprintf`/`swprintf` family flips `%s`'s meaning (narrow
/// `char*`, not `wchar_t*`, per the ANSI convention -- a correctness trap
/// not worth the risk without a dedicated fixture to validate against).
pub(super) fn apply_variadic_printf_format_string_arg_types(
    func: &mut PreHirFunction,
    callsites: &[(Option<String>, String, Vec<Option<String>>)],
) -> bool {
    // The call-site argument variable is often just a same-block temp
    // holding a plain copy of the real source binding (`argN = param_2;
    // printf(fmt, argN)`) -- a shape later copy-propagation would
    // normally collapse, but doing so isn't guaranteed to happen *before*
    // this pass's own type refinement would otherwise need to survive
    // (confirmed via a real fixture: the temp's type refined correctly on
    // every fixed-point iteration, but never reached the real `char *`
    // parameter it was copied from in the final output). Walking the
    // copy chain back to the true source and refining every hop directly
    // sidesteps that pipeline-ordering fragility instead of depending on
    // it.
    let mut copy_sources = HashMap::default();
    collect_copy_sources(&func.body, &mut copy_sources);

    let mut changed = false;
    for (_, callee, arg_vars) in callsites {
        let Some(format_index) = admitted_printf_style_format_index(func, callee) else {
            continue;
        };
        let Some(literal) = arg_vars
            .get(format_index)
            .and_then(|arg| arg.as_deref())
            .and_then(quoted_string_literal_text)
        else {
            continue;
        };
        for (offset, ty) in parse_printf_format_specifier_types(literal)
            .into_iter()
            .enumerate()
        {
            let Some(ty) = ty else { continue };
            let arg_index = format_index + 1 + offset;
            let Some(Some(arg_var)) = arg_vars.get(arg_index) else {
                continue;
            };
            changed |= apply_variadic_printf_arg_ty_transitively(func, &copy_sources, arg_var, &ty);
        }
    }
    changed
}

fn admitted_printf_style_format_index(func: &PreHirFunction, target: &str) -> Option<usize> {
    let format_index = printf_style_format_string_arg_index(target)?;
    let canonical = canonical_variadic_runtime_symbol(target);
    if matches!(
        canonical.as_str(),
        "error" | "error_at_line" | "printf_chk" | "fprintf_chk" | "sprintf_chk" | "snprintf_chk"
    ) {
        let summary = func.callee_summaries.get(target)?;
        if !summary.target.is_import_locked() {
            return None;
        }
    }
    Some(format_index)
}

#[derive(Clone, Default)]
struct FormatFlowState {
    translated_literals: HashMap<String, String>,
    copy_chains: HashMap<String, Vec<String>>,
}

impl FormatFlowState {
    fn clear(&mut self) {
        self.translated_literals.clear();
        self.copy_chains.clear();
    }

    fn copy_chain_for_expr(&self, expr: &PreHirExpr) -> Vec<String> {
        let Some(name) = plain_copy_var(expr) else {
            return Vec::new();
        };
        self.copy_chains
            .get(name)
            .cloned()
            .unwrap_or_else(|| vec![name.to_string()])
    }

    fn format_literal_for_expr(&self, expr: &PreHirExpr) -> Option<String> {
        match expr {
            PreHirExpr::AddressOfGlobal(name) => {
                quoted_string_literal_text(name).map(str::to_string)
            }
            PreHirExpr::Var(name) => self.translated_literals.get(name).cloned(),
            PreHirExpr::Cast { expr, .. } => self.format_literal_for_expr(expr),
            _ => None,
        }
    }
}

fn plain_copy_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr, .. } => plain_copy_var(expr),
        _ => None,
    }
}

fn imported_translation_message_index(func: &PreHirFunction, target: &str) -> Option<usize> {
    let message_index = match canonical_variadic_runtime_symbol(target).as_str() {
        "gettext" => 0,
        "dcgettext" => 1,
        _ => return None,
    };
    func.callee_summaries
        .get(target)
        .filter(|summary| summary.target.is_import_locked())?;
    Some(message_index)
}

fn translated_literal_from_expr(func: &PreHirFunction, expr: &PreHirExpr) -> Option<String> {
    let expr = match expr {
        PreHirExpr::Cast { expr, .. } => expr.as_ref(),
        _ => expr,
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        return None;
    };
    let message_index = imported_translation_message_index(func, target)?;
    args.get(message_index).and_then(|message| match message {
        PreHirExpr::AddressOfGlobal(name) => quoted_string_literal_text(name).map(str::to_string),
        PreHirExpr::Cast { expr, .. } => match expr.as_ref() {
            PreHirExpr::AddressOfGlobal(name) => {
                quoted_string_literal_text(name).map(str::to_string)
            }
            _ => None,
        },
        _ => None,
    })
}

type FormatCallEvidence = (usize, String, Vec<Vec<String>>);

fn collect_site_sensitive_format_evidence_expr(
    func: &PreHirFunction,
    expr: &PreHirExpr,
    state: &FormatFlowState,
    out: &mut Vec<FormatCallEvidence>,
) {
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            if let Some(format_index) = admitted_printf_style_format_index(func, target)
                && let Some(literal) = args
                    .get(format_index)
                    .and_then(|format| state.format_literal_for_expr(format))
            {
                out.push((
                    format_index,
                    literal,
                    args.iter()
                        .map(|arg| state.copy_chain_for_expr(arg))
                        .collect(),
                ));
            }
            for arg in args {
                collect_site_sensitive_format_evidence_expr(func, arg, state, out);
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_site_sensitive_format_evidence_expr(func, lhs, state, out);
            collect_site_sensitive_format_evidence_expr(func, rhs, state, out);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            collect_site_sensitive_format_evidence_expr(func, expr, state, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_site_sensitive_format_evidence_expr(func, base, state, out);
            collect_site_sensitive_format_evidence_expr(func, index, state, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_site_sensitive_format_evidence_expr(func, cond, state, out);
            collect_site_sensitive_format_evidence_expr(func, then_expr, state, out);
            collect_site_sensitive_format_evidence_expr(func, else_expr, state, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_site_sensitive_format_evidence_stmts(
    func: &PreHirFunction,
    stmts: &[PreHirStmt],
    state: &mut FormatFlowState,
    out: &mut Vec<FormatCallEvidence>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_site_sensitive_format_evidence_expr(func, rhs, state, out);
                let PreHirLValue::Var(target) = lhs else {
                    continue;
                };
                let translated_literal = translated_literal_from_expr(func, rhs).or_else(|| {
                    plain_copy_var(rhs)
                        .and_then(|source| state.translated_literals.get(source).cloned())
                });
                match translated_literal {
                    Some(literal) => {
                        state.translated_literals.insert(target.clone(), literal);
                    }
                    None => {
                        state.translated_literals.remove(target);
                    }
                }

                if let Some(source) = plain_copy_var(rhs) {
                    let mut chain = vec![target.clone()];
                    chain.extend(
                        state
                            .copy_chains
                            .get(source)
                            .cloned()
                            .unwrap_or_else(|| vec![source.to_string()]),
                    );
                    if chain.iter().collect::<HashSet<_>>().len() == chain.len() {
                        state.copy_chains.insert(target.clone(), chain);
                    } else {
                        state.copy_chains.remove(target);
                    }
                } else {
                    state
                        .copy_chains
                        .insert(target.clone(), vec![target.clone()]);
                }
            }
            PreHirStmt::Expr(expr) => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out)
            }
            PreHirStmt::Return(Some(expr)) => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out);
                state.clear();
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_site_sensitive_format_evidence_expr(func, va_list, state, out)
            }
            PreHirStmt::Block(body) => {
                collect_site_sensitive_format_evidence_stmts(func, body, state, out)
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_site_sensitive_format_evidence_expr(func, cond, state, out);
                let mut then_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, then_body, &mut then_state, out);
                let mut else_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, else_body, &mut else_state, out);
                state.clear();
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                collect_site_sensitive_format_evidence_expr(func, cond, state, out);
                let mut body_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, body, &mut body_state, out);
                state.clear();
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let mut loop_state = state.clone();
                if let Some(init) = init {
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        std::slice::from_ref(init.as_ref()),
                        &mut loop_state,
                        out,
                    );
                }
                if let Some(cond) = cond {
                    collect_site_sensitive_format_evidence_expr(func, cond, &loop_state, out);
                }
                collect_site_sensitive_format_evidence_stmts(func, body, &mut loop_state, out);
                if let Some(update) = update {
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        std::slice::from_ref(update.as_ref()),
                        &mut loop_state,
                        out,
                    );
                }
                state.clear();
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out);
                for case in cases {
                    let mut case_state = state.clone();
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        &case.body,
                        &mut case_state,
                        out,
                    );
                }
                let mut default_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(
                    func,
                    default,
                    &mut default_state,
                    out,
                );
                state.clear();
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => state.clear(),
        }
    }
}

pub(super) fn apply_site_sensitive_translated_format_types(func: &mut PreHirFunction) -> bool {
    let mut evidence = Vec::new();
    collect_site_sensitive_format_evidence_stmts(
        func,
        &func.body,
        &mut FormatFlowState::default(),
        &mut evidence,
    );

    let mut changed = false;
    for (format_index, literal, arg_chains) in evidence {
        for (offset, ty) in parse_printf_format_specifier_types(&literal)
            .into_iter()
            .enumerate()
        {
            let Some(ty) = ty else { continue };
            let Some(chain) = arg_chains.get(format_index + 1 + offset) else {
                continue;
            };
            for name in chain {
                if let Some(binding) = binding_by_name_mut(&mut func.locals, name)
                    .or_else(|| binding_by_name_mut(&mut func.params, name))
                {
                    changed |= apply_variadic_printf_arg_ty(binding, &ty);
                }
            }
        }
    }
    changed
}

/// Like [`tighten_binding_ty`], but additionally allowed to override a
/// generic *unsigned*-int binding with a format-specifier scalar type.
///
/// By the time this pass runs on real compiled code, a call-argument
/// binding almost never still has `NirType::Unknown` -- `fission-pcode`'s
/// HIR builder always assigns *some* default int type at materialization
/// time based purely on the raw register/stack-slot width (`type_from_
/// size(size, false)`, used throughout the builder), and that default is
/// always unsigned. That default is not real type evidence, just "whatever
/// size the value happened to be passed in". A format specifier is strong,
/// authoritative evidence for scalar variadic arguments and may also refine
/// signedness/width in ways the ordinary monotone rule deliberately does not.
fn apply_variadic_printf_arg_ty(binding: &mut PreHirBinding, candidate: &NirType) -> bool {
    if tighten_binding_ty(binding, candidate) {
        return true;
    }
    if matches!(binding.ty, NirType::Int { signed: false, .. }) && binding.ty != *candidate {
        binding.ty = candidate.clone();
        return true;
    }
    false
}

/// Applies [`apply_variadic_printf_arg_ty`] to `arg_var`'s own binding,
/// then walks `copy_sources` backward (bounded by a visited-set, same
/// cycle-safety pattern used throughout this crate) applying it to every
/// transitive copy-source too, so a refinement on a call-site temp
/// reaches the real originating parameter/local it was copied from.
fn apply_variadic_printf_arg_ty_transitively(
    func: &mut PreHirFunction,
    copy_sources: &HashMap<String, String>,
    arg_var: &str,
    ty: &NirType,
) -> bool {
    let mut changed = false;
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    while visited.insert(current.clone()) {
        if let Some(b) = binding_by_name_mut(&mut func.locals, &current)
            .or_else(|| binding_by_name_mut(&mut func.params, &current))
        {
            changed |= apply_variadic_printf_arg_ty(b, ty);
        }
        match copy_sources.get(&current) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    changed
}

/// Single-hop `target = source` (bare `Var`-to-`Var`) copy map, used by
/// [`apply_variadic_printf_arg_ty_transitively`] to trace a call-site
/// argument temp back to its real originating binding.
pub(super) fn collect_copy_sources(stmts: &[PreHirStmt], out: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(target),
                rhs: PreHirExpr::Var(source),
            } => {
                out.insert(target.clone(), source.clone());
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_copy_sources(body, out);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_sources(then_body, out);
                collect_copy_sources(else_body, out);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_copy_sources(std::slice::from_ref(i), out);
                }
                if let Some(u) = update {
                    collect_copy_sources(std::slice::from_ref(u), out);
                }
                collect_copy_sources(body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_sources(&case.body, out);
                }
                collect_copy_sources(default, out);
            }
            _ => {}
        }
    }
}

/// Strips the surrounding quotes `NirRenderOptions::from_loaded_binary`
/// wraps every extracted string constant in, or `None` if `name` isn't
/// one (e.g. an ordinary symbol/global name, or a non-constant argument
/// [`arg_var_name`] captured by variable name instead).
fn quoted_string_literal_text(name: &str) -> Option<&str> {
    name.strip_prefix('"')?.strip_suffix('"')
}

/// Scans a printf-style format string for `%`-conversion specifiers,
/// returning one entry per specifier (in order) with the `NirType` it
/// implies for that variadic argument, or `None` for a specifier this
/// doesn't have a confident type for (unrecognized conversion character --
/// leaves that argument's type alone rather than guessing). `%%` (a
/// literal percent) and a `*` dynamic width/precision (which itself
/// consumes an extra leading `int` argument, per the C standard: "the
/// argument supplying [a `*`] width/precision... shall appear before the
/// argument (if any) to be converted") are both handled to keep the
/// specifier-to-argument-position alignment correct.
pub(super) fn parse_printf_format_specifier_types(text: &str) -> Vec<Option<NirType>> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        if chars.peek().copied() == Some('%') {
            chars.next();
            continue;
        }
        while matches!(chars.peek().copied(), Some('-' | '+' | ' ' | '#' | '0')) {
            chars.next();
        }
        if chars.peek().copied() == Some('*') {
            chars.next();
            result.push(Some(NirType::Int {
                bits: 32,
                signed: true,
            }));
        } else {
            while matches!(chars.peek().copied(), Some(c) if c.is_ascii_digit()) {
                chars.next();
            }
        }
        if chars.peek().copied() == Some('.') {
            chars.next();
            if chars.peek().copied() == Some('*') {
                chars.next();
                result.push(Some(NirType::Int {
                    bits: 32,
                    signed: true,
                }));
            } else {
                while matches!(chars.peek().copied(), Some(c) if c.is_ascii_digit()) {
                    chars.next();
                }
            }
        }
        // Length modifiers: `hh`/`h` (narrower, doesn't affect promoted
        // vararg width so ignored), `l`/`ll` (`ll` is always 64-bit, while
        // a lone `l` has ABI-dependent integer width and also means wide-char
        // for `%s`/`%c`), `L`/`z`/`j`/`t`
        // (ignored, doesn't change the promoted vararg width this cares
        // about), MSVC `I32`/`I64`.
        let mut long_count = 0u8;
        loop {
            match chars.peek().copied() {
                Some('h') => {
                    chars.next();
                    if chars.peek().copied() == Some('h') {
                        chars.next();
                    }
                }
                Some('l') => {
                    chars.next();
                    long_count += 1;
                    if chars.peek().copied() == Some('l') {
                        chars.next();
                        long_count += 1;
                    }
                }
                Some('L' | 'z' | 'j' | 't') => {
                    chars.next();
                }
                Some('I') => {
                    chars.next();
                    if chars.peek().copied() == Some('6') {
                        chars.next();
                        if chars.peek().copied() == Some('4') {
                            chars.next();
                            long_count = 2;
                        }
                    } else if chars.peek().copied() == Some('3') {
                        chars.next();
                        if chars.peek().copied() == Some('2') {
                            chars.next();
                        }
                    }
                }
                _ => break,
            }
        }
        let Some(conv) = chars.next() else {
            break;
        };
        let is_wide = long_count >= 1;
        let integer_bits = match long_count {
            0 => Some(32),
            1 => None,
            _ => Some(64),
        };
        let ty = match conv {
            'd' | 'i' => integer_bits.map(|bits| NirType::Int { bits, signed: true }),
            'u' | 'x' | 'X' | 'o' => integer_bits.map(|bits| NirType::Int {
                bits,
                signed: false,
            }),
            'c' => Some(NirType::Int {
                bits: 32,
                signed: true,
            }),
            's' => Some(NirType::Ptr(Box::new(NirType::Int {
                bits: if is_wide { 16 } else { 8 },
                signed: false,
            }))),
            'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'a' | 'A' => Some(NirType::Float { bits: 64 }),
            'p' => Some(NirType::Ptr(Box::new(NirType::Unknown))),
            'n' => Some(NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: true,
            }))),
            _ => None,
        };
        result.push(ty);
    }
    result
}
