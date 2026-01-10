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
            // println!("cargo:rustc-link-lib=dylib=decomp"); // The #[link] attribute handles this
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
