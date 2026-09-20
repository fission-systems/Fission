use cpp_demangle::DemangleOptions;
use cpp_demangle::Symbol as CppSymbol;
use msvc_demangler::demangle as msvc_demangle;
use rustc_demangle::demangle as rust_demangle;
use std::collections::HashMap;
use std::process::Command;
use std::sync::OnceLock;

const SWIFT_BATCH_SIZE: usize = 256;
static SWIFT_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Demangles a symbol name if possible.
/// Supports Rust, C++ (Itanium/GNU), MSVC, and Swift.
pub fn demangle(name: &str) -> String {
    if is_swift_symbol(name) {
        if let Some(demangled) = swift_demangle(name) {
            return demangled;
        }
    }

    demangle_without_swift(name)
}

/// Demangle a collection while batching Swift symbols into a small number of
/// external invocations. Other demanglers remain in-process and retain the
/// same per-name behavior as `demangle`.
pub fn demangle_many(names: &[&str]) -> Vec<String> {
    let mut results: Vec<String> = names
        .iter()
        .map(|name| {
            if is_swift_symbol(name) {
                (*name).to_string()
            } else {
                demangle_without_swift(name)
            }
        })
        .collect();

    let mut unique_swift_names = Vec::new();
    let mut seen_swift_names = HashMap::new();
    for (index, name) in names.iter().enumerate() {
        if is_swift_symbol(name) && seen_swift_names.insert(*name, index).is_none() {
            unique_swift_names.push(*name);
        }
    }

    let demangled_swift_names = swift_demangle_batch(&unique_swift_names);
    let demangled_by_name: HashMap<&str, Option<String>> = unique_swift_names
        .into_iter()
        .zip(demangled_swift_names)
        .collect();
    for (index, name) in names.iter().enumerate() {
        if let Some(Some(demangled)) = demangled_by_name.get(name) {
            results[index] = demangled.clone();
        }
    }

    results
}

fn demangle_without_swift(name: &str) -> String {
    // 1. Rust demangling (Starts with _R or _ZN)
    if name.starts_with("_R")
        || (name.starts_with("_ZN") && (name.contains("rust") || name.contains("E")))
    {
        let demangled = rust_demangle(name).to_string();
        if demangled != name {
            return demangled;
        }
    }

    // 2. C++ (Itanium/GNU) demangling (Starts with _Z)
    if name.starts_with("_Z") {
        if let Ok(sym) = CppSymbol::new(name) {
            if let Ok(demangled) = sym.demangle(&DemangleOptions::default()) {
                return demangled;
            }
        }
    }

    // 3. MSVC demangling (Starts with ?)
    if name.starts_with('?') {
        if let Ok(demangled) = msvc_demangle(name, msvc_demangler::DemangleFlags::COMPLETE) {
            return demangled;
        }
    }

    // 4. Fallback: Check if it's Rust V0 again without checking prefix
    let demangled = rust_demangle(name).to_string();
    if demangled != name {
        return demangled;
    }

    name.to_string()
}

fn is_swift_symbol(name: &str) -> bool {
    name.starts_with("_$s")
        || name.starts_with("_$S")
        || name.starts_with("_T")
        || name.starts_with("__T")
}

