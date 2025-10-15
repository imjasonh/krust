//! Service layer for business logic orchestration
//!
//! This module contains the service layer that orchestrates business logic,
//! separating concerns from the CLI layer in main.rs.

pub mod build;
pub mod platform;

pub use build::{BuildConfig, BuildResult, BuildService};
pub use platform::PlatformDetector;
