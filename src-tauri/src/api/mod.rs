//! The Photos.sh API: the client, its errors and what it returns.

pub mod client;
pub mod error;
pub mod types;

#[cfg(test)]
mod client_tests;

pub use client::{ApiClient, DEFAULT_BASE_URL, DEFAULT_TIMEOUT, ExtraPart, RetryDelays};
pub use error::ApiError;
pub use types::{Account, ImageResult, Marketplace, Preset};
