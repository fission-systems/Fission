fn main() {
    fn collect_vcpkg_roots() -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();

        for key in ["VCPKG_ROOT", "VCPKG_INSTALLATION_ROOT"] {
            if let Ok(val) = std::env::var(key) {
                let path = std::path::PathBuf::from(val);
                if path.exists() {
                    roots.push(path);
                }
            }
        }

        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let candidate = std::path::PathBuf::from(user_profile).join("vcpkg");
            if candidate.exists() {
                roots.push(candidate);
            }
        }

        if let Ok(system_drive) = std::env::var("SystemDrive") {
            let candidate = std::path::PathBuf::from(format!("{}\\", system_drive)).join("vcpkg");
            if candidate.exists() {
                roots.push(candidate);
            }
        }

        roots
    }

    fn find_vcpkg_zlib_lib() -> Option<std::path::PathBuf> {
        for root in collect_vcpkg_roots() {
            let lib_path = root.join("installed").join("x64-windows").join("lib");
            if lib_path.exists() {
                return Some(lib_path);
            }
        }
        None
    }

    fn find_vcpkg_bin() -> Option<std::path::PathBuf> {
        for root in collect_vcpkg_roots() {
            let bin_path = root.join("installed").join("x64-windows").join("bin");
            if bin_path.exists() {
                return Some(bin_path);
            }
        }
        None
    }

    // Only modify search path if the native_decomp feature is enabled
    if std::env::var("CARGO_FEATURE_NATIVE_DECOMP").is_ok() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|e| panic!("CARGO_MANIFEST_DIR should be set: {}", e));
        let manifest_path = std::path::Path::new(&manifest_dir);

        // Assuming directory structure:
        // crates/fission-ffi/
        // ghidra_decompiler/build/libdecomp.dylib

        let root_dir = manifest_path
            .parent() // crates
            .and_then(|p| p.parent()) // root
            .unwrap_or_else(|| panic!("Failed to find project root directory"));

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
                if let Some(zlib_lib) = find_vcpkg_zlib_lib() {
                    println!("cargo:rustc-link-search=native={}", zlib_lib.display());
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
                                    println!(
                                        "cargo:warning=Copied decomp.dll to {}",
                                        dst.display()
                                    );
                                }
                                break;
                            }
                        }
                        // Copy zlib DLLs from vcpkg
                        if let Some(vcpkg_bin) = find_vcpkg_bin() {
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
                                        if path.extension().and_then(|e| e.to_str()) == Some("dll")
                                        {
                                            let Some(fname) = path.file_name() else {
                                                continue;
                                            };
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
                // macOS / Linux: add rpath so the linker embeds the search path.
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_path.display());

                // Additionally copy the dylib into target/{profile}/ so that
                // `dyld` can find it even when rpaths from OUT_DIR entries are
                // used (this affects `cargo run` / `tauri dev` on macOS).
                if let Ok(out_dir) = std::env::var("OUT_DIR") {
                    let target_dir = std::path::Path::new(&out_dir)
                        .ancestors()
                        .find(|p| {
                            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            name == "debug" || name == "release"
                        })
                        .map(|p| p.to_path_buf());

                    if let Some(target_dir) = target_dir {
                        let dylib_src = lib_path.join("libdecomp.dylib");
                        if dylib_src.exists() {
                            let dst = target_dir.join("libdecomp.dylib");
                            // If dst is a symlink (or regular file), check that
                            // it isn't the same inode as the source before copying,
                            // otherwise std::fs::copy would truncate the source.
                            let same_file = dst.exists() && {
                                let src_canon = dylib_src.canonicalize().ok();
                                let dst_canon = dst.canonicalize().ok();
                                src_canon.is_some() && src_canon == dst_canon
                            };
                            if same_file {
                                // Replace the circular symlink with a real copy:
                                // remove the symlink first so that copy targets
                                // a fresh path.
                                let _ = std::fs::remove_file(&dst);
                            }
                            match std::fs::copy(&dylib_src, &dst) {
                                Ok(_) => println!(
                                    "cargo:warning=Copied libdecomp.dylib to {}",
                                    dst.display()
                                ),
                                Err(e) => {
                                    println!("cargo:warning=Failed to copy libdecomp.dylib: {}", e)
                                }
                            }
                        }
                        // Also handle libdecomp.so for Linux
                        let so_src = lib_path.join("libdecomp.so");
                        if so_src.exists() {
                            let dst = target_dir.join("libdecomp.so");
                            let _ = std::fs::copy(&so_src, &dst);
                        }
                    }
                }
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
