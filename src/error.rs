use std::process::ExitStatus;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("required backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("failed to run {program}: {source}")]
    Io {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} failed ({status}): {stderr}")]
    Command {
        program: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error("file operation failed: {0}")]
    FileSystem(#[from] std::io::Error),
    #[error("invalid configuration: {0}")]
    Json(#[from] serde_json::Error),
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("output pipe closed")]
    BrokenPipe,
}
