use super::PostProcessor;
use crate::utils::patterns::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

/// Pattern for Rust overflow checks
static RUST_OVERFLOW_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?s)if\s*\([^\{]*overflow[^\{]*\)\s*\{\s*panic_const_(add|sub|mul)_overflow\(\);?\s*\}",
    )
    .unwrap_or_else(|e| panic!("invalid RUST_OVERFLOW_PATTERN regex: {e}"))
});

/// Pattern for Rust bounds checks
static RUST_BOUNDS_CHECK_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)if\s*\([^\{]*(?:index|len)[^\{]*\)\s*\{\s*panic_bounds_check\([^\{]*\);?\s*\}")
        .unwrap_or_else(|e| panic!("invalid RUST_BOUNDS_CHECK_PATTERN regex: {e}"))
});

impl PostProcessor {
    /// Insert missing casts for common patterns where the decompiler omits
    /// explicit type conversions that would improve readability.
    ///
    /// Patterns handled:
    /// 1. `malloc(N)` → `(type *)malloc(N)` when assigned to a typed pointer
    /// 2. `*(base + offset)` → `*(type *)(base + offset)` when type is inferrable
    /// 3. Void pointer arithmetic without explicit cast
    pub(super) fn insert_missing_casts_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !MALLOC_PATTERN.is_match(code) && !LONG_PTR_OFFSET.is_match(code) {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();

        // Pattern 1: malloc/calloc return without cast
        // `var = malloc(...)` → `var = (void *)malloc(...)`
        // Only when the LHS doesn't already have a cast on the RHS

        result = MALLOC_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let lhs = &caps[1];
                let rhs = &caps[2];
                // Check if there's already a cast
                if rhs.contains("(void *)") || rhs.contains("(void*)") {
                    return caps[0].to_string();
                }
                format!("{} = (void *){};", lhs, rhs)
            })
            .to_string();

        // Pattern 2: Bare integer used as pointer in dereference
        // `*(ulong + 0xN)` → `*(void *)(ulong + 0xN)`
        // This fires when a numeric/undefined type is used with pointer arithmetic

        result = LONG_PTR_OFFSET
            .replace_all(&result, |caps: &regex::Captures| {
                let base_expr = &caps[1];
                let offset = &caps[2];
                format!("*(void *)({} + {})", base_expr, offset)
            })
            .to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn insert_missing_casts(code: &str) -> String {
        Self::insert_missing_casts_cow(code).into_owned()
    }

    pub(super) fn remove_rust_boilerplate_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        if !RUST_OVERFLOW_PATTERN.is_match(code) && !RUST_BOUNDS_CHECK_PATTERN.is_match(code) {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();

        // Replace overflow checks with comments
        result = RUST_OVERFLOW_PATTERN
            .replace_all(&result, "/* [Safety Check: Overflow] */")
            .to_string();

        // Replace bounds checks
        result = RUST_BOUNDS_CHECK_PATTERN
            .replace_all(&result, "/* [Safety Check: Bounds] */")
            .to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn remove_rust_boilerplate(&self, code: &str) -> String {
        self.remove_rust_boilerplate_cow(code).into_owned()
    }

    pub(super) fn remove_go_boilerplate_cow<'a>(&self, code: &'a str) -> Cow<'a, str> {
        if !GO_PANIC.is_match(code) {
            return Cow::Borrowed(code);
        }

        let mut result = code.to_string();

        // Replace Go gopanic checks
        result = GO_PANIC
            .replace_all(&result, "/* [Go Panic Check] */")
            .to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn remove_go_boilerplate(&self, code: &str) -> String {
        self.remove_go_boilerplate_cow(code).into_owned()
    }

    // =========================================================================
    // A-1: Deref → Array Index (RetDec deref_to_array_index_optimizer.cpp)
    //   *(a + N)  →  a[N]     *(N + a)  →  a[N]
    // =========================================================================
    pub(super) fn deref_to_array_index_cow<'a>(code: &'a str) -> Cow<'a, str> {
        // *(var + N)  →  var[N]
        // *(N + var)  →  var[N]  (commutative)

        let mut result = Cow::Borrowed(code);

        if BASE_ARRAY_ACCESS.is_match(result.as_ref()) {
            result = Cow::Owned(
                BASE_ARRAY_ACCESS
                    .replace_all(result.as_ref(), |caps: &regex::Captures| {
                        let base = &caps["base"];
                        let idx = &caps["idx"];
                        if base.starts_with('(') {
                            return caps[0].to_string();
                        }
                        format!("{}[{}]", base, idx)
                    })
                    .to_string(),
            );
        }

        if REV_ARRAY_ACCESS.is_match(result.as_ref()) {
            result = Cow::Owned(
                REV_ARRAY_ACCESS
                    .replace_all(result.as_ref(), |caps: &regex::Captures| {
                        let base = &caps["base"];
                        let idx = &caps["idx"];
                        format!("{}[{}]", base, idx)
                    })
                    .to_string(),
            );
        }

        result
    }

    pub(super) fn deref_to_array_index(code: &str) -> String {
        Self::deref_to_array_index_cow(code).into_owned()
    }

    // =========================================================================
    // A-2: Bit-op → Logical-op in conditions
    //   if ((a == b) & (c != d))   →  if ((a == b) && (c != d))
    //   if ((a < b) | (c > d))     →  if ((a < b) || (c > d))
    // Only when both sides are comparison expressions.
    // (RetDec bit_op_to_log_op_optimizer.cpp)
    // =========================================================================
    pub(super) fn bitop_to_logicop_cow<'a>(code: &'a str) -> Cow<'a, str> {
        // Match (comparison) & (comparison)  →  (comparison) && (comparison)
        static BIT_AND_TO_LOG_AND: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*(?P<lhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
                r"\s*&\s*",
                r"\(\s*(?P<rhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
            ))
            .unwrap_or_else(|e| panic!("invalid BIT_AND_TO_LOG_AND regex: {e}"))
        });
        // Match (comparison) | (comparison)  →  (comparison) || (comparison)
        static BIT_OR_TO_LOG_OR: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*(?P<lhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
                r"\s*\|\s*",
                r"\(\s*(?P<rhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
            ))
            .unwrap_or_else(|e| panic!("invalid BIT_OR_TO_LOG_OR regex: {e}"))
        });

        let mut result = Cow::Borrowed(code);

        if BIT_AND_TO_LOG_AND.is_match(result.as_ref()) {
            result = Cow::Owned(
                BIT_AND_TO_LOG_AND
                    .replace_all(result.as_ref(), |caps: &regex::Captures| {
                        let full = &caps[0];
                        full.replacen(") &", ") &&", 1)
                    })
                    .to_string(),
            );
        }

        if BIT_OR_TO_LOG_OR.is_match(result.as_ref()) {
            result = Cow::Owned(
                BIT_OR_TO_LOG_OR
                    .replace_all(result.as_ref(), |caps: &regex::Captures| {
                        let full = &caps[0];
                        full.replacen(") |", ") ||", 1)
                    })
                    .to_string(),
            );
        }

        result
    }

    pub(super) fn bitop_to_logicop(code: &str) -> String {
        Self::bitop_to_logicop_cow(code).into_owned()
    }

    // =========================================================================
    // A-3: Constant condition removal (RetDec dead_code_optimizer.cpp)
    //   if (true)  / if (1)  → keep body only
    //   if (false) / if (0)  → remove (keep else if present)
    //   while (false)        → remove
    //   Empty else { }       → remove
    // =========================================================================
    pub(super) fn remove_constant_conditions_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut result = code.to_string();

        fn has_nested_braces(s: &str) -> bool {
            s.contains('{') || s.contains('}')
        }

        result = WHILE_FALSE
            .replace_all(&result, |caps: &regex::Captures| {
                if has_nested_braces(&caps["body"]) {
                    caps[0].to_string()
                } else {
                    String::new()
                }
            })
            .to_string();

        result = IF_FALSE_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                let else_body = &caps["else_body"];
                if has_nested_braces(else_body) {
                    caps[0].to_string()
                } else {
                    else_body.trim().to_string()
                }
            })
            .to_string();

        result = IF_FALSE
            .replace_all(&result, |caps: &regex::Captures| {
                if has_nested_braces(&caps["body"]) {
                    caps[0].to_string()
                } else {
                    String::new()
                }
            })
            .to_string();

        result = IF_TRUE_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                let body = &caps["body"];
                if has_nested_braces(body) {
                    caps[0].to_string()
                } else {
                    body.trim().to_string()
                }
            })
            .to_string();

        result = IF_TRUE
            .replace_all(&result, |caps: &regex::Captures| {
                let body = &caps["body"];
                if has_nested_braces(body) {
                    caps[0].to_string()
                } else {
                    body.trim().to_string()
                }
            })
            .to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn remove_constant_conditions(code: &str) -> String {
        Self::remove_constant_conditions_cow(code).into_owned()
    }

    // =========================================================================
    // B-2: Dead local assignment removal
    //  (RetDec dead_local_assign_optimizer.cpp)
    //
    // If a compiler-generated variable (local_XX, xVarN) appears exactly once
    // in the entire function — on the LHS of an assignment — and the RHS has
    // no side effects (no function call), the assignment is dead and removed.
    // Run twice to handle cascading dead assignments.
    // =========================================================================
    pub(super) fn remove_dead_local_assigns_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut var_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for cap in LOCAL_VAR_PATTERN.find_iter(code) {
            *var_counts.entry(cap.as_str().to_string()).or_insert(0) += 1;
        }

        let lines: Vec<&str> = code.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut changed = false;

        for line in &lines {
            if let Some(caps) = LOCAL_VAR_ASSIGN.captures(line) {
                let lhs = &caps[1];
                let rhs = &caps[2];

                if let Some(&count) = var_counts.get(lhs)
                    && count == 1
                    && !FUNC_CALL.is_match(rhs)
                {
                    changed = true;
                    continue;
                }
            }
            result_lines.push(line.to_string());
        }

        if !changed {
            return Cow::Borrowed(code);
        }
        Cow::Owned(result_lines.join("\n"))
    }

    pub(super) fn remove_dead_local_assigns(code: &str) -> String {
        Self::remove_dead_local_assigns_cow(code).into_owned()
    }
}
