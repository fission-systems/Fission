use super::PostProcessor;
use super::condition::negate_condition;
use crate::utils::patterns::EMPTY_ELSE;
use regex::Regex;
use std::borrow::Cow;

impl PostProcessor {
    // =========================================================================
    // A-4: If structure simplification (RetDec if_structure_optimizer.cpp)
    //   Pattern 1: if (c) { return X; } else { S; }  →  if (c) { return X; } S;
    //   Pattern 2: Empty else removal:  } else { }  →  }
    // =========================================================================
    pub(super) fn simplify_if_structure_cow(code: &str) -> Cow<'_, str> {
        // Pattern 1: if (c) { ... return ...; } else { BODY }  →  if (c) { ... return ...; } BODY
        // Safe because if-body always returns, so else is unreachable otherwise.
        static IF_RETURN_ELSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
            Regex::new(concat!(
                r"(?P<if_block>if\s*\([^)]+\)\s*\{[^}]*\breturn\b[^}]*\})",
                r"\s*else\s*\{(?P<else_body>[^}]*)\}",
            ))
            .unwrap_or_else(|e| panic!("invalid IF_RETURN_ELSE regex: {e}"))
        });

        let mut result = code.to_string();

        result = IF_RETURN_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                let if_block = &caps["if_block"];
                let else_body = caps["else_body"].trim();
                let if_open_count = if_block.matches('{').count();
                if if_open_count > 1 || else_body.contains('{') || else_body.contains('}') {
                    return caps[0].to_string();
                }
                if else_body.is_empty() {
                    if_block.to_string()
                } else {
                    format!("{if_block}\n{else_body}")
                }
            })
            .to_string();

        result = EMPTY_ELSE.replace_all(&result, "}").to_string();

        if result == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(result)
        }
    }

    pub(super) fn simplify_if_structure(code: &str) -> String {
        Self::simplify_if_structure_cow(code).into_owned()
    }

    // =========================================================================
    // A-5: while(true) → while(cond)  (simple case)
    //   while (true) { if (cond) break; S; }  →  while (!cond) { S; }
    //   Only when the break-if is the FIRST statement in the loop body.
    // (RetDec while_true_to_while_cond_optimizer.cpp)
    // =========================================================================
    pub(super) fn while_true_to_while_cond_cow(code: &str) -> Cow<'_, str> {
        static WHILE_TRUE_BREAK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
            Regex::new(concat!(
                r"(?P<indent>\s*)while\s*\(\s*(?:true|1)\s*\)\s*\{\s*\n",
                r"(?P<inner_indent>\s*)if\s*\(\s*(?P<cond>[^{}\n]+?)\s*\)\s*\{?\s*\n?\s*break\s*;\s*\}?\s*\n",
                r"(?P<body>(?s).*?)",
                r"\n(?P<close_indent>\s*)\}",
            ))
            .unwrap_or_else(|e| panic!("invalid WHILE_TRUE_BREAK regex: {e}"))
        });

        if !WHILE_TRUE_BREAK.is_match(code) {
            return Cow::Borrowed(code);
        }

        Cow::Owned(
            WHILE_TRUE_BREAK
                .replace_all(code, |caps: &regex::Captures| {
                    let indent = &caps["indent"];
                    let cond = caps["cond"].trim();
                    let body = &caps["body"];
                    let close_indent = &caps["close_indent"];
                    let negated = negate_condition(cond);
                    format!(
                        "{indent}while ({negated}) {{\n{body}\n{close_indent}}}"
                    )
                })
                .to_string(),
        )
    }

    pub(super) fn while_true_to_while_cond(code: &str) -> String {
        Self::while_true_to_while_cond_cow(code).into_owned()
    }
}
