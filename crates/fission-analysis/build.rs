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

    fn collect_vcpkg_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();

        for key in ["VCPKG_ROOT", "VCPKG_INSTALLATION_ROOT"] {
            if let Ok(val) = std::env::var(key) {
                let path = PathBuf::from(val);
                if path.exists() {
                    roots.push(path);
                }
            }
        }

        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let candidate = PathBuf::from(user_profile).join("vcpkg");
            if candidate.exists() {
                roots.push(candidate);
            }
        }

        if let Ok(system_drive) = std::env::var("SystemDrive") {
            let candidate = PathBuf::from(format!("{}\\", system_drive)).join("vcpkg");
            if candidate.exists() {
                roots.push(candidate);
            }
        }

        roots
    }

    fn find_vcpkg_toolchain() -> Option<PathBuf> {
        for root in collect_vcpkg_roots() {
            let toolchain = root
                .join("scripts")
                .join("buildsystems")
                .join("vcpkg.cmake");
            if toolchain.exists() {
                return Some(toolchain);
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    fn find_vcpkg_zlib_lib() -> Option<PathBuf> {
        for root in collect_vcpkg_roots() {
            let lib_path = root.join("installed").join("x64-windows").join("lib");
            if lib_path.exists() {
                return Some(lib_path);
            }
        }
        None
    }

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

    // Build CMake configure arguments.
    // Use --fresh to avoid stale cache collisions when source location changes
    // (e.g. vendor path -> root path migration).
    let mut cmake_args: Vec<String> = vec![
        "-S".to_string(),
        decomp_dir.to_string_lossy().into_owned(),
        "-B".to_string(),
        build_dir.to_string_lossy().into_owned(),
        "--fresh".to_string(),
    ];

    if let Some(toolchain) = find_vcpkg_toolchain() {
        cmake_args.push(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()));
        println!(
            "cargo:warning=Using vcpkg toolchain: {}",
            toolchain.display()
        );
    }

    // Run cmake configure
    let cmake_status = Command::new("cmake")
        .args(&cmake_args)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run cmake configure: {}", e));

    if !cmake_status.success() {
        panic!("CMake configure failed");
    }

    // Build the decomp target (cross-platform: cmake --build instead of make)
    let build_status = Command::new("cmake")
        .args([
            "--build",
            &build_dir.to_string_lossy(),
            "--target",
            "decomp",
            "--parallel",
            "4",
        ])
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
        println!(
            "cargo:rustc-link-search=native={}\\Debug",
            build_dir.display()
        );
        println!(
            "cargo:rustc-link-search=native={}\\Release",
            build_dir.display()
        );
        println!("cargo:rustc-link-lib=dylib=decomp");

        // Link against zlib from discovered vcpkg installation
        if let Some(zlib_lib) = find_vcpkg_zlib_lib() {
            println!("cargo:rustc-link-search=native={}", zlib_lib.display());
        } else {
            println!("cargo:warning=No vcpkg zlib path found; relying on system zlib");
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
