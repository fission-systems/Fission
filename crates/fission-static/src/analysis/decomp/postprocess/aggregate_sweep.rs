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

static ZERO_AGG_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:ZEXT\d+\s*\(\s*0\s*\)|CONCAT0?\d+\s*\(\s*0\s*,\s*0\s*\))$")
        .unwrap_or_else(|e| panic!("invalid ZERO_AGG_EXPR regex: {e}"))
});

static NOOP_AGG_ASSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<indent>\s*)\*\((?P<lhs_agg>fission_agg\d+)\s*\*\)(?P<lhs>[^=;]+?)\s*=\s*(?P<rhs_expr>\*\([^;]+)\s*;\s*$",
    )
    .unwrap_or_else(|e| panic!("invalid NOOP_AGG_ASSIGN regex: {e}"))
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
        if (!code.contains("CONCAT0") || !code.contains("uint8_t")) && !code.contains("fission_agg")
        {
            return Cow::Borrowed(code);
        }

        let local_sizes = collect_local_aggregate_sizes(code);
        let mut used_sizes = BTreeSet::new();
        let mut changed = false;
        let mut rewritten_lines = Vec::new();

        for line in code.lines() {
            let Some(caps) = WHOLE_OBJECT_COPY.captures(line) else {
                rewritten_lines.push(line.to_string());
                continue;
            };
            let Some(lhs) = caps.name("lhs").map(|m| m.as_str().trim()) else {
                rewritten_lines.push(line.to_string());
                continue;
            };
            let Some(rhs) = caps.name("rhs").map(|m| m.as_str().trim()) else {
                rewritten_lines.push(line.to_string());
                continue;
            };

            let Some(size) = infer_aggregate_size(lhs, rhs, &local_sizes) else {
                rewritten_lines.push(line.to_string());
                continue;
            };
            let Some(lhs_expr) = rewrite_aggregate_lvalue(lhs, size, &local_sizes) else {
                rewritten_lines.push(line.to_string());
                continue;
            };
            let Some(rhs_expr) = rewrite_aggregate_rvalue(rhs, size, &local_sizes) else {
                rewritten_lines.push(line.to_string());
                continue;
            };

            used_sizes.insert(size);
            changed = true;
            if normalize_expr(&lhs_expr) != normalize_expr(&rhs_expr) {
                rewritten_lines.push(format!(
                    "{}{} = {};",
                    caps.name("indent").map_or("", |m| m.as_str()),
                    lhs_expr,
                    rhs_expr
                ));
            }
        }

        let rewritten = rewritten_lines.join("\n");

        let cleaned = remove_noop_aggregate_assigns(&rewritten);
        if !changed && cleaned == code {
            return Cow::Borrowed(code);
        }

        Cow::Owned(insert_aggregate_typedefs(&cleaned, &used_sizes))
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

    if ZERO_AGG_EXPR.is_match(expr) {
        return Some(format!("(fission_agg{size}){{0}}"));
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

fn normalize_expr(expr: &str) -> String {
    expr.split_whitespace().collect::<String>()
}

fn remove_noop_aggregate_assigns(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let Some(caps) = NOOP_AGG_ASSIGN.captures(line) else {
                return true;
            };
            let lhs_agg = caps
                .name("lhs_agg")
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let rhs_expr = caps
                .name("rhs_expr")
                .map(|m| m.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let rhs_prefix = format!("*({lhs_agg} *)");
            let Some(rhs) = rhs_expr.strip_prefix(&rhs_prefix) else {
                return true;
            };
            let lhs = caps.name("lhs").map(|m| m.as_str()).unwrap_or_default();
            normalize_expr(lhs) != normalize_expr(rhs)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
