//! Assemble per-function renders into one buildable translation unit.
//!
//! Every function is decompiled independently, and each render carries the
//! declarations *that function* needs: the aggregate typedefs it references,
//! the globals it touches, an `extern` for each callee it does not define.
//! Concatenated, those per-function preambles contradict each other -- the
//! same aggregate typedef'd twice, an `extern unsigned long long main()`
//! standing a few hundred lines above `void main(void)`. On the measurement
//! binary that accounted for 48 of 124 compile errors, none of them about the
//! code itself.
//!
//! So the unit is assembled rather than concatenated: declarations are lifted
//! out of every function, merged into one prelude, and the bodies follow. A
//! callee the unit defines needs no `extern` at all -- it needs a prototype,
//! which is taken verbatim from that function's own definition line and is
//! therefore exact by construction.

use std::collections::{BTreeSet, HashMap, HashSet};

/// The C types the renders name but C does not define.
///
/// These spellings come from Ghidra's type model and are what the printer
/// emits for a value of known width and unknown meaning. A unit that uses
/// them has to define them, and nothing else in the pipeline does. `bool` is
/// the exception that C does define, from C23 on, as a keyword a typedef may
/// not redeclare.
const PRELUDE: &str = "\
#include <stdint.h>
#include <stddef.h>

typedef unsigned char undefined;
typedef unsigned char undefined1;
typedef unsigned short undefined2;
typedef unsigned int undefined4;
typedef unsigned long long undefined8;
typedef unsigned char byte;
typedef unsigned char uchar;
typedef unsigned short ushort;
typedef unsigned int uint;
typedef unsigned long ulong;
typedef __int128 int128;
#if !defined(__STDC_VERSION__) || __STDC_VERSION__ <= 201710L
typedef _Bool bool;
#endif
";

/// One function's render, split where its declarations end and its body begins.
///
/// A declaration is one *record*, not one line: an aggregate typedef spelled
/// with its fields spans several, and splitting it into lines files its
/// opening and closing halves separately.
struct SplitRender<'a> {
    declarations: Vec<String>,
    body: &'a str,
}

/// The assembled source and any unresolved conflicts found while combining
/// per-function declarations.
pub(crate) struct ProjectAssembly {
    pub code: String,
    pub diagnostics: Vec<ProjectAssemblyDiagnostic>,
}

/// A machine-readable conflict in the project-wide global prelude.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct ProjectAssemblyDiagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub analysis_status: &'static str,
    pub global_name: String,
    pub message: String,
    pub resolution: &'static str,
    pub declarations: Vec<ProjectGlobalDeclarationCandidate>,
}

/// One distinct spelling and an example function render that supplied it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct ProjectGlobalDeclarationCandidate {
    pub declaration: String,
    pub function: Option<String>,
}

struct SeenGlobalDeclaration<'a> {
    declaration: &'a str,
    function: Option<String>,
    diagnostic_index: Option<usize>,
}

