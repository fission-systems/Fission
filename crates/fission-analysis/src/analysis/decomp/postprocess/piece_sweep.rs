use super::PostProcessor;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static PIECE_ACCESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?P<var>[A-Za-z_][A-Za-z0-9_]*)\._(?P<offset>\d+)_(?P<size>\d+)_")
        .unwrap_or_else(|e| panic!("invalid PIECE_ACCESS regex: {e}"))
});

impl PostProcessor {
    pub(super) fn normalize_piece_accesses_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("._") {
            return Cow::Borrowed(code);
        }

        let rewritten = PIECE_ACCESS
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
