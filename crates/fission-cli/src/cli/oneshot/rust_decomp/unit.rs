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
/// them has to define them, and nothing else in the pipeline does.
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
typedef _Bool bool;
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

/// Assemble `renders` (in emission order) into a single translation unit.
pub(crate) fn assemble(renders: &[String]) -> String {
    let split: Vec<SplitRender<'_>> = renders.iter().map(|text| split_render(text)).collect();

    let defined: HashSet<&str> = split
        .iter()
        .filter_map(|render| definition_line(render.body))
        .filter_map(declared_function_name)
        .collect();

    let mut typedefs: Vec<&str> = Vec::new();
    let mut aggregates: Vec<&str> = Vec::new();
    let mut globals: Vec<&str> = Vec::new();
    let mut externs: Vec<&str> = Vec::new();
    // A typedef name may be reached two different ways in two functions. The
    // first spelling wins: a second one is a compile error, not extra
    // information, and nothing downstream can choose between them.
    let mut typedef_names: HashMap<&str, &str> = HashMap::new();
    let mut extern_names: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for render in &split {
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
                // information. Same rule the typedefs follow: first wins.
                if extern_names.insert(name) {
                    externs.push(decl);
                }
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
            if global_name(decl).is_some_and(|name| defined.contains(name)) {
                continue;
            }
            if seen.insert(decl) {
                globals.push(decl);
            }
        }
    }

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
    push_section(&mut out, "external functions", &externs);
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
    out
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

/// Split a render at the first line that opens a function definition.
///
/// Every declaration the printer emits ends in `;`, possibly after a braced
/// body it opened on an earlier line (`typedef struct ... {`). A function
/// definition is the first line that takes a parameter list without ending in
/// one -- that is what separates `void f(void)` from `extern void f();`.
fn split_render(text: &str) -> SplitRender<'_> {
    let mut depth = 0usize;
    let mut offset = 0usize;
    let mut declarations: Vec<String> = Vec::new();
    let mut record = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if depth == 0 && trimmed.contains('(') && !trimmed.ends_with(';') {
            return SplitRender {
                declarations,
                body: &text[offset..],
            };
        }
        depth += trimmed.matches('{').count();
        depth = depth.saturating_sub(trimmed.matches('}').count());
        if !trimmed.is_empty() {
            if !record.is_empty() {
                record.push('\n');
            }
            record.push_str(line);
            if depth == 0 {
                declarations.push(std::mem::take(&mut record));
            }
        }
        offset += line.len() + 1;
    }
    SplitRender {
        declarations,
        body: "",
    }
}

/// The definition line of the function a body opens, comments skipped.
fn definition_line(body: &str) -> Option<&str> {
    body.lines()
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with("//"))
}

/// The name a `extern <type> <name>();` declaration declares.
fn extern_function_name(decl: &str) -> Option<&str> {
    let rest = decl.trim().strip_prefix("extern ")?;
    let open = rest.find('(')?;
    if !rest[open..].starts_with("()") {
        return None;
    }
    identifier_before(&rest[..open])
}

/// The name a `typedef ... <name>;` record defines, however many lines it took.
fn typedef_name(decl: &str) -> Option<&str> {
    let rest = decl.trim_start().strip_prefix("typedef ")?;
    identifier_before(rest.trim_end().strip_suffix(';')?)
}

/// The name a `<type> <name>;` data declaration declares.
fn global_name(decl: &str) -> Option<&str> {
    identifier_before(decl.trim_end().strip_suffix(';')?)
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
            !unit.contains("extern unsigned long long main·main();"),
            "{unit}"
        );
        assert!(unit.contains("void main·main(void);"), "{unit}");
    }

    #[test]
    fn unit_drops_an_extern_for_a_function_it_defines() {
        let unit = assemble(&[
            "extern unsigned long long helper();\n\nvoid caller(void)\n{\n    helper();\n}\n"
                .to_string(),
            "void helper(void)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(
            !unit.contains("extern unsigned long long helper();"),
            "{unit}"
        );
        // It still needs a declaration before the call -- the definition's own.
        assert!(unit.contains("void helper(void);"), "{unit}");
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
        assert_eq!(unit.matches("helper();").count() - 2, 1, "{unit}");
        assert!(unit.contains("extern uchar helper();"), "{unit}");
        assert!(
            !unit.contains("extern unsigned long long helper();"),
            "{unit}"
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
            unit.matches("typedef unsigned long long HANDLE;").count(),
            1,
            "{unit}"
        );
    }

    #[test]
    fn unit_keeps_the_first_spelling_when_a_typedef_name_disagrees() {
        let unit = assemble(&[
            "typedef unsigned long long FILE;\n\nvoid a(FILE f)\n{\n    return;\n}\n".to_string(),
            "typedef long long FILE;\n\nvoid b(FILE f)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(unit.contains("typedef unsigned long long FILE;"), "{unit}");
        assert!(!unit.contains("typedef long long FILE;"), "{unit}");
    }

    #[test]
    fn unit_drops_a_data_declaration_naming_a_function_it_defines() {
        let unit = assemble(&[
            "unsigned long long handler;\n\nvoid uses(void)\n{\n    take(handler);\n}\n"
                .to_string(),
            "void handler(void)\n{\n    return;\n}\n".to_string(),
        ]);
        assert!(!unit.contains("unsigned long long handler;"), "{unit}");
        assert!(unit.contains("void handler(void);"), "{unit}");
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
        let prelude_end = unit.find("void takes(fission_agg16 v)\n{").expect("body");
        assert!(unit[..prelude_end].contains("} fission_agg16;"), "{unit}");
    }
}
