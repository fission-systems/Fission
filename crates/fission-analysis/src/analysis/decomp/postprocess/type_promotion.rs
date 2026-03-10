use super::PostProcessor;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::sync::LazyLock;

static PARAM_DECL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"uint8_t\s*\(\*\s*(param_\d+)\s*\)\s*\[\s*16\s*\]")
        .expect("valid RECT param regex")
});

fn call_trigger_re(param_name: &str) -> Regex {
    Regex::new(&format!(
        r"GetClientRect\s*\([^;\n]*\b{}\b[^;\n]*\)",
        regex::escape(param_name)
    ))
    .expect("valid GetClientRect trigger regex")
}

fn whole_object_write_re(param_name: &str) -> Regex {
    Regex::new(&format!(
        r"\*\s*{}\s*=\s*CONCAT0?16\s*\(",
        regex::escape(param_name)
    ))
    .expect("valid whole-object write regex")
}

fn whole_object_assign_re(param_name: &str) -> Regex {
    Regex::new(&format!(
        r"\*\s*{}\s*=\s*(CONCAT0?16\s*\([^;]+\));",
        regex::escape(param_name)
    ))
    .expect("valid whole-object assign regex")
}

impl PostProcessor {
    pub(super) fn promote_rect_params_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut promoted_params = Vec::new();

        for captures in PARAM_DECL_RE.captures_iter(code) {
            let Some(param_match) = captures.get(1) else {
                continue;
            };
            let param_name = param_match.as_str();
            if !call_trigger_re(param_name).is_match(code) {
                continue;
            }
            if !whole_object_write_re(param_name).is_match(code) {
                continue;
            }
            promoted_params.push(param_name.to_string());
        }

        if promoted_params.is_empty() {
            return Cow::Borrowed(code);
        }

        let mut rewritten = code.to_string();
        for param_name in promoted_params {
            rewritten = PARAM_DECL_RE
                .replace_all(&rewritten, |caps: &Captures| {
                    if caps.get(1).map(|m| m.as_str()) == Some(param_name.as_str()) {
                        format!("LPRECT {}", param_name)
                    } else {
                        caps.get(0).map_or("", |m| m.as_str()).to_string()
                    }
                })
                .into_owned();

            let assign_re = whole_object_assign_re(&param_name);
            rewritten = assign_re
                .replace_all(&rewritten, |caps: &Captures| {
                    let rhs = caps.get(1).map_or("", |m| m.as_str());
                    format!("*(uint8_t (*)[16]){} = {};", param_name, rhs)
                })
                .into_owned();
        }

        if rewritten == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(rewritten)
        }
    }

    pub(super) fn promote_rect_params(code: &str) -> String {
        Self::promote_rect_params_cow(code).into_owned()
    }
}
