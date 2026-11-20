//! The two bits of hand-rolled XML scanning shared by the small spec parsers.

/// What follows a `<?...?>`, `<!...>` or `<!-- ... -->` that starts at `rest`
/// (the text just after the `<`), or `None` if it never ends.
///
/// A comment runs to `-->`, not to the first `>`: Ghidra's specs comment out
/// whole runs of elements, and every `>` inside the comment belongs to it.
pub(crate) fn skip_markup(rest: &str) -> Option<&str> {
    if let Some(body) = rest.strip_prefix("!--") {
        return body.find("-->").map(|end| &body[end + 3..]);
    }
    rest.find('>').map(|end| &rest[end + 1..])
}

/// The name of the element whose `<` was just consumed, including the leading
/// `/` of a closing tag (`</volatile>` is `/volatile`).
///
/// The name stops at whitespace, `>` or `/` -- but a closing tag's own `/` is
/// part of the name, otherwise every closing tag came out as the empty string
/// and no `"/volatile"`-style match could ever fire.
pub(crate) fn tag_name(rest: &str) -> &str {
    let skip = usize::from(rest.starts_with('/'));
    let end = rest[skip..]
        .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
        .map_or(rest.len(), |i| i + skip);
    &rest[..end]
}

/// Extract an attribute value by name from an XML tag segment (e.g., `key="value"` or `key='value'`).
/// Matches attributes as distinct tokens (avoiding prefix/substring collisions), tolerates single
/// quotes and optional whitespace around `=`, and rejects unclosed attribute quotes.
pub(crate) fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    let mut chars = segment.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            chars.next();
            while let Some((_, qc)) = chars.next() {
                if qc == quote {
                    break;
                }
            }
            continue;
        }
        if c == '/' || c == '>' {
            chars.next();
            continue;
        }

        // Start of a token (tag name or attribute name).
        let name_start = i;
        while let Some(&(next_i, next_c)) = chars.peek() {
            if next_c.is_ascii_whitespace()
                || next_c == '='
                || next_c == '/'
                || next_c == '>'
                || next_c == '"'
                || next_c == '\''
            {
                break;
            }
            chars.next();
        }
        let name_end = chars.peek().map_or(segment.len(), |&(ni, _)| ni);
        let name = &segment[name_start..name_end];

        while let Some(&(_, next_c)) = chars.peek() {
            if next_c.is_ascii_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        if let Some(&(_, '=')) = chars.peek() {
            chars.next();

            while let Some(&(_, next_c)) = chars.peek() {
                if next_c.is_ascii_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }

            if let Some(&(_, quote)) = chars.peek()
                && (quote == '"' || quote == '\'')
            {
                chars.next();
                let val_start = chars.peek().map_or(segment.len(), |&(vi, _)| vi);
                let mut val_end = val_start;
                let mut closed = false;
                while let Some((vi, vc)) = chars.next() {
                    if vc == quote {
                        val_end = vi;
                        closed = true;
                        break;
                    }
                }
                if closed && name == key {
                    return Some(&segment[val_start..val_end]);
                }
            }
        }
    }
    None
}
