//! Ghidra Sleigh compiler and runtime glue for Fission.

// `cargo clippy -- -D warnings` is CI policy; keep this experimental surface from blocking merges.
#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(hidden_glob_reexports)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod compiler;
pub mod runtime;
