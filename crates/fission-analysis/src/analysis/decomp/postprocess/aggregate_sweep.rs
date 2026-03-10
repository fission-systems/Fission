use super::PostProcessor;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeSet;

static WHOLE_OBJECT_COPY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<indent>\s*)(?P<lhs>[^=;]+?)\s*=\s*CONCAT0?\d+\s*\(\s*0\s*,\s*(?P<rhs>.+)\s*\)\s*;\s*$",
    )
    .unwrap_or_else(|e| panic!("invalid WHOLE_OBJECT_COPY regex: {e}"))
});

static LOCAL_ARRAY_DECL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:uint8_t|byte|undefined\d*)\s+(?P<name>local_[A-Za-z0-9_]+)\s*\[\s*(?P<size>16|24|32)\s*\]\s*;\s*$",
    )
        .unwrap_or_else(|e| panic!("invalid LOCAL_ARRAY_DECL regex: {e}"))
});

static CAST_ARRAY_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\*\(uint8_t\s*\(\*\)\s*\[\s*(?P<size>\d+)\s*\]\)\s*(?P<base>.+)$")
        .unwrap_or_else(|e| panic!("invalid CAST_ARRAY_EXPR regex: {e}"))
});

static SIMPLE_DEREF_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\*(?P<base>[A-Za-z_][A-Za-z0-9_]*(?:\[[^\]]+\])?)$")
        .unwrap_or_else(|e| panic!("invalid SIMPLE_DEREF_EXPR regex: {e}"))
});

static SIMPLE_IDENT_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<name>[A-Za-z_][A-Za-z0-9_]*)$")
        .unwrap_or_else(|e| panic!("invalid SIMPLE_IDENT_EXPR regex: {e}"))
});

impl PostProcessor {
    pub(super) fn normalize_aggregate_copies_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("CONCAT0") || !code.contains("uint8_t") {
            return Cow::Borrowed(code);
        }

        let local_sizes = collect_local_aggregate_sizes(code);
        let mut used_sizes = BTreeSet::new();
        let mut changed = false;

        let rewritten = code
            .lines()
            .map(|line| {
                let Some(caps) = WHOLE_OBJECT_COPY.captures(line) else {
                    return line.to_string();
                };
                let Some(lhs) = caps.name("lhs").map(|m| m.as_str().trim()) else {
                    return line.to_string();
                };
                let Some(rhs) = caps.name("rhs").map(|m| m.as_str().trim()) else {
                    return line.to_string();
                };

                let Some(size) = infer_aggregate_size(lhs, rhs, &local_sizes) else {
                    return line.to_string();
                };
                let Some(lhs_expr) = rewrite_aggregate_lvalue(lhs, size, &local_sizes) else {
                    return line.to_string();
                };
                let Some(rhs_expr) = rewrite_aggregate_rvalue(rhs, size, &local_sizes) else {
                    return line.to_string();
                };

                used_sizes.insert(size);
                changed = true;
                format!(
                    "{}{} = {};",
                    caps.name("indent").map_or("", |m| m.as_str()),
                    lhs_expr,
                    rhs_expr
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        if !changed {
            return Cow::Borrowed(code);
        }

        Cow::Owned(insert_aggregate_typedefs(&rewritten, &used_sizes))
    }

    pub(super) fn normalize_aggregate_copies(code: &str) -> String {
        Self::normalize_aggregate_copies_cow(code).into_owned()
    }
}

fn collect_local_aggregate_sizes(code: &str) -> std::collections::HashMap<String, usize> {
    let mut sizes = std::collections::HashMap::new();
    for line in code.lines() {
        if let Some(caps) = LOCAL_ARRAY_DECL.captures(line) {
            let name = caps.name("name").map(|m| m.as_str().to_string());
            let size = caps
                .name("size")
                .and_then(|m| m.as_str().parse::<usize>().ok());
            if let (Some(name), Some(size)) = (name, size) {
                sizes.insert(name, size);
            }
        }
    }
    sizes
}

fn infer_aggregate_size(
    lhs: &str,
    rhs: &str,
    local_sizes: &std::collections::HashMap<String, usize>,
) -> Option<usize> {
    extract_size_from_operand(lhs, local_sizes)
        .or_else(|| extract_size_from_operand(rhs, local_sizes))
}

fn extract_size_from_operand(
    expr: &str,
    local_sizes: &std::collections::HashMap<String, usize>,
) -> Option<usize> {
    if let Some(caps) = CAST_ARRAY_EXPR.captures(expr) {
        return caps
            .name("size")
            .and_then(|m| m.as_str().parse::<usize>().ok());
    }
    if let Some(caps) = SIMPLE_IDENT_EXPR.captures(expr) {
        return local_sizes.get(caps.name("name")?.as_str()).copied();
    }
    None
}

fn rewrite_aggregate_lvalue(
    expr: &str,
    size: usize,
    local_sizes: &std::collections::HashMap<String, usize>,
) -> Option<String> {
    if !matches!(size, 16 | 24 | 32) {
        return None;
    }

    if let Some(caps) = CAST_ARRAY_EXPR.captures(expr) {
        let base = caps.name("base")?.as_str().trim();
        let cast_size = caps.name("size")?.as_str().parse::<usize>().ok()?;
        if cast_size == size {
            return Some(format!("*(fission_agg{size} *){base}"));
        }
    }

    if let Some(caps) = SIMPLE_IDENT_EXPR.captures(expr) {
        let name = caps.name("name")?.as_str();
        if local_sizes.get(name).copied() == Some(size) {
            return Some(format!("*(fission_agg{size} *)&{name}"));
        }
    }

    if let Some(caps) = SIMPLE_DEREF_EXPR.captures(expr) {
        let base = caps.name("base")?.as_str().trim();
        return Some(format!("*(fission_agg{size} *){base}"));
    }

    None
}

fn rewrite_aggregate_rvalue(
    expr: &str,
    size: usize,
    local_sizes: &std::collections::HashMap<String, usize>,
) -> Option<String> {
    if !matches!(size, 16 | 24 | 32) {
        return None;
    }

    if let Some(caps) = CAST_ARRAY_EXPR.captures(expr) {
        let base = caps.name("base")?.as_str().trim();
        let cast_size = caps.name("size")?.as_str().parse::<usize>().ok()?;
        if cast_size == size {
            return Some(format!("*(fission_agg{size} *){base}"));
        }
    }

    if let Some(caps) = SIMPLE_DEREF_EXPR.captures(expr) {
        let base = caps.name("base")?.as_str().trim();
        return Some(format!("*(fission_agg{size} *){base}"));
    }

    if let Some(caps) = SIMPLE_IDENT_EXPR.captures(expr) {
        let name = caps.name("name")?.as_str();
        if local_sizes.get(name).copied() == Some(size) {
            return Some(format!("*(fission_agg{size} *)&{name}"));
        }
    }

    None
}

fn insert_aggregate_typedefs(code: &str, sizes: &BTreeSet<usize>) -> String {
    if sizes.is_empty() {
        return code.to_string();
    }

    let mut typedefs = Vec::new();
    for size in sizes {
        let marker = format!("typedef struct {{ uint8_t bytes[{size}]; }} fission_agg{size};");
        if !code.contains(&marker) {
            typedefs.push(marker);
        }
    }
    if typedefs.is_empty() {
        return code.to_string();
    }

    format!("{}\n\n{}", typedefs.join("\n"), code)
}
