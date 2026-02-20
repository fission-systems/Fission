fn main() {
    // Only modify search path if the native_decomp feature is enabled
    if std::env::var("CARGO_FEATURE_NATIVE_DECOMP").is_ok() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest_path = std::path::Path::new(&manifest_dir);

        // Assuming directory structure:
        // crates/fission-ffi/
        // ghidra_decompiler/build/libdecomp.dylib

        let root_dir = manifest_path
            .parent() // crates
            .and_then(|p| p.parent()) // root
            .expect("Failed to find project root directory");

        let lib_path = root_dir.join("ghidra_decompiler").join("build");

        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", lib_path.display());

            let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

            if target_os == "windows" {
                // MSVC puts outputs under Debug/ or Release/ sub-directories
                let debug_path = lib_path.join("Debug");
                let release_path = lib_path.join("Release");
                if debug_path.exists() {
                    println!("cargo:rustc-link-search=native={}", debug_path.display());
                }
                if release_path.exists() {
                    println!("cargo:rustc-link-search=native={}", release_path.display());
                }

                // Add vcpkg zlib search path
                if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
                    let zlib_lib = std::path::Path::new(&vcpkg_root)
                        .join("installed")
                        .join("x64-windows")
                        .join("lib");
                    if zlib_lib.exists() {
                        println!("cargo:rustc-link-search=native={}", zlib_lib.display());
                    }
                } else {
                    let default_vcpkg_zlib = "C:\\vcpkg\\installed\\x64-windows\\lib";
                    if std::path::Path::new(default_vcpkg_zlib).exists() {
                        println!("cargo:rustc-link-search=native={}", default_vcpkg_zlib);
                    }
                }

                // Auto-copy DLLs to cargo output directory for runtime discovery
                if let Ok(out_dir) = std::env::var("OUT_DIR") {
                    // OUT_DIR is like target/debug/build/fission-ffi-xxx/out
                    // We need target/debug/
                    let target_dir = std::path::Path::new(&out_dir)
                        .ancestors()
                        .find(|p| p.ends_with("debug") || p.ends_with("release"))
                        .map(|p| p.to_path_buf());

                    if let Some(target_dir) = target_dir {
                        // Copy decomp.dll from Debug/ or Release/
                        for sub in &["Debug", "Release"] {
                            let dll_src = lib_path.join(sub).join("decomp.dll");
                            if dll_src.exists() {
                                let dst = target_dir.join("decomp.dll");
                                if std::fs::copy(&dll_src, &dst).is_ok() {
                                    println!("cargo:warning=Copied decomp.dll to {}", dst.display());
                                }
                                break;
                            }
                        }
                        // Copy zlib DLLs from vcpkg
                        let vcpkg_bin = if let Ok(vr) = std::env::var("VCPKG_ROOT") {
                            std::path::PathBuf::from(vr).join("installed").join("x64-windows").join("bin")
                        } else {
                            std::path::PathBuf::from("C:\\vcpkg\\installed\\x64-windows\\bin")
                        };
                        if vcpkg_bin.exists() {
                            for dll_name in &["zlib1.dll", "zlibd1.dll"] {
                                let src = vcpkg_bin.join(dll_name);
                                if src.exists() {
                                    let dst = target_dir.join(dll_name);
                                    let _ = std::fs::copy(&src, &dst);
                                }
                            }
                        }
                        // Also copy from build/Debug/ any other DLLs
                        for sub in &["Debug", "Release"] {
                            let sub_dir = lib_path.join(sub);
                            if sub_dir.exists() {
                                if let Ok(entries) = std::fs::read_dir(&sub_dir) {
                                    for entry in entries.flatten() {
                                        let path = entry.path();
                                        if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                                            let fname = path.file_name().unwrap();
                                            let dst = target_dir.join(fname);
                                            let _ = std::fs::copy(&path, &dst);
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            } else {
                // On macOS and Linux, add rpath for runtime discovery
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_path.display());
            }

            println!("cargo:rerun-if-changed={}", lib_path.display());
        } else {
            println!(
                "cargo:warning=Native library path not found: {}",
                lib_path.display()
            );
            println!(
                "cargo:warning=Make sure you have built ghidra_decompiler/build/libdecomp.dylib"
            );
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
