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

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|e| panic!("CARGO_MANIFEST_DIR should be set: {}", e));
    let decomp_dir = PathBuf::from(&manifest_dir)
        .join("..")
        .join("..")
        .join("ghidra_decompiler");
    let build_dir = decomp_dir.join("build");

    // Ensure build directory exists
    std::fs::create_dir_all(&build_dir)
        .unwrap_or_else(|e| panic!("Failed to create build directory: {}", e));

    // Build CMake configure arguments
    let mut cmake_args: Vec<String> = vec!["..".to_string()];

    // If VCPKG_ROOT is set, pass the toolchain file to CMake
    if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
        let toolchain = PathBuf::from(&vcpkg_root)
            .join("scripts")
            .join("buildsystems")
            .join("vcpkg.cmake");
        if toolchain.exists() {
            cmake_args.push(format!(
                "-DCMAKE_TOOLCHAIN_FILE={}",
                toolchain.display()
            ));
            println!("cargo:warning=Using vcpkg toolchain: {}", toolchain.display());
        }
    } else {
        // Try well-known vcpkg locations on Windows
        #[cfg(target_os = "windows")]
        {
            let candidates = [
                "C:\\vcpkg\\scripts\\buildsystems\\vcpkg.cmake",
                "C:\\tools\\vcpkg\\scripts\\buildsystems\\vcpkg.cmake",
            ];
            for candidate in &candidates {
                if std::path::Path::new(candidate).exists() {
                    cmake_args.push(format!("-DCMAKE_TOOLCHAIN_FILE={}", candidate));
                    println!("cargo:warning=Using vcpkg toolchain: {}", candidate);
                    break;
                }
            }
        }
    }

    // Run cmake configure
    let cmake_status = Command::new("cmake")
        .args(&cmake_args)
        .current_dir(&build_dir)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run cmake configure: {}", e));

    if !cmake_status.success() {
        panic!("CMake configure failed");
    }

    // Build the decomp target (cross-platform: cmake --build instead of make)
    let build_status = Command::new("cmake")
        .args(["--build", ".", "--target", "decomp", "--parallel", "4"])
        .current_dir(&build_dir)
        .status()
        .unwrap_or_else(|e| panic!("Failed to build decomp target: {}", e));

    if !build_status.success() {
        panic!("Failed to build libdecomp");
    }

    // Tell cargo where to find the library
    println!("cargo:rustc-link-search=native={}", build_dir.display());

    // Platform-specific library linking
    #[cfg(target_os = "windows")]
    {
        // On Windows, MSVC builds produce decomp.lib / decomp.dll
        println!("cargo:rustc-link-search=native={}\\Debug", build_dir.display());
        println!("cargo:rustc-link-search=native={}\\Release", build_dir.display());
        println!("cargo:rustc-link-lib=dylib=decomp");

        // Link against zlib from vcpkg
        if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
            let zlib_lib = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows")
                .join("lib");
            if zlib_lib.exists() {
                println!("cargo:rustc-link-search=native={}", zlib_lib.display());
            }
        } else {
            // Try well-known vcpkg path
            let zlib_lib = "C:\\vcpkg\\installed\\x64-windows\\lib";
            if std::path::Path::new(zlib_lib).exists() {
                println!("cargo:rustc-link-search=native={}", zlib_lib);
            }
        }
        println!("cargo:rustc-link-lib=zlib");
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Link against libdecomp
        println!("cargo:rustc-link-lib=dylib=decomp");

        // Also need to link against zlib (dependency of libdecomp)
        println!("cargo:rustc-link-lib=z");
    }

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
