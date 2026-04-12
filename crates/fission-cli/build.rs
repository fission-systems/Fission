fn main() {
    if std::env::var("CARGO_FEATURE_NATIVE_DECOMP").is_ok() {
        panic!("feature 'native_decomp' is deprecated and blocked; use default Rust-only CLI mode");
    }

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NATIVE_DECOMP");
    println!("cargo:rerun-if-changed=build.rs");
}
