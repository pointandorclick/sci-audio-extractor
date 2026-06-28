use thiserror::Error;

#[derive(Error, Debug)]
pub enum SciError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid resource map: {0}")]
    InvalidMap(String),

    #[error("Invalid resource: {0}")]
    InvalidResource(String),

    #[error("Unsupported SCI version: {0}")]
    UnsupportedVersion(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("No MT-32 track found in sound resource {0}")]
    NoMt32Track(u16),

    #[error("ROM error: {0}")]
    RomError(String),

    #[error("Encoding error: {0}")]
    EncodingError(String),
}
