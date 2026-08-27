//! Unified error type for murl-core.

use thiserror::Error;

use crate::murl::MurlParseError;

/// All fallible operations in murl-core return this error.
///
/// Variants are grouped by pipeline stage so an embedder (CLI, daemon, GUI)
/// can map them onto exit codes or user-facing messages without string
/// matching.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    /// The mURL string itself is malformed.
    #[error("parse error: {0}")]
    Parse(#[from] MurlParseError),

    /// The manifest is not well-formed JSON or not a JSON object.
    #[error("manifest error: {0}")]
    Manifest(String),

    /// The manifest is well-formed but violates the specification.
    #[error("validation failed: {0}")]
    Validation(String),

    /// A name could not be resolved to a manifest.
    #[error("resolution error: {0}")]
    Resolution(String),

    /// A configured limit (size, depth, count, time) was exceeded.
    /// Limits exist to keep hostile inputs cheap; exceeding one is always a
    /// hard stop, never a warning.
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),

    /// Recursive resolution encountered a cycle.
    #[error("cycle detected: {0}")]
    Cycle(String),

    /// Signature verification or trust evaluation failed.
    #[error("trust error: {0}")]
    Trust(String),

    /// The policy engine refused the operation.
    #[error("denied by policy: {0}")]
    Denied(String),

    /// A name, file, or resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Network fetch failure (reported by the embedder's fetcher).
    #[error("fetch error: {0}")]
    Fetch(String),

    /// Launching a resource handler failed.
    #[error("dispatch error: {0}")]
    Dispatch(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