/// Assemble `renders` (in emission order) into a single translation unit.
pub(crate) fn assemble(renders: &[String]) -> ProjectAssembly {
    let renders = disambiguate_conflicting_typedefs(renders);
    let split: Vec<SplitRender<'_>> = renders.iter().map(|text| split_render(text)).collect();

    let defined: HashSet<&str> = split
        .iter()
        .filter_map(|render| definition_line(render.body))
        .filter_map(declared_function_name)
        .collect();

    let mut typedefs: Vec<&str> = Vec::new();
    let mut aggregates: Vec<&str> = Vec::new();
    let mut globals: Vec<&str> = Vec::new();
    // Non-aggregate typedefs with conflicting spellings keep the first
    // declaration. Aggregate variants were assigned distinct names before
    // splitting so each function body retains the layout it was rendered
    // against.
    let mut typedef_names: HashMap<&str, &str> = HashMap::new();
    // Every render's declaration of each callee, by name, in first-seen order.
    let mut extern_variants: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut extern_index: HashMap<&str, usize> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut global_declarations: HashMap<&str, SeenGlobalDeclaration<'_>> = HashMap::new();
    let mut diagnostics: Vec<ProjectAssemblyDiagnostic> = Vec::new();

    for render in &split {
        let function = definition_line(render.body)
            .and_then(declared_function_name)
            .map(str::to_owned);
        for decl in render.declarations.iter().map(String::as_str) {
            if let Some(name) = extern_function_name(decl) {
                // The unit defines it; a prototype below says so exactly.
                if defined.contains(name) {
                    continue;
                }
                // Keyed by name, not by text. Two renders can disagree about
                // an undefined callee's return type -- a Go binary produced
                // both `extern uchar __popcount();` and `extern unsigned long
                // long __popcount();` -- and keeping both is a conflicting
                // declaration, which is a compile error rather than extra
                // information. They can disagree about its parameters too;
                // `merge_extern_declarations` settles both once all are seen.
                let index = *extern_index.entry(name).or_insert_with(|| {
                    extern_variants.push((name, Vec::new()));
                    extern_variants.len() - 1
                });
                extern_variants[index].1.push(decl);
                continue;
            }
            if let Some(name) = typedef_name(decl) {
                if typedef_names.insert(name, decl).is_none() {
                    // Aggregate definitions come first: the library-named
                    // typedefs are written in terms of them.
                    if name.starts_with("fission_agg") {
                        aggregates.push(decl);
                    } else {
                        typedefs.push(decl);
                    }
                }
                continue;
            }
            // A function's own address reaches the printer as a name, and the
            // render declares it the way it declares any other address-named
            // global. The unit defines that name as a function, so the data
            // declaration is not just redundant -- it is a different kind of
            // symbol under the same name, which does not compile.
            if let Some(name) = global_name(decl) {
                if defined.contains(name) {
                    continue;
                }

                if let Some(existing) = global_declarations.get(name) {
                    if existing.declaration != decl {
                        let current_candidate = ProjectGlobalDeclarationCandidate {
                            declaration: decl.to_owned(),
                            function: function.clone(),
                        };
                        if let Some(index) = existing.diagnostic_index {
                            let diagnostic = &mut diagnostics[index];
                            if !diagnostic
                                .declarations
                                .iter()
                                .any(|candidate| candidate.declaration == decl)
                            {
                                diagnostic.declarations.push(current_candidate);
                            }
                        } else {
                            diagnostics.push(ProjectAssemblyDiagnostic {
                                code: "project_global_declaration_disagreement",
                                severity: "warning",
                                analysis_status: "incomplete",
                                global_name: name.to_owned(),
                                message: format!(
                                    "Per-function renders disagree on the declaration for global `{name}`; compatibility is unresolved."
                                ),
                                resolution: "first_declaration_emitted",
                                declarations: vec![
                                    ProjectGlobalDeclarationCandidate {
                                        declaration: existing.declaration.to_owned(),
                                        function: existing.function.clone(),
                                    },
                                    current_candidate,
                                ],
                            });
                            let index = diagnostics.len() - 1;
                            global_declarations
                                .get_mut(name)
                                .expect("global declaration was observed")
                                .diagnostic_index = Some(index);
                        }
                    }
                    continue;
                }

                // The emitted C namespace is the only identity still available
                // at this assembly boundary. Keep one declaration per name;
                // when spellings disagree, the diagnostic records that the
                // first spelling was retained without claiming it is safe.
                globals.push(decl);
                global_declarations.insert(
                    name,
                    SeenGlobalDeclaration {
                        declaration: decl,
                        function: function.clone(),
                        diagnostic_index: None,
                    },
                );
                continue;
            }

            if seen.insert(decl) {
                globals.push(decl);
            }
        }
    }

    let externs: Vec<String> = extern_variants
        .iter()
        .map(|(name, variants)| merge_extern_declarations(name, variants))
        .collect();

    let prototypes: BTreeSet<String> = split
        .iter()
        .filter_map(|render| definition_line(render.body))
        .filter(|line| declared_function_name(line).is_some())
        .map(|line| format!("{};", line.trim_end()))
        .collect();

    let mut out = String::new();
    out.push_str(PRELUDE);
    push_section(&mut out, "aggregates", &aggregates);
    push_section(&mut out, "types", &typedefs);
    push_section(&mut out, "globals", &globals);
    push_section(
        &mut out,
        "external functions",
        &externs.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    push_section(
        &mut out,
        "functions defined in this unit",
        &prototypes.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    out.push('\n');
    for render in &split {
        out.push_str(render.body.trim_end());
        out.push_str("\n\n");
    }
    ProjectAssembly {
        code: out,
        diagnostics,
    }
}

/// Keep same-sized aggregate layouts from different functions distinct.
///
/// The per-function renderer names aggregates by byte size, so two unrelated
/// 16-byte layouts both arrive as `fission_agg16`. Keeping the first typedef
/// while leaving later function bodies unchanged makes valid field accesses
/// refer to members absent from the retained layout. Rename a conflicting
/// aggregate and all of its C identifier references in that render. Repeat
/// when a renamed aggregate is nested in another aggregate or a surface
/// typedef, so those dependent declarations remain consistent too.
fn disambiguate_conflicting_typedefs(renders: &[String]) -> Vec<String> {
    let mut reserved_names = HashSet::new();
    for render in renders {
        visit_c_identifiers(render, |_, _, name| {
            reserved_names.insert(name.to_string());
        });
    }

    let mut known_typedefs: HashMap<String, String> = HashMap::new();
    let mut variant_counters: HashMap<String, u32> = HashMap::new();
    let mut rewritten_renders = Vec::with_capacity(renders.len());

    for render in renders {
        let mut renames = HashMap::new();
        let rewritten = loop {
            let rewritten = replace_c_identifiers(render, &renames);
            let split = split_render(&rewritten);
            let mut added_rename = false;

            for declaration in &split.declarations {
                let Some(name) = typedef_name(declaration) else {
                    continue;
                };
                let Some(existing) = known_typedefs.get(name) else {
                    continue;
                };
                if existing == declaration {
                    continue;
                }

                let is_aggregate = name.starts_with("fission_agg");
                let depends_on_renamed_type =
                    declaration_references_renamed_name(declaration, &renames);
                if (is_aggregate || depends_on_renamed_type) && !renames.contains_key(name) {
                    let variant =
                        fresh_typedef_variant(name, &mut reserved_names, &mut variant_counters);
                    renames.insert(name.to_string(), variant);
                    added_rename = true;
                }
            }

            if !added_rename {
                break rewritten;
            }
        };

        for declaration in split_render(&rewritten).declarations {
            if let Some(name) = typedef_name(&declaration) {
                known_typedefs
                    .entry(name.to_string())
                    .or_insert_with(|| declaration.clone());
            }
        }
        rewritten_renders.push(rewritten);
    }

    rewritten_renders
}

fn fresh_typedef_variant(
    name: &str,
    reserved_names: &mut HashSet<String>,
    variant_counters: &mut HashMap<String, u32>,
) -> String {
    let suffix = variant_counters.entry(name.to_string()).or_insert(1);
    loop {
        *suffix += 1;
        let candidate = format!("{name}_variant_{suffix}");
        if reserved_names.insert(candidate.clone()) {
            return candidate;
        }
    }
}

fn declaration_references_renamed_name(
    declaration: &str,
    renames: &HashMap<String, String>,
) -> bool {
    if renames.is_empty() {
        return false;
    }
    let renamed: HashSet<&str> = renames.values().map(String::as_str).collect();
    let mut references_renamed = false;
    visit_c_identifiers(declaration, |_, _, name| {
        references_renamed |= renamed.contains(name);
    });
    references_renamed
}

fn replace_c_identifiers(source: &str, replacements: &HashMap<String, String>) -> String {
    if replacements.is_empty() {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    let mut copied_through = 0;
    visit_c_identifiers(source, |start, end, identifier| {
        if let Some(replacement) = replacements.get(identifier) {
            out.push_str(&source[copied_through..start]);
            out.push_str(replacement);
            copied_through = end;
        }
    });

    out.push_str(&source[copied_through..]);
    out
}

fn visit_c_identifiers(source: &str, mut visit: impl FnMut(usize, usize, &str)) {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            cursor += 2;
            while cursor + 1 < bytes.len() && !(bytes[cursor] == b'*' && bytes[cursor + 1] == b'/')
            {
                cursor += 1;
            }
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if matches!(bytes[cursor], b'\'' | b'"') {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' {
                    cursor = (cursor + 2).min(bytes.len());
                } else if bytes[cursor] == quote {
                    cursor += 1;
                    break;
                } else {
                    cursor += 1;
                }
            }
            continue;
        }
        if !is_c_identifier_start(bytes[cursor]) {
            cursor += 1;
            continue;
        }

        let start = cursor;
        cursor += 1;
        while cursor < bytes.len() && is_c_identifier_continue(bytes[cursor]) {
            cursor += 1;
        }
        if is_standalone_c_identifier(bytes, start, cursor) {
            visit(start, cursor, &source[start..cursor]);
        }
    }
}

