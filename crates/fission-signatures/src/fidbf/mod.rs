pub mod loader;
pub mod parser;
mod raw_db;
mod raw_fields;
mod tables;
pub mod types;

pub use loader::{discover_fidbf_paths, parse_all_fidbf_for_arch};
pub use parser::{FidbfParseError, parse_fidbf};
pub use types::{
    FID_ACCEPT_THRESHOLD, FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfMatch, FidbfRelation,
    FidbfRelationType,
};
