pub mod loader;
pub mod parser;
pub mod types;

pub use loader::{discover_fidbf_paths, parse_all_fidbf_for_arch};
pub use parser::{FidbfParseError, parse_fidbf};
pub use types::{FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfRelation, FidbfRelationType};
