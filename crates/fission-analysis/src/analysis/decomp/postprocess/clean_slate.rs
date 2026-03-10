use super::PostProcessor;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

static LPRECT_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bLPRECT\s+(param_\d+)\b").expect("valid LPRECT param regex")
});

static RECT_WHOLE_ASSIGN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\*\(uint8_t\s*\(\*\)\[16\]\)\s*(param_\d+)\s*=\s*CONCAT016\s*\(\s*0\s*,\s*(local_[A-Za-z0-9_]+)\s*\);",
    )
    .expect("valid RECT whole-assign regex")
});

static LOCAL16_DECL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\buint8_t\s+(local_[A-Za-z0-9_]+)\s*\[\s*16\s*\]\s*;")
        .expect("valid local 16-byte decl regex")
});

static RETURN_MOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"return\s+\((\w+)\s*%\s*4294967296\);").expect("valid return mod regex")
});

static RETURN_MASK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"return\s+(\w+)\s*&\s*0x[fF]{8}(?:[uU](?:[lL]{1,2})?)?\s*;")
        .expect("valid return mask regex")
});

fn local_decl_re(local_name: &str) -> Regex {
    Regex::new(&format!(
        r"\buint8_t\s+{}\s*\[\s*16\s*\]\s*;",
        regex::escape(local_name)
    ))
    .expect("valid local decl regex")
}

fn local_index_re(local_name: &str) -> Regex {
    Regex::new(&format!(r"\b{}\s*\[", regex::escape(local_name)))
        .expect("valid local index regex")
}

impl PostProcessor {
    pub(super) fn clean_ghidra_artifacts_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut rewritten = code.to_string();

        let lprect_params: Vec<String> = LPRECT_PARAM_RE
            .captures_iter(code)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect();

        for caps in RECT_WHOLE_ASSIGN_RE.captures_iter(code) {
            let Some(param_name) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let Some(local_name) = caps.get(2).map(|m| m.as_str()) else {
                continue;
            };
            if !lprect_params.iter().any(|p| p == param_name) {
                continue;
            }
            if !LOCAL16_DECL_RE.is_match(&rewritten) {
                continue;
            }
            if local_index_re(local_name).find_iter(&rewritten).count() > 1 {
                continue;
            }

            rewritten = local_decl_re(local_name)
                .replace_all(&rewritten, format!("RECT {};", local_name))
                .into_owned();

            rewritten = rewritten.replace(
                caps.get(0).map_or("", |m| m.as_str()),
                &format!("*{} = {};", param_name, local_name),
            );
        }

        if let Some(return_caps) = RETURN_MOD_RE
            .captures(&rewritten)
            .or_else(|| RETURN_MASK_RE.captures(&rewritten))
        {
            if let Some(var_name) = return_caps.get(1).map(|m| m.as_str().to_string()) {
                let needle = format!("{var_name} = CONCAT71(");
                let mut replaced_any = false;
                let lines: Vec<String> = rewritten
                    .lines()
                    .map(|line| {
                        let trimmed = line.trim_start();
                        if !trimmed.starts_with(&needle) {
                            return line.to_string();
                        }

                        let Some(comma_pos) = trimmed.find(',') else {
                            return line.to_string();
                        };
                        let Some(close_pos) = trimmed.rfind(");") else {
                            return line.to_string();
                        };
                        if close_pos <= comma_pos {
                            return line.to_string();
                        }

                        let indent = &line[..line.len() - trimmed.len()];
                        let expr = trimmed[comma_pos + 1..close_pos].trim();
                        replaced_any = true;
                        format!("{indent}{var_name} = (ulonglong)({expr});")
                    })
                    .collect();
                if replaced_any {
                    rewritten = lines.join("\n");
                    rewritten = RETURN_MOD_RE
                        .replace_all(&rewritten, format!("return {};", var_name))
                        .into_owned();
                    rewritten = RETURN_MASK_RE
                        .replace_all(&rewritten, format!("return {};", var_name))
                        .into_owned();
                }
            }
        }

        if rewritten == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(rewritten)
        }
    }

    pub(super) fn clean_ghidra_artifacts(code: &str) -> String {
        Self::clean_ghidra_artifacts_cow(code).into_owned()
    }
}
