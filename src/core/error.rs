use std::io;

/// Error type shared by application code and scanner adapters.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("invalid input: {0}")]
    Input(String),
    #[error("SCSI error: {0}")]
    Scsi(String),
    #[error("no paper in feeder")]
    NoPaper,
    #[error("paper feed error: {0}")]
    Feed(String),
    #[error("registration rejected with status {status}: {reason}")]
    Registration { status: i32, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;
