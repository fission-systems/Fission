use super::PostProcessor;
use super::type_promotion::lookup_promoted_struct_param;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

static WHOLE_ASSIGN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\*\(uint8_t\s*\(\*\)\[(\d+)\]\)\s*(param_\d+)\s*=\s*CONCAT0?\d+\s*\(\s*0\s*,\s*(local_[A-Za-z0-9_]+)\s*\);",
    )
    .expect("valid whole-assign regex")
});

static RETURN_MOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"return\s+\((\w+)\s*%\s*4294967296\);").expect("valid return mod regex")
});

static RETURN_MASK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"return\s+(\w+)\s*&\s*0x[fF]{8}(?:[uU](?:[lL]{1,2})?)?\s*;")
        .expect("valid return mask regex")
});

fn local_decl_re(local_name: &str, size: usize) -> Regex {
    Regex::new(&format!(
        r"\buint8_t\s+{}\s*\[\s*{}\s*\]\s*;",
        regex::escape(local_name),
        size
    ))
    .expect("valid local decl regex")
}

fn local_index_re(local_name: &str) -> Regex {
    Regex::new(&format!(r"\b{}\s*\[", regex::escape(local_name))).expect("valid local index regex")
}

impl PostProcessor {
    pub(super) fn clean_ghidra_artifacts_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let mut rewritten = code.to_string();

        for caps in WHOLE_ASSIGN_RE.captures_iter(code) {
            let Some(size) = caps.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) else {
                continue;
            };
            let Some(param_name) = caps.get(2).map(|m| m.as_str()) else {
                continue;
            };
            let Some(local_name) = caps.get(3).map(|m| m.as_str()) else {
                continue;
            };
            let Some(promoted) = lookup_promoted_struct_param(&rewritten, param_name, size) else {
                continue;
            };
            if !local_decl_re(local_name, size).is_match(&rewritten) {
                continue;
            }
            if local_index_re(local_name).find_iter(&rewritten).count() > 1 {
                continue;
            }

            rewritten = local_decl_re(local_name, size)
                .replace_all(
                    &rewritten,
                    format!("{} {};", promoted.struct_name, local_name),
                )
                .into_owned();

            rewritten = rewritten.replace(
                caps.get(0).map_or("", |m| m.as_str()),
                &format!("*{} = {};", promoted.param_name, local_name),
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
