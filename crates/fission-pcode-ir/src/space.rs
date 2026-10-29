//! Address-space ids the lifters assign to register and unique (temporary)
//! varnodes. Two numbering schemes are live (the legacy one and the Rust-Sleigh
//! one), so each class has more than one id.

use crate::pcode::Varnode;

pub const UNIQUE_SPACE_ID: u64 = 3;
pub const RUST_SLEIGH_UNIQUE_SPACE_ID: u64 = 2;
pub const REGISTER_SPACE_ID: u64 = 1;
pub const RUST_SLEIGH_REGISTER_SPACE_ID: u64 = 4;
pub const RUST_SLEIGH_ALT_REGISTER_SPACE_ID: u64 = 5;

pub fn is_register_space_id(space_id: u64) -> bool {
    space_id == REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_ALT_REGISTER_SPACE_ID
}

pub fn is_unique_space_id(space_id: u64) -> bool {
    space_id == UNIQUE_SPACE_ID || space_id == RUST_SLEIGH_UNIQUE_SPACE_ID
}

pub fn is_register_varnode(vn: &Varnode) -> bool {
    is_register_space_id(vn.space_id)
}
