//! Build script for Fission
//!
//! When the `native_decomp` feature is enabled, this script:
//! 1. Builds the libdecomp shared library via CMake
//! 2. Sets up linker paths for Rust to find the library

fn main() {
    // Only run cmake build when native_decomp feature is enabled
    #[cfg(feature = "native_decomp")]
    build_libdecomp();

    // For all builds, add the standard library search paths
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "native_decomp")]
fn build_libdecomp() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let decomp_dir = PathBuf::from(&manifest_dir)
        .join("..")
        .join("..")
        .join("ghidra_decompiler");
    let build_dir = decomp_dir.join("build");

    // Ensure build directory exists
    std::fs::create_dir_all(&build_dir).expect("Failed to create build directory");

    // Run cmake configure
    let cmake_status = Command::new("cmake")
        .arg("..")
        .current_dir(&build_dir)
        .status()
        .expect("Failed to run cmake configure");

    if !cmake_status.success() {
        panic!("CMake configure failed");
    }

    // Build the decomp target
    let make_status = Command::new("make")
        .args(["-j4", "decomp"])
        .current_dir(&build_dir)
        .status()
        .expect("Failed to run make");

    if !make_status.success() {
        panic!("Make failed to build libdecomp");
    }

    // Tell cargo where to find the library
    println!("cargo:rustc-link-search=native={}", build_dir.display());

    // Link against libdecomp
    println!("cargo:rustc-link-lib=dylib=decomp");

    // Also need to link against zlib (dependency of libdecomp)
    println!("cargo:rustc-link-lib=z");

    // Set rpath for runtime library discovery
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", build_dir.display());

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", build_dir.display());

    // Rerun if any C++ files change
    println!(
        "cargo:rerun-if-changed={}",
        decomp_dir.join("CMakeLists.txt").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        decomp_dir.join("src/ffi/libdecomp_ffi.cpp").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        decomp_dir
            .join("include/fission/ffi/libdecomp_ffi.h")
            .display()
    );
}
