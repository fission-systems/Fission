//! Manually compare two loader timings for a local binary.
//!
//! Run with:
//! `cargo run -p fission-loader --example profile_loader_fixture -- <path-to-binary>`
//! This tool is intentionally outside crate unit-test sources: it accepts an
//! explicit local input and is not part of normal tests or CI.

use std::error::Error;
use std::time::Instant;

use fission_loader::LoadedBinary;

fn main() -> Result<(), Box<dyn Error>> {
    let Some(path) = std::env::args_os().nth(1) else {
        return Err(
            "usage: cargo run -p fission-loader --example profile_loader_fixture -- <path-to-binary>"
                .into(),
        );
    };

    println!("=== Loader profiling (RUN 1) ===");
    let start1 = Instant::now();
    let _binary1 = LoadedBinary::from_file(&path)?;
    println!("LoadedBinary::from_file RUN 1 took: {:?}", start1.elapsed());

    println!("=== Loader profiling (RUN 2) ===");
    let start2 = Instant::now();
    let _binary2 = LoadedBinary::from_file(&path)?;
    println!("LoadedBinary::from_file RUN 2 took: {:?}", start2.elapsed());

    Ok(())
}
