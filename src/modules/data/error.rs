use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestionError {
    #[error("failed to open or map graph file: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not detect a supported graph format for path: {0}")]
    UnknownFormat(String),

    #[error("malformed {format} at line {line}: {reason}")]
    Malformed {
        format: &'static str,
        line: usize,
        reason: String,
    },
}
