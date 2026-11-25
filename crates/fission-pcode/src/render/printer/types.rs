//! C type spelling: binding declarations, surface type names, byte sizes,
//! `print_type` and the return-type rule.

use super::*;

pub(super) fn print_binding_type(binding: &NirBinding) -> String {
    binding
        .surface_type_name
        .clone()
        .filter(|surface| surface_type_is_definable(surface, &binding.ty))
        .unwrap_or_else(|| print_type(&binding.ty))
}

pub(super) fn print_binding_declaration(binding: &NirBinding) -> String {
    if let Some((base, suffix)) = surface_array_declarator(binding.surface_type_name.as_deref()) {
        return format!("{base} {}{suffix}", binding.name);
    }
    format!("{} {}", print_binding_type(binding), binding.name)
}

pub(super) fn surface_array_declarator(surface: Option<&str>) -> Option<(&str, &str)> {
    let surface = surface?.trim();
    let open = surface.rfind('[')?;
    let suffix = surface[open..].strip_suffix(']')?;
    if suffix[1..].trim().is_empty() || surface[..open].contains('[') {
        return None;
    }
    let base = surface[..open].trim_end();
    (!base.is_empty()).then_some((base, &surface[open..]))
}

pub(crate) fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } | NirType::Float { bits } if *bits > 0 && bits % 8 == 0 => {
            Some(bits / 8)
        }
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Unknown => None,
        NirType::Int { .. } | NirType::Float { .. } => None,
    }
}

/// Return the byte width of a primitive pointee spelled in a surface
/// declaration. Named library types intentionally return `None`: their
/// definition may be supplied by the project prelude and the recovered
/// internal type remains the authority for those accesses.
pub(super) fn surface_pointee_byte_size(declaration: &str) -> Option<u32> {
    let (base, stars) = declaration.split_once('*')?;
    if stars.contains('*') {
        return Some(8);
    }
    let base = base
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile"))
        .collect::<Vec<_>>()
        .join(" ");
    match base.as_str() {
        "char" | "signed char" | "unsigned char" | "int8_t" | "uint8_t" | "uchar" => Some(1),
        "short" | "signed short" | "signed short int" | "unsigned short" | "unsigned short int"
        | "int16_t" | "uint16_t" | "ushort" => Some(2),
        "int" | "signed" | "signed int" | "unsigned" | "unsigned int" | "int32_t" | "uint32_t"
        | "uint" => Some(4),
        "long long"
        | "signed long long"
        | "signed long long int"
        | "unsigned long long"
        | "unsigned long long int"
        | "int64_t"
        | "uint64_t"
        | "ulong" => Some(8),
        "float" => Some(4),
        "double" => Some(8),
        _ => None,
    }
}

/// The recovered type a kept `surface_type_name` would be defined from.
///
/// The type library supplies names it does not supply definitions for -- a
/// declaration reading `STARTUPINFOA *lpStartupInfo` with no struct behind it.
/// The name alone invalidates its declaration and every use of the variable
/// with it: one such name accounted for 13 of the first whole-binary compile's
/// 20 errors, most of them `arithmetic on a pointer to an incomplete type`.
///
/// Dropping every library name would fix that and cost every `HWND` and
/// `LPRECT` in the output. It is not necessary, because the emitted unit can
/// define these names itself, from the type it recovered: `typedef unsigned
/// long long HWND;`. That spelling is exact -- same width, same pointer
/// stride, same arithmetic -- so the name stays pure annotation and cannot
/// change what the code does, which is the property a rebuild needs.
///
/// This returns the type the definition would be written from: the surface's
/// trailing stars peeled off `ty`, since those stars belong to the variable
/// and not to the name.
pub(crate) fn surface_type_definition_source<'a>(
    surface: &str,
    ty: &'a NirType,
) -> Option<&'a NirType> {
    let stars = surface.chars().filter(|c| *c == '*').count();
    let mut core = ty;
    for _ in 0..stars {
        // `ty` does not always model every level the surface spells. Where it
        // stops, it is describing what the name stands for rather than the
        // variable, so that is the definition source and peeling ends.
        let NirType::Ptr(pointee) = core else {
            break;
        };
        core = pointee;
    }
    Some(core)
}

/// Whether a `surface_type_name` can carry a declaration in emitted C.
pub(super) fn surface_type_is_definable(surface: &str, ty: &NirType) -> bool {
    let base = surface
        .trim_end_matches(|c: char| c == '*' || c.is_whitespace())
        .trim();
    base.is_empty()
        || is_self_describing_c_type(base)
        || surface_type_definition_source(surface, ty).is_some()
}

/// Whether every word of `base` is a type C or the emitted prelude provides.
pub(super) fn is_self_describing_c_type(base: &str) -> bool {
    base.starts_with("fission_agg")
        || base.split_whitespace().all(|word| {
            matches!(
                word,
                "void"
                    | "char"
                    | "short"
                    | "int"
                    | "long"
                    | "float"
                    | "double"
                    | "signed"
                    | "unsigned"
                    | "_Bool"
                    | "bool"
                    | "size_t"
                    | "ptrdiff_t"
                    | "intptr_t"
                    | "uintptr_t"
                    | "uint"
                    | "ushort"
                    | "uchar"
                    | "ulong"
                    | "int8_t"
                    | "int16_t"
                    | "int32_t"
                    | "int64_t"
                    | "uint8_t"
                    | "uint16_t"
                    | "uint32_t"
                    | "uint64_t"
                    | "__int128"
                    | "int128"
                    | "undefined"
                    | "undefined1"
                    | "undefined2"
                    | "undefined4"
                    | "undefined8"
                    | "const"
                    | "volatile"
            )
        })
}

/// A function's return type, which unlike a value's cannot be left open.
///
/// `print_type` renders `NirType::Unknown` as `undefined`, which is not a C
/// type and states no width. On a *value* that is at worst uninformative; in a
/// signature it decides what every caller does with the result. The
/// benchmark's uniform recompilation fixup resolves `undefined` to `unsigned
/// char`, so a function whose return we could not determine returns one byte
/// and every call site truncates.
///
/// Word width instead. It is the same answer `opaque_pcodeop_return_type_name`
/// already gives for the same question one layer down, and it is never
/// narrower than the value a caller reads.
pub(super) fn print_return_type(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "unsigned long long".to_string(),
        _ => print_type(ty),
    }
}

pub(crate) fn print_type(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "undefined".to_string(),
        NirType::Bool => "bool".to_string(),
        NirType::Int { bits, signed } => match (*bits, *signed) {
            (8, false) => "uchar".to_string(),
            (8, true) => "char".to_string(),
            (16, false) => "ushort".to_string(),
            (16, true) => "short".to_string(),
            (32, false) => "uint".to_string(),
            (32, true) => "int".to_string(),
            (64, false) => "unsigned long long".to_string(),
            (64, true) => "long long".to_string(),
            (128, false) => "unsigned __int128".to_string(),
            (128, true) => "__int128".to_string(),
            _ => format!("int{}", bits),
        },
        NirType::Ptr(inner) if matches!(inner.as_ref(), NirType::Unknown) => "void *".to_string(),
        NirType::Ptr(inner) => format!("{} *", print_type(inner)),
        NirType::Aggregate { size, .. } => format!("fission_agg{}", size),
        NirType::Float { bits } => match *bits {
            32 => "float".to_string(),
            64 => "double".to_string(),
            80 => "long double".to_string(),
            _ => format!("float{}", bits),
        },
    }
}