fn swift_available() -> bool {
    *SWIFT_AVAILABLE.get_or_init(|| {
        Command::new("swift")
            .args(["demangle", "--version"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

/// Helper to demangle one Swift symbol using the system `swift` tool.
fn swift_demangle(name: &str) -> Option<String> {
    swift_demangle_batch(&[name]).into_iter().next().flatten()
}

/// Run Swift's demangler over a bounded batch so argv size stays predictable.
/// The compact mode emits one result per input symbol, preserving positional
/// correspondence for the caller. A failed batch is represented by `None`
/// entries and falls back to the original mangled symbol.
fn swift_demangle_batch(names: &[&str]) -> Vec<Option<String>> {
    if names.is_empty() || !swift_available() {
        return vec![None; names.len()];
    }

    let mut results = vec![None; names.len()];
    for (batch_index, batch) in names.chunks(SWIFT_BATCH_SIZE).enumerate() {
        let Ok(output) = Command::new("swift")
            .args(["demangle", "--compact", "--simplified"])
            .args(batch)
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let start = batch_index * SWIFT_BATCH_SIZE;
        for (offset, line) in String::from_utf8_lossy(&output.stdout).lines().enumerate() {
            let Some(name) = batch.get(offset) else {
                break;
            };
            let demangled = line.trim();
            if !demangled.is_empty() && demangled != *name {
                results[start + offset] = Some(demangled.to_string());
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_demangle() {
        let manged = "_RNvCs6id789_4core4main";
        assert_ne!(demangle(manged), manged);
    }

    #[test]
    fn test_cpp_demangle() {
        let manged = "_Z3fooi";
        assert_eq!(demangle(manged), "foo(int)");
    }

    #[test]
    fn test_msvc_demangle() {
        let manged = "?foo@@YAXH@Z";
        assert_eq!(demangle(manged), "void __cdecl foo(int)");
    }

    #[test]
    fn demangle_many_preserves_order_for_in_process_demanglers() {
        let names = ["_Z3fooi", "plain_name", "_RNvCs6id789_4core4main"];
        let demangled = demangle_many(&names);
        assert_eq!(demangled[0], "foo(int)");
        assert_eq!(demangled[1], "plain_name");
        assert_ne!(demangled[2], names[2]);
    }

    #[test]
    fn demangle_many_batches_swift_symbols_when_tool_is_available() {
        if !swift_available() {
            return;
        }

        let names = ["_$s4main3fooyyF", "_$s4main3baryyF"];
        assert_eq!(
            demangle_many(&names),
            vec!["foo()".to_string(), "bar()".to_string()]
        );
    }
}

/// Undo i386 PE/COFF symbol decoration.
///
/// On 32-bit x86 Windows the compiler decorates C symbols by calling
/// convention: `_name` for cdecl, `_name@N` for stdcall, `@name@N` for
/// fastcall. DWARF, and every source-level view, records the undecorated
/// `name`. 64-bit Windows does not decorate at all, which is why this is
/// gated on the pointer width rather than applied everywhere.
///
/// Measured cost of not doing it: on the DecBench dev corpus, 186 of 726
/// ground-truth functions failed to match on the leading underscore alone,
/// taking 750 of 2708 ground-truth variables (27.7%) with them -- every
/// `gcc-m32` binary scored exactly zero because all 79 of its functions were
/// decorated.
///
/// Strips exactly one underscore, including from names that then still start
/// with one: the source name `__mingw_invalidParameterHandler` is decorated to
/// `___mingw_invalidParameterHandler`, and DWARF records the two-underscore
/// form. i386 cdecl decoration is always exactly one `_`, so removing one is
/// right regardless of what follows.
#[must_use]
pub fn undecorate_i386(name: &str) -> String {
    // Fastcall: `@name@N`.
    if let Some(rest) = name.strip_prefix('@')
        && let Some((base, suffix)) = rest.rsplit_once('@')
        && !base.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
        && !suffix.is_empty()
    {
        return base.to_string();
    }
    // Mangled symbols own their leading underscore: `_Z`/`_R` are Itanium and
    // Rust, and stripping it leaves a string no demangler recognizes. Those go
    // to `demangle` intact.
    if name.starts_with("_Z")
        || name.starts_with("_R")
        || name.starts_with("_$s")
        || name.starts_with("_$S")
        || name.starts_with("_T")
        || name.starts_with('?')
    {
        return name.to_string();
    }
    // Cdecl `_name`, stdcall `_name@N`. One underscore only.
    let Some(rest) = name.strip_prefix('_') else {
        return name.to_string();
    };
    if rest.is_empty() {
        return name.to_string();
    }
    match rest.rsplit_once('@') {
        Some((base, suffix))
            if !base.is_empty()
                && !suffix.is_empty()
                && suffix.chars().all(|c| c.is_ascii_digit()) =>
        {
            base.to_string()
        }
        _ => rest.to_string(),
    }
}

#[cfg(test)]
mod undecorate_tests {
    use super::undecorate_i386;

    #[test]
    fn strips_one_underscore_for_cdecl() {
        assert_eq!(undecorate_i386("_mul_ints"), "mul_ints");
        assert_eq!(undecorate_i386("_main"), "main");
    }

    /// `__mingw_invalidParameterHandler` is the source name; the object file
    /// carries `___mingw_...`. Exactly one underscore comes off, which is why
    /// this cannot bail out on a second one.
    #[test]
    fn strips_one_underscore_from_already_underscored_names() {
        assert_eq!(
            undecorate_i386("___mingw_invalidParameterHandler"),
            "__mingw_invalidParameterHandler"
        );
        assert_eq!(undecorate_i386("___dyn_tls_dtor"), "__dyn_tls_dtor");
    }

    #[test]
    fn strips_stdcall_and_fastcall_suffixes() {
        assert_eq!(undecorate_i386("_WinMain@16"), "WinMain");
        assert_eq!(undecorate_i386("@fastcall_fn@8"), "fastcall_fn");
    }

    #[test]
    fn leaves_undecorated_and_malformed_names_alone() {
        assert_eq!(undecorate_i386("main"), "main");
        assert_eq!(undecorate_i386("_"), "_");
        assert_eq!(undecorate_i386(""), "");
        // `@` with a non-numeric tail is not a stdcall suffix.
        assert_eq!(undecorate_i386("_odd@name"), "odd@name");
        // A bare `@` prefix without a numeric suffix is not fastcall.
        assert_eq!(undecorate_i386("@plain"), "@plain");
    }

    /// C++ and Rust mangling is handled by `demangle`, which keys off the
    /// leading `_Z`/`_R`. Undecorating first would leave a string no demangler
    /// recognizes, turning a readable C++ name into `ZN4core3fmtE`.
    #[test]
    fn leaves_mangled_symbols_for_the_demangler() {
        assert_eq!(undecorate_i386("_ZN4core3fmtE"), "_ZN4core3fmtE");
        assert_eq!(undecorate_i386("_RNvC4main3foo"), "_RNvC4main3foo");
        assert_eq!(undecorate_i386("?func@@YAXXZ"), "?func@@YAXXZ");
    }
}