fn is_standalone_c_identifier(bytes: &[u8], start: usize, end: usize) -> bool {
    let left_is_identifier =
        start > 0 && (is_c_identifier_continue(bytes[start - 1]) || bytes[start - 1] >= 0x80);
    let right_is_identifier =
        end < bytes.len() && (is_c_identifier_continue(bytes[end]) || bytes[end] >= 0x80);
    !left_is_identifier && !right_is_identifier
}

fn is_c_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_c_identifier_continue(byte: u8) -> bool {
    is_c_identifier_start(byte) || byte.is_ascii_digit()
}

fn push_section(out: &mut String, title: &str, lines: &[&str]) {
    if lines.is_empty() {
        return;
    }
    out.push_str(&format!("\n// -- {title} --\n"));
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

fn line_spans(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos >= text.len() {
            return None;
        }
        let start = pos;
        let rest = &text[start..];
        if let Some(nl) = rest.find('\n') {
            pos = start + nl + 1;
            let line = rest[..nl].strip_suffix('\r').unwrap_or(&rest[..nl]);
            Some((start, line))
        } else {
            pos = text.len();
            Some((start, rest))
        }
    })
}

fn starts_with_keyword(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword).is_some_and(|rest| {
        rest.is_empty()
            || (!rest.as_bytes()[0].is_ascii_alphanumeric() && rest.as_bytes()[0] != b'_')
    })
}

fn record_starts_with_non_func_prefix(record: &str, line: &str) -> bool {
    let first = if record.is_empty() {
        line.trim_start()
    } else {
        record.trim_start()
    };
    starts_with_keyword(first, "typedef") || first.starts_with('#')
}

fn record_starts_as_declaration(record: &str, line: &str) -> bool {
    let first = if record.is_empty() {
        line.trim_start()
    } else {
        record.trim_start()
    };
    record_starts_with_non_func_prefix(record, line) || starts_with_keyword(first, "extern")
}

struct LineScanResult {
    has_code: bool,
    opens_function_body: bool,
    last_code_byte: Option<u8>,
}

