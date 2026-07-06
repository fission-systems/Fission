pub mod linux;
pub mod windows;
pub mod bare_metal;
pub mod env;

pub use env::{OsEnvironment, HleResult};
pub use windows::WindowsEnv;
pub use linux::LinuxEnv;
pub use bare_metal::BareMetalEnv;
