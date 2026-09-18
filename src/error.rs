//! Crate-wide error type.

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors `vdb` can return.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The underlying LMDB engine returned an error.
    #[cfg(feature = "storage")]
    #[error("LMDB error: {0}")]
    Kv(#[from] heed::Error),

    /// A collection with the given name does not exist.
    #[error("collection not found: {0}")]
    CollectionNotFound(String),

    /// A vector's dimensionality did not match the collection's configured dimension.
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch {
        /// Dimension configured for the collection.
        expected: u32,
        /// Dimension of the vector actually supplied.
        got: u32,
    },

    /// An on-disk file's magic bytes did not match what was expected.
    #[error("bad file magic: expected {expected:?}, got {got:?}")]
    BadMagic {
        /// Expected magic bytes.
        expected: &'static [u8],
        /// Magic bytes actually read.
        got: Vec<u8>,
    },
}
