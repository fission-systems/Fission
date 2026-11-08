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
