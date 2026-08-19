use cpp_demangle::DemangleOptions;
use cpp_demangle::Symbol as CppSymbol;
use msvc_demangler::demangle as msvc_demangle;
use rustc_demangle::demangle as rust_demangle;
use std::process::Command;

/// Demangles a symbol name if possible.
/// Supports Rust, C++ (Itanium/GNU), MSVC, and Swift.
pub fn demangle(name: &str) -> String {
    // 0. Swift demangling (Starts with _$s, _$S, _T, __T)
    if name.starts_with("_$s")
        || name.starts_with("_$S")
        || name.starts_with("_T")
        || name.starts_with("__T")
    {
        if let Some(demangled) = swift_demangle(name) {
            return demangled;
        }
    }

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

/// Helper to demangle Swift symbols using system 'swift' tool
fn swift_demangle(name: &str) -> Option<String> {
    // Avoid launching process for short strings or obviously non-mangled names
    if name.len() < 4 {
        return None;
    }

    // Use 'swift demangle -compact -simplified <name>'
    match Command::new("swift")
        .args(&["demangle", "--compact", "--simplified", name])
        .output()
    {
        Ok(output) if output.status.success() => {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() && s != name {
                return Some(s);
            }
        }
        _ => {}
    }
    None
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
