//! Centralized constants for the Fission codebase
//!
//! This module provides well-named constants to replace magic numbers
//! and hardcoded values throughout the codebase, improving maintainability
//! and documentation.

pub mod binary_format;
pub mod memory;

#[cfg(windows)]
pub mod windows_api;
