//! Error types for `amber-core`.

/// Errors returned by the AmberHTML core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The provided URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// No output format was requested (there is no default output).
    #[error("no output format selected; choose at least one")]
    NoOutputSelected,

    /// Filesystem / I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A network/HTTP fetch failed (transport, timeout, etc.).
    #[error("HTTP fetch failed: {0}")]
    Fetch(String),

    /// The server returned a non-success HTTP status for the given URL.
    #[error("HTTP {0} for {1}")]
    HttpStatus(u16, String),

    /// A browser-management or CDP failure (download, spawn, protocol, etc.).
    #[error("browser error: {0}")]
    Browser(String),

    /// A code path that is scaffolded but not yet implemented.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),
}

/// Convenience result alias for the core.
pub type Result<T> = std::result::Result<T, Error>;
