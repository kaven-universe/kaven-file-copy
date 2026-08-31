use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum KfcError {
    #[error("invalid rule: {0}")]
    InvalidRule(String),
    #[error("unsupported rule schema version {actual}; maximum supported version is {supported}")]
    UnsupportedSchema { actual: u32, supported: u32 },
    #[error("path escapes the configured endpoint root: {0}")]
    PathEscape(String),
    #[error("source and destination form an unsafe relationship: {0}")]
    UnsafeRelationship(String),
    #[error("I/O operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("job was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, KfcError>;

pub trait IoResultExt<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        let path = path.into();
        self.map_err(|source| KfcError::Io { path, source })
    }
}
