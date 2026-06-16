#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encode error: {0}")]
    Encode(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("audio error: {0}")]
    Audio(String),
}

pub type Result<T> = std::result::Result<T, Error>;
