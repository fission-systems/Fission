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