fn scan_declaration_line(
    line: &str,
    depth: &mut usize,
    paren_depth: &mut usize,
    saw_paren_at_depth0: &mut bool,
    saw_top_level_equals: &mut bool,
    in_block_comment: &mut bool,
    skip_body_open: bool,
) -> LineScanResult {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut has_code = false;
    let mut last_code_byte = None;
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        if *in_block_comment {
            if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if let Some(q) = quote {
            has_code = true;
            last_code_byte = Some(bytes[i]);
            if bytes[i] == b'\\' {
                i = (i + 2).min(bytes.len());
            } else if bytes[i] == q {
                quote = None;
                i += 1;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            break;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            *in_block_comment = true;
            i += 2;
            continue;
        }
        if matches!(bytes[i], b'"' | b'\'') {
            has_code = true;
            last_code_byte = Some(bytes[i]);
            quote = Some(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        has_code = true;
        let b = bytes[i];
        last_code_byte = Some(b);
        match b {
            b'(' => {
                if *depth == 0 {
                    *saw_paren_at_depth0 = true;
                }
                *paren_depth += 1;
            }
            b')' => {
                *paren_depth = paren_depth.saturating_sub(1);
            }
            b'=' if *depth == 0 && *paren_depth == 0 => {
                *saw_top_level_equals = true;
            }
            b'{' => {
                if *depth == 0
                    && *paren_depth == 0
                    && *saw_paren_at_depth0
                    && !*saw_top_level_equals
                    && !skip_body_open
                {
                    return LineScanResult {
                        has_code,
                        opens_function_body: true,
                        last_code_byte,
                    };
                }
                *depth += 1;
            }
            b'}' => {
                *depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        i += 1;
    }

    LineScanResult {
        has_code,
        opens_function_body: false,
        last_code_byte,
    }
}

/// Split a render at the first line that opens a function definition.
///
/// Every declaration the printer emits ends in `;`, possibly after a braced
/// body it opened on an earlier line (`typedef struct ... {`) or a multi-line
/// parameter list (`extern int f(\n ...);`). A function definition is the first
/// top-level signature that opens a braced body rather than ending in `;`.
///
/// Two things are not code and must not decide that: a comment, which can
/// trail a declaration (`extern int f(int); /* inferred from calls */`) or
/// stand on its own line ahead of one, and a preprocessor directive, which
/// the printer uses to declare an unknown-arity callee once per C dialect. A
/// conditional group is one record from `#if` to its `#endif`, and a comment
/// line belongs to the record that follows it. Comments ahead of the
/// definition are not part of its body.
fn split_render(text: &str) -> SplitRender<'_> {
    let mut depth = 0usize;
    let mut paren_depth = 0usize;
    let mut conditional_depth = 0usize;
    let mut saw_paren_at_depth0 = false;
    let mut saw_top_level_equals = false;
    let mut in_block_comment = false;
    let mut declarations: Vec<String> = Vec::new();
    let mut record = String::new();
    // Where the record starts, its leading comments included, and where its
    // first line of code starts.
    let mut record_start: Option<usize> = None;
    let mut code_start: Option<usize> = None;

    for (offset, line) in line_spans(text) {
        let line_starts_in_comment = in_block_comment;
        let code_record = code_start.map_or("", |start| &text[start..offset]);
        let skip_body_open = record_starts_with_non_func_prefix(code_record, line);
        let scan = scan_declaration_line(
            line,
            &mut depth,
            &mut paren_depth,
            &mut saw_paren_at_depth0,
            &mut saw_top_level_equals,
            &mut in_block_comment,
            skip_body_open,
        );
        if scan.opens_function_body {
            let body_start = code_start.unwrap_or(offset);
            return SplitRender {
                declarations,
                body: &text[body_start..],
            };
        }

        if code_start.is_none() && line.trim().is_empty() {
            continue;
        }

        if record_start.is_none() {
            record_start = Some(offset);
        }
        if code_start.is_none() && scan.has_code {
            code_start = Some(offset);
        }
        if !record.is_empty() {
            record.push('\n');
        }
        record.push_str(line);

        let directive = (!line_starts_in_comment && scan.has_code)
            .then(|| line.trim_start().strip_prefix('#'))
            .flatten();
        if let Some(directive) = directive {
            match directive.split_whitespace().next() {
                Some("if" | "ifdef" | "ifndef") => conditional_depth += 1,
                Some("endif") => conditional_depth = conditional_depth.saturating_sub(1),
                _ => {}
            }
        }

        let is_preprocessor =
            code_start.is_some_and(|start| text[start..].trim_start().starts_with('#'));
        if scan.has_code
            && depth == 0
            && paren_depth == 0
            && conditional_depth == 0
            && (scan.last_code_byte == Some(b';')
                || (is_preprocessor && !line.trim_end().ends_with('\\')))
        {
            declarations.push(std::mem::take(&mut record));
            record_start = None;
            code_start = None;
            saw_paren_at_depth0 = false;
            saw_top_level_equals = false;
        }
    }

    if let Some(code_start) = code_start {
        if saw_paren_at_depth0
            && !saw_top_level_equals
            && !record_starts_as_declaration(&text[code_start..], "")
        {
            return SplitRender {
                declarations,
                body: &text[code_start..],
            };
        }
        declarations.push(record);
        return SplitRender {
            declarations,
            body: "",
        };
    }

    SplitRender {
        declarations,
        // Comments with no declaration after them (an error render is only
        // one) stay where the function would have been.
        body: record_start.map_or("", |start| &text[start..]),
    }
}

/// A line that is a comment and nothing else, or opens one that runs on.
fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed
            .strip_prefix("/*")
            .is_some_and(|rest| rest.find("*/").is_none_or(|close| close + 2 == rest.len()))
}

/// `text` without a comment that closes its last line: `extern int f(int);
/// /* ... */` reads as `extern int f(int);`. Declarations carry no string
/// literals, so a comment opener is never inside one.
fn strip_trailing_comment(text: &str) -> &str {
    let text = text.trim_end();
    let last_line = text.rfind('\n').map_or(0, |newline| newline + 1);
    let text = match text.strip_suffix("*/").and_then(|head| head.rfind("/*")) {
        Some(open) if open >= last_line => &text[..open],
        _ => text,
    };
    let text = match text[last_line..].find("//") {
        Some(open) => &text[..last_line + open],
        None => text,
    };
    text.trim_end()
}

/// The C declarations a record makes, one per `;`: comment lines and
/// preprocessor directives dropped, trailing comments stripped. A declaration
/// can span lines (`extern int f(\n    int a,\n    int b);`) and stays whole.
fn declaration_statements(decl: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut in_block_comment = false;
    let mut start: Option<usize> = None;
    for (offset, line) in line_spans(decl) {
        let trimmed = line.trim();
        if in_block_comment {
            in_block_comment = !trimmed.contains("*/");
            continue;
        }
        if start.is_none() {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if is_comment_line(trimmed) {
                in_block_comment = trimmed.starts_with("/*") && !trimmed.contains("*/");
                continue;
            }
            start = Some(offset + (line.len() - line.trim_start().len()));
        }
        let code = strip_trailing_comment(trimmed);
        if code.ends_with(';')
            && let Some(begin) = start.take()
        {
            let end = offset + (line.len() - line.trim_start().len()) + code.len();
            statements.push(&decl[begin..end]);
        }
    }
    if let Some(begin) = start {
        statements.push(strip_trailing_comment(&decl[begin..]));
    }
    statements
}

/// The declaration a record makes, without the comments ahead of it.
fn declaration_code(decl: &str) -> &str {
    let mut rest = decl.trim_start();
    loop {
        if rest.starts_with("//") {
            rest = rest.find('\n').map_or("", |newline| &rest[newline + 1..]);
        } else if let Some(comment) = rest.strip_prefix("/*") {
            rest = comment.find("*/").map_or("", |close| &comment[close + 2..]);
        } else {
            break;
        }
        rest = rest.trim_start();
    }
    strip_trailing_comment(rest)
}

/// The definition header of the function a body opens, comments skipped.
fn definition_line(body: &str) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut i = 0usize;
    let mut start: Option<usize> = None;
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        if let Some(q) = quote {
            if bytes[i] == b'\\' {
                i = (i + 2).min(bytes.len());
            } else if bytes[i] == q {
                quote = None;
                i += 1;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if start.is_none() {
            start = Some(i);
        }
        if bytes[i] == b'{' {
            let header = body[start.unwrap()..i].trim_end();
            return (!header.is_empty()).then_some(header);
        }
        if matches!(bytes[i], b'"' | b'\'') {
            quote = Some(bytes[i]);
        }
        i += 1;
    }

    // No body opened: the header is the line the first code starts on. A
    // render that is only comments has none.
    let start = start?;
    body[start..].lines().next().map(str::trim_end)
}

/// The name a function-style `extern` declaration declares, whether its
/// parameter list is typed or has unspecified arity. Function-pointer
/// declarations are excluded because their first parenthesis opens the
/// pointer declarator rather than the function's parameter list.
///
/// A conditional group declares one function when every branch declares the
/// same one: that is how an unknown-arity callee is spelled for C17 and C23.
fn extern_function_name(decl: &str) -> Option<&str> {
    let mut name = None;
    for line in declaration_statements(decl) {
        let (_, this, _) = parse_extern_function(line)?;
        if name.is_some_and(|name| name != this) {
            return None;
        }
        name = Some(this);
    }
    name
}

/// `extern <return type> <name>(<params>);` taken apart.
fn parse_extern_function(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.trim().strip_prefix("extern ")?;
    let open = rest.find('(')?;
    if rest[open..].starts_with("(*") {
        return None;
    }
    let name = identifier_before(&rest[..open])?;
    let return_type = rest[..open].trim_end().strip_suffix(name)?.trim();
    // Up to the last `)`, so a parameter that is itself a function pointer
    // stays whole; trailing attributes, if any, only make the list differ.
    let close = rest.rfind(')').filter(|close| *close > open)?;
    Some((return_type, name, rest[open + 1..close].trim()))
}

/// How much one render's declaration of a callee says about its parameters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExternEvidence {
    /// A declared signature or a modeled runtime prototype.
    Declared,
    /// A parameter list read off that render's call sites.
    Inferred,
    /// No parameter list: `()` / `(...)`.
    UnknownArity,
}

fn extern_evidence(decl: &str) -> ExternEvidence {
    let unknown = declaration_statements(decl).into_iter().all(|line| {
        parse_extern_function(line).is_some_and(|(_, _, params)| matches!(params, "" | "..."))
    });
    if unknown {
        ExternEvidence::UnknownArity
    } else if decl.contains(fission_decompiler::INFERRED_PROTOTYPE_MARKER) {
        ExternEvidence::Inferred
    } else {
        ExternEvidence::Declared
    }
}

/// One declaration for a callee the unit does not define, from every render's.
///
/// Each render infers a callee's parameters from its own call sites, so two
/// renders can disagree -- `f(int)` here, `f(int, int)` there -- and keeping
/// either one turns the other's call into a compile error, in C17 as much as
/// C23. Where the inferred lists do not agree, the unit declares what all of
/// them do agree on: the callee takes some arguments. A declared signature is
/// evidence rather than a view of call sites, and is kept over both. Return
/// types disagree for the same reason and, as before, the first is kept.
fn merge_extern_declarations(name: &str, variants: &[&str]) -> String {
    let first = variants[0];
    if let Some(declared) = variants
        .iter()
        .find(|decl| extern_evidence(decl) == ExternEvidence::Declared)
    {
        return (*declared).to_owned();
    }
    let parameter_lists: HashSet<&str> = variants
        .iter()
        .flat_map(|decl| declaration_statements(decl))
        .filter_map(|line| parse_extern_function(line).map(|(_, _, params)| params))
        .collect();
    let agree = variants
        .iter()
        .all(|decl| extern_evidence(decl) == extern_evidence(first))
        && (extern_evidence(first) == ExternEvidence::UnknownArity || parameter_lists.len() == 1);
    if agree {
        return first.to_owned();
    }
    let return_type = declaration_statements(first)
        .into_iter()
        .find_map(parse_extern_function)
        .map_or("unsigned long long", |(return_type, _, _)| return_type);
    fission_decompiler::print_unknown_arity_extern(return_type, name)
        .trim_end()
        .to_owned()
}

/// The name a `typedef ... <name>;` record defines, however many lines it took.
/// Trailing GNU attributes are not part of the typedef name.
fn typedef_name(decl: &str) -> Option<&str> {
    let rest = declaration_code(decl).strip_prefix("typedef ")?;
    let definition = rest.trim_end().strip_suffix(';')?;
    let definition = definition
        .split_once(" __attribute__")
        .map_or(definition, |(definition, _)| definition);
    identifier_before(definition)
}

/// The name a `<type> <name>;` data declaration declares.
fn global_name(decl: &str) -> Option<&str> {
    identifier_before(declaration_code(decl).strip_suffix(';')?)
}

/// The name a function definition line declares.
fn declared_function_name(line: &str) -> Option<&str> {
    let open = line.find('(')?;
    identifier_before(&line[..open])
}

/// The trailing C identifier of `text`, if it ends in one.
///
/// The split point is the byte *after* the last non-identifier character, and
/// that character is not always one byte: a Go symbol carries `·`, two bytes,
/// and `index + 1` landed inside it. Assembling a Go binary's translation
/// unit panicked with `start byte index 46 is not a char boundary`.
fn identifier_before(text: &str) -> Option<&str> {
    let end = text.trim_end();
    let start = end
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map(|(index, c)| index + c.len_utf8())
        .unwrap_or(0);
    let name = &end[start..];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Go symbol carries `·`, which is two bytes.
    ///
    /// The identifier split used `index + 1` after the last non-identifier
    /// character, which landed inside it: assembling a Go binary's unit
    /// panicked with `start byte index 46 is not a char boundary`.
    #[test]
    fn a_multi_byte_character_before_a_name_does_not_split_it() {
        let unit = assemble(&[
            "void main·main(void)\n{\n    return;\n}\n".to_string(),
            "extern unsigned long long main·main();\n\nvoid caller(void)\n{\n    main·main();\n}\n"
                .to_string(),
        ]);
        // The extern is dropped because the unit defines the function -- which
        // only works if the name was read past the `·`.
        assert!(
            !unit.code.contains("extern unsigned long long main·main();"),
            "{}",
            unit.code
        );
        assert!(unit.code.contains("void main·main(void);"), "{}", unit.code);
    }

    #[test]
    fn unit_drops_an_extern_for_a_function_it_defines() {
        let unit = assemble(&[
            "extern unsigned long long helper();\n\nvoid caller(void)\n{\n    helper();\n}\n"
                .to_string(),
            "void helper(void)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(
            !unit.code.contains("extern unsigned long long helper();"),
            "{}",
            unit.code
        );
        // It still needs a declaration before the call -- the definition's own.
        assert!(unit.code.contains("void helper(void);"), "{}", unit.code);
    }

    #[test]
    fn project_keeps_c11_unspecified_parameter_list_for_import_thunk() {
        let unit = assemble(&[
            "extern unsigned long long sub_11c0();\n".to_string(),
            "void caller(void)\n{\n    sub_11c0(1);\n}\n".to_string(),
        ]);

        assert!(
            unit.code.contains("unsigned long long sub_11c0();"),
            "{}",
            unit.code
        );
        assert!(!unit.code.contains("sub_11c0()\n{"), "{}", unit.code);
        assert!(!unit.code.contains("sub_11c0(...)"), "{}", unit.code);
    }

    #[test]
    fn unit_drops_a_typed_variadic_extern_when_the_function_is_defined() {
        let unit = assemble(&[
            "extern int fprintf(FILE* __stream, const char* __format, ...);\n\nvoid caller(void)\n{\n    fprintf(stderr, \"%d\", 1);\n}\n".to_string(),
            "int fprintf(FILE* __stream, const char* __format, ...)\n{\n    return 0;\n}\n"
                .to_string(),
        ]);
        assert!(
            !unit
                .code
                .contains("extern int fprintf(FILE* __stream, const char* __format, ...);"),
            "{}",
            unit.code
        );
        assert!(
            unit.code
                .contains("int fprintf(FILE* __stream, const char* __format, ...);"),
            "{}",
            unit.code
        );
    }

    #[test]
    fn typed_extern_function_pointer_is_not_mistaken_for_a_function_prototype() {
        assert_eq!(extern_function_name("extern int (*callback)(int);"), None);
    }

    /// Two renders can disagree about an undefined callee's return type, and
    /// keeping both declarations is a compile error rather than extra
    /// information. A Go binary produced `extern uchar __popcount();` and
    /// `extern unsigned long long __popcount();` in one unit.
    #[test]
    fn one_extern_survives_when_two_renders_declare_a_callee_differently() {
        let unit = assemble(&[
            "extern uchar helper();\n\nvoid a(void)\n{\n    helper();\n}\n".to_string(),
            "extern unsigned long long helper();\n\nvoid b(void)\n{\n    helper();\n}\n"
                .to_string(),
        ]);
        assert_eq!(
            unit.code.matches("helper();").count() - 2,
            1,
            "{}",
            unit.code
        );
        assert!(
            unit.code.contains("extern uchar helper();"),
            "{}",
            unit.code
        );
        assert!(
            !unit.code.contains("extern unsigned long long helper();"),
            "{}",
            unit.code
        );
    }

    #[test]
    fn unit_emits_one_typedef_when_two_functions_need_the_same_one() {
        let render = |name: &str| {
            format!(
                "typedef unsigned long long HANDLE;\n\nvoid {name}(HANDLE h)\n{{\n    return;\n}}\n"
            )
        };
        let unit = assemble(&[render("first"), render("second")]);
        assert_eq!(
            unit.code
                .matches("typedef unsigned long long HANDLE;")
                .count(),
            1,
            "{}",
            unit.code
        );
    }

    #[test]
    fn unit_keeps_the_first_spelling_when_a_typedef_name_disagrees() {
        let unit = assemble(&[
            "typedef unsigned long long FILE;\n\nvoid a(FILE f)\n{\n    return;\n}\n".to_string(),
            "typedef long long FILE;\n\nvoid b(FILE f)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(
            unit.code.contains("typedef unsigned long long FILE;"),
            "{}",
            unit.code
        );
        assert!(
            !unit.code.contains("typedef long long FILE;"),
            "{}",
            unit.code
        );
    }

    #[test]
    fn unit_preserves_conflicting_aggregate_shapes_and_rewrites_their_uses() {
        let unit = assemble(&[
            concat!(
                "typedef struct fission_agg16 {\n",
                "    unsigned int value;\n",
                "    unsigned char _pad_4[4];\n",
                "    unsigned long long next;\n",
                "} fission_agg16;\n\n",
                "typedef fission_agg16 Node;\n\n",
                "int list_sum(const Node *head)\n{\n    return head->value;\n}\n"
            )
            .to_string(),
            concat!(
                "typedef struct fission_agg16 {\n",
                "    uint field_0;\n",
                "    unsigned char _pad_4[4];\n",
                "    unsigned long long field_8;\n",
                "} fission_agg16;\n\n",
                "typedef fission_agg16 Node;\n\n",
                "uint inspect(const Node *p)\n{\n    return p->field_8;\n}\n"
            )
            .to_string(),
        ])
        .code;

        assert!(unit.contains("} fission_agg16;"), "{unit}");
        assert!(unit.contains("} fission_agg16_variant_2;"), "{unit}");
        assert!(unit.contains("typedef fission_agg16 Node;"), "{unit}");
        assert!(
            unit.contains("typedef fission_agg16_variant_2 Node_variant_2;"),
            "{unit}"
        );
        assert!(
            unit.contains("uint inspect(const Node_variant_2 *p)"),
            "{unit}"
        );
        assert!(unit.contains("return p->field_8;"), "{unit}");
    }

    #[test]
    fn unit_disambiguates_attribute_qualified_aggregate_typedefs() {
        let unit = assemble(&[
            concat!(
                "typedef struct fission_agg16 {\n",
                "    uint value;\n",
                "    unsigned char _pad_4[4];\n",
                "    unsigned long long next;\n",
                "} fission_agg16;\n\n",
                "void use_struct(fission_agg16 *p)\n{\n    return;\n}\n"
            )
            .to_string(),
            concat!(
                "typedef unsigned __int128 fission_agg16 __attribute__((aligned(1), may_alias));\n\n",
                "fission_agg16 opaque_value(void)\n{\n    return 0;\n}\n"
            )
            .to_string(),
        ])
        .code;

        assert!(unit.contains("} fission_agg16;"), "{unit}");
        assert!(
            unit.contains("typedef unsigned __int128 fission_agg16_variant_2"),
            "{unit}"
        );
        assert!(
            unit.contains("fission_agg16_variant_2 opaque_value(void)"),
            "{unit}"
        );
    }

    #[test]
    fn unit_disambiguates_aggregate_typedefs_that_depend_on_a_renamed_shape() {
        let unit = assemble(&[
            concat!(
                "typedef struct fission_agg8 { uint first; } fission_agg8;\n",
                "typedef struct fission_agg16 {\n",
                "    fission_agg8 *child;\n",
                "    uint count;\n",
                "} fission_agg16;\n\n",
                "uint first(fission_agg16 *p)\n{\n    return p->count;\n}\n"
            )
            .to_string(),
            concat!(
                "typedef struct fission_agg8 { uint second; } fission_agg8;\n",
                "typedef struct fission_agg16 {\n",
                "    fission_agg8 *child;\n",
                "    uint count;\n",
                "} fission_agg16;\n\n",
                "uint second(fission_agg16 *p)\n{\n    return p->count;\n}\n"
            )
            .to_string(),
        ])
        .code;

        assert!(unit.contains("} fission_agg8_variant_2;"), "{unit}");
        assert!(unit.contains("} fission_agg16_variant_2;"), "{unit}");
        assert!(unit.contains("fission_agg8_variant_2 *child;"), "{unit}");
        assert!(
            unit.contains("uint second(fission_agg16_variant_2 *p)"),
            "{unit}"
        );
    }

    #[test]
    fn typedef_renames_skip_comments_strings_and_extended_symbol_names() {
        let replacements = HashMap::from([(
            "fission_agg16".to_string(),
            "fission_agg16_variant_2".to_string(),
        )]);
        let source = concat!(
            "fission_agg16 *value;\n",
            "const char *text = \"fission_agg16\"; // fission_agg16\n",
            "void main·fission_agg16(void);\n"
        );

        let rewritten = replace_c_identifiers(source, &replacements);

        assert!(rewritten.contains("fission_agg16_variant_2 *value;"));
        assert!(rewritten.contains("\"fission_agg16\"; // fission_agg16"));
        assert!(rewritten.contains("main·fission_agg16(void)"));
    }

    #[test]
    fn unit_drops_a_data_declaration_naming_a_function_it_defines() {
        let unit = assemble(&[
            "unsigned long long handler;\n\nvoid uses(void)\n{\n    take(handler);\n}\n"
                .to_string(),
            "void handler(void)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(
            !unit.code.contains("unsigned long long handler;"),
            "{}",
            unit.code
        );
        assert!(unit.code.contains("void handler(void);"), "{}", unit.code);
    }

    #[test]
    fn unit_splits_a_multi_line_aggregate_typedef_from_the_body() {
        let unit = assemble(&[concat!(
            "typedef struct fission_agg16 {\n",
            "    unsigned int field_0;\n",
            "} fission_agg16;\n",
            "\n",
            "void takes(fission_agg16 v)\n{\n    return;\n}\n"
        )
        .to_string()]);
        let prelude_end = unit
            .code
            .find("void takes(fission_agg16 v)\n{")
            .expect("body");
        assert!(
            unit.code[..prelude_end].contains("} fission_agg16;"),
            "{}",
            unit.code
        );
    }

    #[test]
    fn conflicting_global_declarations_emit_one_name_and_structured_incomplete_diagnostic() {
        let assembly = assemble(&[
            "uint * tmp_140007020;\n\nvoid reads_as_pointer(void)\n{\n    take(tmp_140007020);\n}\n"
                .to_string(),
            "uint tmp_140007020;\n\nvoid reads_as_scalar(void)\n{\n    take(tmp_140007020);\n}\n"
                .to_string(),
        ]);

        assert_eq!(
            assembly.code.matches("tmp_140007020;").count(),
            1,
            "{}",
            assembly.code
        );
        assert!(
            assembly.code.contains("uint * tmp_140007020;"),
            "{}",
            assembly.code
        );
        assert!(
            !assembly.code.contains("uint tmp_140007020;"),
            "{}",
            assembly.code
        );
        assert_eq!(assembly.diagnostics.len(), 1);
        let diagnostic = &assembly.diagnostics[0];
        assert_eq!(diagnostic.code, "project_global_declaration_disagreement");
        assert_eq!(diagnostic.analysis_status, "incomplete");
        assert_eq!(diagnostic.global_name, "tmp_140007020");
        assert_eq!(diagnostic.resolution, "first_declaration_emitted");
        assert_eq!(diagnostic.declarations.len(), 2);
        assert_eq!(
            diagnostic.declarations[0].function.as_deref(),
            Some("reads_as_pointer")
        );
        assert_eq!(
            diagnostic.declarations[1].function.as_deref(),
            Some("reads_as_scalar")
        );

        let json = serde_json::to_value(diagnostic).expect("diagnostic serializes");
        assert_eq!(json["analysis_status"], "incomplete");
        assert_eq!(json["global_name"], "tmp_140007020");
        assert_eq!(
            json["declarations"][1]["declaration"],
            "uint tmp_140007020;"
        );
    }

    #[test]
    fn matching_global_declarations_are_deduplicated_without_a_conflict() {
        let declaration = "uint tmp_140007020;\n\n";
        let render = |name: &str| {
            format!("{declaration}void {name}(void)\n{{\n    take(tmp_140007020);\n}}\n")
        };
        let assembly = assemble(&[render("first"), render("second")]);

        assert_eq!(assembly.code.matches("uint tmp_140007020;").count(), 1);
        assert!(assembly.diagnostics.is_empty());
    }

    /// The lines of one `// -- title --` section of an assembled unit.
    fn unit_section<'a>(code: &'a str, title: &str) -> Vec<&'a str> {
        let header = format!("// -- {title} --");
        code.lines()
            .skip_while(|line| *line != header)
            .skip(1)
            .take_while(|line| !line.starts_with("// -- ") && !line.is_empty())
            .collect()
    }

    fn caller(name: &str, declaration: &str, call: &str) -> String {
        format!("{declaration}\nvoid {name}(void)\n{{\n    {call};\n}}\n")
    }

    fn unknown_arity(return_type: &str, name: &str) -> String {
        fission_decompiler::print_unknown_arity_extern(return_type, name)
    }

    fn inferred(return_type: &str, name: &str, params: &[&str]) -> String {
        let params: Vec<String> = params.iter().map(|param| param.to_string()).collect();
        fission_decompiler::print_inferred_extern(return_type, name, &params)
    }

    /// The renderer declares an unknown-arity callee as an `#if` .. `#endif`
    /// group, once per C dialect. Its first line takes a parameter list and
    /// does not end in `;` -- the shape of a definition line -- so the unit
    /// registered `defined` as a function it defines and put an unterminated
    /// `#if` among the prototypes.
    #[test]
    fn an_unknown_arity_declaration_group_stays_one_declaration() {
        let unit = assemble(&[
            caller("first", &unknown_arity("int", "opaque"), "opaque(1)"),
            caller("second", &unknown_arity("int", "opaque"), "opaque(1, 2)"),
            // An import thunk renders as the declaration alone.
            unknown_arity("unsigned long long", "thunk"),
        ])
        .code;

        assert_eq!(
            unit_section(&unit, "functions defined in this unit"),
            ["void first(void);", "void second(void);"],
            "{unit}"
        );
        assert!(
            !unit
                .lines()
                .any(|line| line.starts_with('#') && line.ends_with(';')),
            "{unit}"
        );
        let externs = unit_section(&unit, "external functions").join("\n");
        assert_eq!(
            externs,
            format!(
                "{}\n{}",
                unknown_arity("int", "opaque").trim_end(),
                unknown_arity("unsigned long long", "thunk").trim_end()
            ),
            "{unit}"
        );
        assert_eq!(
            unit.matches("#if ").count(),
            unit.matches("#endif").count(),
            "{unit}"
        );
        // Each group's lines are contiguous and close before any body.
        assert!(unit.find("#endif").unwrap() < unit.find("void first(void)\n{").unwrap());
    }

    /// An inferred prototype carries a trailing comment, so its line does not
    /// end in `;`. The unit read it as the definition of the callee: it counted
    /// the callee as defined and dropped every other render's extern for it.
    #[test]
    fn an_inferred_declaration_with_a_trailing_comment_is_not_a_definition() {
        let unit = assemble(&[
            caller("first", &inferred("int", "helper", &["int"]), "helper(1)"),
            caller("second", &inferred("int", "helper", &["int"]), "helper(2)"),
        ])
        .code;

        assert_eq!(
            unit_section(&unit, "functions defined in this unit"),
            ["void first(void);", "void second(void);"],
            "{unit}"
        );
        assert_eq!(
            unit_section(&unit, "external functions"),
            [inferred("int", "helper", &["int"]).trim_end()],
            "{unit}"
        );
    }

    /// Each render infers a callee's parameters from its own calls. Keeping
    /// the first render's `helper(int)` makes the other's `helper(1, 2)` a
    /// compile error in C17 as well as C23, so disagreeing inferences, and an
    /// inference that disagrees with an unknown-arity view, declare the
    /// callee with unknown arity.
    #[test]
    fn disagreeing_inferred_arities_declare_the_callee_with_unknown_arity() {
        let unit = assemble(&[
            caller("first", &inferred("int", "helper", &["int"]), "helper(1)"),
            caller(
                "second",
                &inferred("long long", "helper", &["int", "int"]),
                "helper(1, 2)",
            ),
            caller("third", &inferred("int", "mixed", &["int"]), "mixed(1)"),
            caller("fourth", &unknown_arity("int", "mixed"), "mixed()"),
        ])
        .code;

        assert_eq!(
            unit_section(&unit, "external functions").join("\n"),
            format!(
                "{}\n{}",
                unknown_arity("int", "helper").trim_end(),
                unknown_arity("int", "mixed").trim_end()
            ),
            "{unit}"
        );
    }

    /// A declared signature is evidence, not one render's view of its calls,
    /// so it is kept over inferred lists wherever it appears.
    #[test]
    fn a_declared_signature_outranks_inferred_parameter_lists() {
        let unit = assemble(&[
            caller(
                "first",
                &inferred("int", "puts_like", &["int"]),
                "puts_like(1)",
            ),
            caller(
                "second",
                "extern int puts_like(const char *s);\n",
                "puts_like(\"x\")",
            ),
        ])
        .code;

        assert_eq!(
            unit_section(&unit, "external functions"),
            ["extern int puts_like(const char *s);"],
            "{unit}"
        );
    }

    /// The assembled unit from the cases above compiles in both dialects the
    /// declarations are written for. Skipped, loudly, without a C compiler.
    #[test]
    fn a_unit_with_inferred_and_unknown_arity_callees_compiles_as_c17_and_c23() {
        let unit = assemble(&[
            caller("first", &inferred("int", "helper", &["int"]), "helper(1)"),
            caller(
                "second",
                &inferred("int", "helper", &["int", "int"]),
                "helper(1, 2)",
            ),
            caller("third", &unknown_arity("int", "opaque"), "opaque(1)"),
            caller("fourth", &inferred("int", "agreed", &["int"]), "agreed(1)"),
            caller("fifth", &inferred("int", "agreed", &["int"]), "agreed(2)"),
            unknown_arity("unsigned long long", "thunk"),
            caller("sixth", "extern unsigned long long thunk();\n", "thunk(3)"),
        ])
        .code;

        let dir = std::env::temp_dir().join(format!(
            "fission-unit-compile-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let source = dir.join("unit.c");
        std::fs::write(&source, &unit).expect("write unit");
        // GCC before 14 (Ubuntu 24.04's default) names C23 `gnu2x`; it already
        // reads `()` as `(void)` and accepts a bare `(...)` there.
        for spellings in [&["gnu17"][..], &["gnu23", "gnu2x"][..]] {
            let mut compiled = false;
            for std in spellings {
                let output = match std::process::Command::new("cc")
                    .arg("-fsyntax-only")
                    .arg(format!("-std={std}"))
                    .arg(&source)
                    .output()
                {
                    Ok(output) => output,
                    Err(error) => {
                        eprintln!("SKIPPED: no C compiler (`cc`): {error}");
                        return;
                    }
                };
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !output.status.success() && stderr.contains("std=") {
                    eprintln!("`cc` does not accept -std={std}: {stderr}");
                    continue;
                }
                assert!(output.status.success(), "-std={std}:\n{stderr}\n{unit}");
                compiled = true;
                break;
            }
            if !compiled {
                eprintln!("SKIPPED: `cc` accepts none of {spellings:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_render_tracks_exact_crlf_offsets_and_skips_comments_and_wrapped_externs() {
        let input = concat!(
            "// Preamble comment (with parens)\r\n",
            "/* Block comment\r\n",
            "   also (with parens) */\r\n",
            "typedef struct Box {\r\n",
            "    int val;\r\n",
            "} Box;\r\n",
            "\r\n",
            "extern int wrapped_callee(\r\n",
            "    int a,\r\n",
            "    int b);\r\n",
            "\r\n",
            "// Function note (before body)\r\n",
            "void caller(Box box)\r\n",
            "{\r\n",
            "    wrapped_callee(box.val, 1);\r\n",
            "}\r\n",
        );

        let split = split_render(input);
        assert_eq!(split.declarations.len(), 2, "{:?}", split.declarations);
        assert!(split.declarations[0].contains("} Box;"));
        assert!(split.declarations[1].contains("extern int wrapped_callee("));
        assert!(
            split.body.starts_with("void caller(Box box)\r\n{"),
            "body offset drifted on CRLF:\n{:?}",
            split.body
        );

        let unit = assemble(&[
            input.to_string(),
            "int wrapped_callee(int a, int b)\r\n{\r\n    return a + b;\r\n}\r\n".to_string(),
        ]);
        assert!(
            !unit.code.contains("extern int wrapped_callee("),
            "{}",
            unit.code
        );
        assert!(unit.code.contains("void caller(Box box);"), "{}", unit.code);
        assert!(
            unit.code.contains("int wrapped_callee(int a, int b);"),
            "{}",
            unit.code
        );
    }

    #[test]
    fn split_render_handles_multiline_function_signature() {
        let input = concat!(
            "extern int helper();\n\n",
            "int compute(\n",
            "    int first,\n",
            "    int second)\n",
            "{\n",
            "    return helper();\n",
            "}\n",
        );

        let split = split_render(input);
        assert_eq!(split.declarations, vec!["extern int helper();"]);
        assert!(split.body.starts_with("int compute(\n"), "{:?}", split.body);

        let unit = assemble(&[input.to_string()]);
        assert!(
            unit.code
                .contains("int compute(\n    int first,\n    int second);"),
            "{}",
            unit.code
        );
    }
}
