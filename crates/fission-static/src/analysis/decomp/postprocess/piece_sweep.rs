use super::PostProcessor;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static PIECE_ACCESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?P<var>[A-Za-z_][A-Za-z0-9_]*)\._(?P<offset>\d+)_(?P<size>\d+)_")
        .unwrap_or_else(|e| panic!("invalid PIECE_ACCESS regex: {e}"))
});

static EXPLICIT_BYTE_POINTER_ACCESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\*\((?P<ty>uint(?:8|16|32|64)_t)\s*\*\)\(\((?:uint8_t|byte|uint1)\s*\*\)&(?P<base>[A-Za-z_][A-Za-z0-9_]*)\s*\+\s*(?P<offset>\d+)\)",
    )
    .unwrap_or_else(|e| panic!("invalid EXPLICIT_BYTE_POINTER_ACCESS regex: {e}"))
});

impl PostProcessor {
    pub(super) fn normalize_piece_accesses_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("._") && !code.contains("((uint8_t *)&") && !code.contains("((byte *)&") {
            return Cow::Borrowed(code);
        }

        let rewritten_pieces = PIECE_ACCESS
            .replace_all(code, |caps: &regex::Captures| {
                let var = caps.name("var").map(|m| m.as_str()).unwrap_or_default();
                let offset = caps
                    .name("offset")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0);
                let size = caps
                    .name("size")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0);

                rewrite_piece_access(var, offset, size).unwrap_or_else(|| caps[0].to_string())
            })
            .to_string();

        let rewritten = EXPLICIT_BYTE_POINTER_ACCESS
            .replace_all(&rewritten_pieces, |caps: &regex::Captures| {
                let ty = caps.name("ty").map(|m| m.as_str()).unwrap_or_default();
                let base = caps.name("base").map(|m| m.as_str()).unwrap_or_default();
                let offset = caps
                    .name("offset")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .unwrap_or(0);

                rewrite_explicit_byte_pointer_access(ty, base, offset)
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .to_string();

        if rewritten == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(rewritten)
        }
    }

    pub(super) fn normalize_piece_accesses(code: &str) -> String {
        Self::normalize_piece_accesses_cow(code).into_owned()
    }
}

fn rewrite_piece_access(var: &str, offset: u32, size: u32) -> Option<String> {
    if let Some(ty) = scalar_piece_type(size) {
        let ptr = if offset == 0 {
            format!("&{var}")
        } else {
            format!("((uint8_t *)&{var} + {offset})")
        };
        return Some(format!("*({ty} *){ptr}"));
    }

    if size == 0 {
        return None;
    }

    let ptr = if offset == 0 {
        format!("&{var}")
    } else {
        format!("((uint8_t *)&{var} + {offset})")
    };
    Some(format!("*(uint8_t (*)[{size}]){ptr}"))
}

fn scalar_piece_type(size: u32) -> Option<&'static str> {
    match size {
        1 => Some("uint8_t"),
        2 => Some("uint16_t"),
        4 => Some("uint32_t"),
        8 => Some("uint64_t"),
        _ => None,
    }
}

fn rewrite_explicit_byte_pointer_access(ty: &str, base: &str, offset: u32) -> Option<String> {
    let width = match ty {
        "uint8_t" => 1,
        "uint16_t" => 2,
        "uint32_t" => 4,
        "uint64_t" => 8,
        _ => return None,
    };

    if offset % width != 0 {
        return None;
    }

    let index = offset / width;
    if index == 0 {
        Some(format!("*({ty} *)&{base}"))
    } else {
        Some(format!("(({ty} *)&{base})[{index}]"))
    }
}
