pub mod cli;
pub mod scanner;
pub mod tui;
pub mod encoder;
pub mod ffmpeg;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("File scanning error: {0}")]
    ScanError(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("TUI error: {0}")]
    TuiError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, AppError>;