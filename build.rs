//! Fission Build Script
//!
//! Links the pre-built Ghidra C++ decompiler library.

fn main() {
    println!("cargo:rerun-if-changed=build/Release/ghidra_decompiler.lib");
    println!("cargo:rerun-if-changed=ghidra_decompiler/wrapper.cpp");
    println!("cargo:rerun-if-changed=ghidra_decompiler/wrapper.h");

    #[cfg(feature = "native_decomp")]
    {
        link_ghidra_library();
    }

    #[cfg(not(feature = "native_decomp"))]
    {
        println!("cargo:warning=Native Ghidra decompiler disabled. Using stub mode.");
        println!("cargo:warning=To enable: cargo build --features native_decomp");
    }
}

#[cfg(feature = "native_decomp")]
fn link_ghidra_library() {
    use std::path::PathBuf;
    use std::env;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    
    // Link paths
    let lib_dir = manifest_dir.join("build").join("Release");
    let vcpkg_lib_dir = PathBuf::from("C:/vcpkg/installed/x64-windows/lib");

    if !lib_dir.join("ghidra_decompiler.lib").exists() {
        println!("cargo:warning=ghidra_decompiler.lib not found!");
        println!("cargo:warning=Run: cmake -B build -S ghidra_decompiler -DCMAKE_TOOLCHAIN_FILE=C:/vcpkg/scripts/buildsystems/vcpkg.cmake");
        println!("cargo:warning=     cmake --build build --config Release");
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ghidra_decompiler");

    // Link vcpkg zlib
    if vcpkg_lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", vcpkg_lib_dir.display());
        println!("cargo:rustc-link-lib=static=zlib");
    }

    // Link Windows system libraries (required by C++ runtime)
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=dylib=msvcrt");
    }

    println!("cargo:warning=Ghidra decompiler library linked successfully!");
}
