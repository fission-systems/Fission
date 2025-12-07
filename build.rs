//! Fission Build Script
//!
//! Compiles the Ghidra C++ decompiler and links it as a static library.
//! Currently disabled - will be enabled when build environment is set up.

fn main() {
    println!("cargo:rerun-if-changed=ghidra_decompiler/wrapper.cpp");
    println!("cargo:rerun-if-changed=ghidra_decompiler/wrapper.h");

    // C++ compilation is currently disabled
    // To enable:
    // 1. Ensure MSVC or MinGW C++ toolchain is installed
    // 2. Set the native_decomp feature flag
    // 3. Uncomment the build code below
    
    #[cfg(feature = "native_decomp")]
    {
        build_native_library();
    }

    #[cfg(not(feature = "native_decomp"))]
    {
        println!("cargo:warning=Native Ghidra decompiler disabled. Using stub mode.");
        println!("cargo:warning=To enable: cargo build --features native_decomp");
    }
}

#[cfg(feature = "native_decomp")]
fn build_native_library() {
    use std::path::PathBuf;

    let ghidra_dir = PathBuf::from("ghidra_decompiler");
    let wrapper_src = ghidra_dir.join("wrapper.cpp");

    if !wrapper_src.exists() {
        println!("cargo:warning=C++ wrapper not found - native decompiler disabled");
        return;
    }

    // Collect Ghidra C++ source files
    let ghidra_sources: Vec<PathBuf> = std::fs::read_dir(&ghidra_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|ext| ext == "cc").unwrap_or(false))
        .filter(|p| {
            let name = p.file_stem().unwrap().to_str().unwrap();
            // Exclude problematic files
            !name.contains("bfd") &&
            !name.starts_with("test") &&
            name != "consolemain" &&
            !name.starts_with("slgh_compile") &&
            name != "slghscan" &&
            !name.contains("ghidra_process") &&
            !name.contains("ghidra_arch") &&
            name != "sleighexample"
        })
        .collect();

    if ghidra_sources.is_empty() {
        println!("cargo:warning=No Ghidra sources found - native decompiler disabled");
        return;
    }

    println!("cargo:warning=Building Ghidra with {} source files", ghidra_sources.len());

    let mut build = cc::Build::new();
    
    build
        .cpp(true)
        .include(&ghidra_dir)
        .define("__TERMINAL__", None)
        .warnings(false)
        .extra_warnings(false);

    #[cfg(target_os = "windows")]
    {
        build.std("c++17");
        build.flag_if_supported("/EHsc");
        build.flag_if_supported("/bigobj");
    }

    #[cfg(not(target_os = "windows"))]
    {
        build.std("c++11");
        build.flag("-Wno-sign-compare");
    }

    build.file(&wrapper_src);
    for src in &ghidra_sources {
        build.file(src);
    }

    build.compile("ghidra_decomp");
    
    println!("cargo:rustc-link-lib=static=ghidra_decomp");
    
    #[cfg(not(target_os = "windows"))]
    println!("cargo:rustc-link-lib=z");

    println!("cargo:warning=Ghidra decompiler built successfully!");
}
