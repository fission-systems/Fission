//! Fission Build Script
//!
//! Handles:
//! 1. Generating gRPC client code from .proto files
//! 2. Linking native Ghidra library (if native_decomp feature enabled)

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=protos/ghidra_service.proto");

    // Set protoc path from vcpkg if not already set
    if std::env::var("PROTOC").is_err() {
        let vcpkg_protoc = "C:/vcpkg/installed/x64-windows/tools/protobuf/protoc.exe";
        if std::path::Path::new(vcpkg_protoc).exists() {
            std::env::set_var("PROTOC", vcpkg_protoc);
        }
    }

    // 1. Generate gRPC code
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile(
            &["protos/ghidra_service.proto"],
            &["protos"]
        )?;

    // 2. Legacy FFI Linking (optional)
    #[cfg(feature = "native_decomp")]
    {
        println!("cargo:rerun-if-changed=build/Release/ghidra_decompiler.lib");
        link_ghidra_library();
    }

    Ok(())
}

#[cfg(feature = "native_decomp")]
fn link_ghidra_library() {
    use std::path::PathBuf;
    use std::env;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lib_dir = manifest_dir.join("build").join("Release");
    let vcpkg_lib_dir = PathBuf::from("C:/vcpkg/installed/x64-windows/lib");

    if lib_dir.join("ghidra_decompiler.lib").exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=ghidra_decompiler");
        println!("cargo:rustc-link-search=native={}", vcpkg_lib_dir.display());
        println!("cargo:rustc-link-lib=static=zlib");
        
        #[cfg(target_os = "windows")]
        println!("cargo:rustc-link-lib=dylib=msvcrt");
    }
}
