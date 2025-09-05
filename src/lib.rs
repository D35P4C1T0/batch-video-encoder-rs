pub mod cli;
pub mod scanner;
pub mod tui;
pub mod encoder;
pub mod ffmpeg;
pub mod error_recovery;
pub mod performance;
pub mod benchmarks;

#[cfg(test)]
mod error_handling_tests;

use thiserror::Error;
use std::path::PathBuf;

#[derive(Debug, Clone, Error)]
pub enum AppError {
    #[error("File scanning error: {0}")]
    ScanError(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("TUI error: {0}")]
    TuiError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Hardware acceleration error: {0}")]
    HardwareAccelerationError(String),
    
    #[error("FFmpeg error: {0}")]
    FFmpegError(String),
    
    #[error("File processing error for {file}: {error}")]
    FileProcessingError { file: PathBuf, error: String },
    
    #[error("Batch processing error: {completed} completed, {failed} failed")]
    BatchProcessingError { completed: usize, failed: usize },
    
    #[error("Recovery error: {0}")]
    RecoveryError(String),
    
    #[error("User cancellation")]
    UserCancellation,
    
    #[error("System resource error: {0}")]
    SystemResourceError(String),
}

/// Categorizes errors by their severity and recovery potential
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    /// Critical errors that should stop all processing
    Critical,
    /// Errors that affect a single file but allow batch processing to continue
    FileLevel,
    /// Warnings that don't prevent operation but should be reported
    Warning,
    /// Recoverable errors that can be automatically handled
    Recoverable,
}



impl AppError {
    /// Determine the severity of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            AppError::ConfigError(_) => ErrorSeverity::Critical,
            AppError::SystemResourceError(_) => ErrorSeverity::Critical,
            AppError::UserCancellation => ErrorSeverity::Critical,
            AppError::TuiError(_) => ErrorSeverity::Critical,
            AppError::IoError(_) => ErrorSeverity::FileLevel,
            AppError::ScanError(_) => ErrorSeverity::FileLevel,
            AppError::EncodingError(_) => ErrorSeverity::FileLevel,
            AppError::FFmpegError(_) => ErrorSeverity::FileLevel,
            AppError::FileProcessingError { .. } => ErrorSeverity::FileLevel,
            AppError::HardwareAccelerationError(_) => ErrorSeverity::Recoverable,
            AppError::BatchProcessingError { .. } => ErrorSeverity::Warning,
            AppError::RecoveryError(_) => ErrorSeverity::Warning,
        }
    }
    
    /// Determine the recommended recovery strategy
    pub fn recovery_strategy(&self) -> crate::error_recovery::RecoveryStrategy {
        match self {
            AppError::ConfigError(_) => crate::error_recovery::RecoveryStrategy::Abort,
            AppError::SystemResourceError(_) => crate::error_recovery::RecoveryStrategy::Abort,
            AppError::UserCancellation => crate::error_recovery::RecoveryStrategy::Abort,
            AppError::TuiError(_) => crate::error_recovery::RecoveryStrategy::Abort,
            AppError::IoError(_) => crate::error_recovery::RecoveryStrategy::SkipFile,
            AppError::ScanError(_) => crate::error_recovery::RecoveryStrategy::SkipFile,
            AppError::EncodingError(_) => crate::error_recovery::RecoveryStrategy::SkipFile,
            AppError::FFmpegError(_) => crate::error_recovery::RecoveryStrategy::SkipFile,
            AppError::FileProcessingError { .. } => crate::error_recovery::RecoveryStrategy::SkipFile,
            AppError::HardwareAccelerationError(_) => crate::error_recovery::RecoveryStrategy::Fallback,
            AppError::BatchProcessingError { .. } => crate::error_recovery::RecoveryStrategy::UserConfirmation,
            AppError::RecoveryError(_) => crate::error_recovery::RecoveryStrategy::UserConfirmation,
        }
    }
    
    /// Check if this error allows continuing with batch processing
    pub fn allows_batch_continuation(&self) -> bool {
        matches!(self.severity(), ErrorSeverity::FileLevel | ErrorSeverity::Warning | ErrorSeverity::Recoverable)
    }
    
    /// Create a file-specific error
    pub fn file_error(file: PathBuf, error: impl Into<String>) -> Self {
        AppError::FileProcessingError {
            file,
            error: error.into(),
        }
    }
    
    /// Create a hardware acceleration error
    pub fn hardware_error(message: impl Into<String>) -> Self {
        AppError::HardwareAccelerationError(message.into())
    }
    
    /// Create an FFmpeg-specific error
    pub fn ffmpeg_error(message: impl Into<String>) -> Self {
        AppError::FFmpegError(message.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::IoError(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

// Re-export error types for external use
pub use AppError::*;

/// Generate output filename with H.265 suffix
pub fn generate_output_filename(input_filename: &str) -> String {
    if let Some(dot_pos) = input_filename.rfind('.') {
        let (name, ext) = input_filename.split_at(dot_pos);
        format!("{}-h265{}", name, ext)
    } else {
        format!("{}-h265", input_filename)
    }
}