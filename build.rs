//! Fission Build Script
//!
//! Handles:
//! 1. Linking native Ghidra library (if native_decomp feature enabled)
//! 2. Cross-platform library discovery

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Legacy FFI Linking (optional)
    #[cfg(feature = "native_decomp")]
    {
        println!("cargo:rerun-if-changed=build/Release/ghidra_decompiler.lib");
        println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
        link_ghidra_library();
    }

    Ok(())
}

#[cfg(feature = "native_decomp")]
fn link_ghidra_library() {
    use std::path::PathBuf;
    use std::env;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    
    // Determine library directory based on platform
    #[cfg(target_os = "windows")]
    let lib_dir = manifest_dir.join("build").join("Release");
    
    #[cfg(not(target_os = "windows"))]
    let lib_dir = manifest_dir.join("build");

    // Check if our library exists
    #[cfg(target_os = "windows")]
    let lib_file = "ghidra_decompiler.lib";
    
    #[cfg(not(target_os = "windows"))]
    let lib_file = "libghidra_decompiler.a";

    if !lib_dir.join(lib_file).exists() {
        // Library not built yet, skip linking
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=static=ghidra_decompiler");
        
        // Find vcpkg libraries using VCPKG_ROOT environment variable
        if let Ok(vcpkg_root) = env::var("VCPKG_ROOT") {
            let vcpkg_lib_dir = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows")
                .join("lib");
            
            if vcpkg_lib_dir.exists() {
                println!("cargo:rustc-link-search=native={}", vcpkg_lib_dir.display());
                println!("cargo:rustc-link-lib=static=zlib");
            } else {
                println!("cargo:warning=VCPKG_ROOT set but lib directory not found: {}", vcpkg_lib_dir.display());
            }
        } else {
            // Fallback: try common install locations
            let common_paths = [
                "C:/vcpkg/installed/x64-windows/lib",
                "D:/vcpkg/installed/x64-windows/lib",
            ];
            
            for path in &common_paths {
                let p = PathBuf::from(path);
                if p.exists() {
                    println!("cargo:rustc-link-search=native={}", p.display());
                    println!("cargo:rustc-link-lib=static=zlib");
                    break;
                }
            }
            
            println!("cargo:warning=VCPKG_ROOT not set. Set it to your vcpkg installation directory.");
        }
        
        println!("cargo:rustc-link-lib=dylib=msvcrt");
    }

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=static=ghidra_decompiler");
        
        // Use pkg-config for zlib on Linux
        if let Ok(output) = std::process::Command::new("pkg-config")
            .args(&["--libs", "zlib"])
            .output()
        {
            if output.status.success() {
                println!("cargo:rustc-link-lib=z");
            }
        }
        
        println!("cargo:rustc-link-lib=stdc++");
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=static=ghidra_decompiler");
        
        // macOS typically has zlib in system
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=c++");
        
        // Homebrew location
        if let Ok(brew_prefix) = std::process::Command::new("brew")
            .args(&["--prefix"])
            .output()
        {
            if brew_prefix.status.success() {
                let prefix = String::from_utf8_lossy(&brew_prefix.stdout).trim().to_string();
                let lib_path = PathBuf::from(&prefix).join("lib");
                if lib_path.exists() {
                    println!("cargo:rustc-link-search=native={}", lib_path.display());
                }
            }
        }
    }
}
