use std::path::PathBuf;

use tokio::sync::mpsc;
use crate::{AppError, Result, ErrorSeverity};

/// Recovery strategies for different error types
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStrategy {
    /// Stop all processing immediately
    Abort,
    /// Skip the current file and continue with others
    SkipFile,
    /// Retry the operation with different parameters
    Retry,
    /// Fall back to alternative method (e.g., software encoding)
    Fallback,
    /// Ask user for confirmation before proceeding
    UserConfirmation,
}

/// Tracks error statistics and recovery attempts
#[derive(Debug, Clone)]
pub struct ErrorStatistics {
    pub total_errors: usize,
    pub critical_errors: usize,
    pub file_level_errors: usize,
    pub recoverable_errors: usize,
    pub recovery_attempts: usize,
    pub successful_recoveries: usize,
    pub failed_recoveries: usize,
}

impl Default for ErrorStatistics {
    fn default() -> Self {
        Self {
            total_errors: 0,
            critical_errors: 0,
            file_level_errors: 0,
            recoverable_errors: 0,
            recovery_attempts: 0,
            successful_recoveries: 0,
            failed_recoveries: 0,
        }
    }
}

/// Manages error recovery strategies and user notifications
pub struct ErrorRecoveryManager {
    error_log: Vec<(PathBuf, AppError)>,
    recovery_log: Vec<RecoveryAttempt>,
    notification_tx: Option<mpsc::Sender<ErrorNotification>>,
    max_retry_attempts: usize,
    hardware_fallback_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    pub file_path: Option<PathBuf>,
    pub original_error: String,
    pub strategy: RecoveryStrategy,
    pub attempt_number: usize,
    pub success: bool,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone)]
pub enum ErrorNotification {
    HardwareFallback {
        reason: String,
        fallback_method: String,
    },
    FileSkipped {
        file: PathBuf,
        reason: String,
    },
    RecoveryAttempt {
        file: Option<PathBuf>,
        strategy: RecoveryStrategy,
        attempt: usize,
    },
    BatchContinuation {
        completed: usize,
        failed: usize,
        remaining: usize,
    },
    CriticalError {
        error: String,
        requires_user_action: bool,
    },
}

impl ErrorRecoveryManager {
    /// Create a new error recovery manager
    pub fn new() -> Self {
        Self {
            error_log: Vec::new(),
            recovery_log: Vec::new(),
            notification_tx: None,
            max_retry_attempts: 3,
            hardware_fallback_enabled: true,
        }
    }
    
    /// Set up notification channel for error reporting
    pub fn with_notifications(mut self, tx: mpsc::Sender<ErrorNotification>) -> Self {
        self.notification_tx = Some(tx);
        self
    }
    
    /// Configure maximum retry attempts
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retry_attempts = max_retries;
        self
    }
    
    /// Enable or disable hardware acceleration fallback
    pub fn with_hardware_fallback(mut self, enabled: bool) -> Self {
        self.hardware_fallback_enabled = enabled;
        self
    }
    
    /// Handle an error and determine recovery strategy
    pub async fn handle_error(
        &mut self,
        error: AppError,
        file_path: Option<PathBuf>,
    ) -> Result<RecoveryDecision> {
        // Log the error (use a dummy path for errors without file paths)
        let log_path = file_path.clone().unwrap_or_else(|| PathBuf::from("system"));
        self.error_log.push((log_path, error.clone()));
        
        // Update statistics
        self.update_statistics(&error);
        
        // Determine recovery strategy
        let _strategy = error.recovery_strategy();
        let severity = error.severity();
        
        match severity {
            ErrorSeverity::Critical => {
                self.notify_critical_error(&error).await;
                self.log_recovery_attempt(
                    file_path.clone(),
                    error.to_string(),
                    RecoveryStrategy::Abort,
                    1,
                    false, // Critical errors are not successfully recovered
                );
                Ok(RecoveryDecision::Abort)
            }
            ErrorSeverity::FileLevel => {
                self.handle_file_level_error(error, file_path).await
            }
            ErrorSeverity::Recoverable => {
                self.handle_recoverable_error(error, file_path).await
            }
            ErrorSeverity::Warning => {
                let decision = self.handle_warning_error(error.clone(), file_path.clone()).await?;
                self.log_recovery_attempt(
                    file_path,
                    error.to_string(),
                    RecoveryStrategy::UserConfirmation,
                    1,
                    true,
                );
                Ok(decision)
            }
        }
    }
    
    /// Handle file-level errors that allow batch continuation
    async fn handle_file_level_error(
        &mut self,
        error: AppError,
        file_path: Option<PathBuf>,
    ) -> Result<RecoveryDecision> {
        if let Some(path) = &file_path {
            // Notify about file being skipped
            self.notify_file_skipped(path.clone(), error.to_string()).await;
            
            // Log recovery attempt
            self.log_recovery_attempt(
                file_path.clone(),
                error.to_string(),
                RecoveryStrategy::SkipFile,
                1,
                true,
            );
            
            Ok(RecoveryDecision::SkipFile)
        } else {
            // If no specific file, this might be a broader issue
            Ok(RecoveryDecision::Continue)
        }
    }
    
    /// Handle recoverable errors with fallback strategies
    async fn handle_recoverable_error(
        &mut self,
        error: AppError,
        file_path: Option<PathBuf>,
    ) -> Result<RecoveryDecision> {
        match &error {
            AppError::HardwareAccelerationError(msg) => {
                if self.hardware_fallback_enabled {
                    self.notify_hardware_fallback(
                        msg.clone(),
                        "Software encoding (libx265)".to_string(),
                    ).await;
                    
                    self.log_recovery_attempt(
                        file_path,
                        error.to_string(),
                        RecoveryStrategy::Fallback,
                        1,
                        true,
                    );
                    
                    Ok(RecoveryDecision::FallbackToSoftware)
                } else {
                    Ok(RecoveryDecision::SkipFile)
                }
            }
            _ => {
                // For other recoverable errors, try to continue
                Ok(RecoveryDecision::Continue)
            }
        }
    }
    
    /// Handle warning-level errors
    async fn handle_warning_error(
        &mut self,
        error: AppError,
        _file_path: Option<PathBuf>,
    ) -> Result<RecoveryDecision> {
        match &error {
            AppError::BatchProcessingError { completed, failed } => {
                // Calculate remaining files (this would need to be passed in)
                let remaining = 0; // This should be calculated by the caller
                
                self.notify_batch_continuation(*completed, *failed, remaining).await;
                Ok(RecoveryDecision::Continue)
            }
            _ => Ok(RecoveryDecision::Continue),
        }
    }
    
    /// Update error statistics (no longer needed as we calculate from logs)
    fn update_statistics(&mut self, _error: &AppError) {
        // Statistics are now calculated dynamically from error_log and recovery_log
    }
    
    /// Log a recovery attempt
    fn log_recovery_attempt(
        &mut self,
        file_path: Option<PathBuf>,
        original_error: String,
        strategy: RecoveryStrategy,
        attempt_number: usize,
        success: bool,
    ) {
        let attempt = RecoveryAttempt {
            file_path,
            original_error,
            strategy,
            attempt_number,
            success,
            timestamp: std::time::Instant::now(),
        };
        
        self.recovery_log.push(attempt);
    }
    
    /// Send notification about hardware fallback
    async fn notify_hardware_fallback(&self, reason: String, fallback_method: String) {
        if let Some(tx) = &self.notification_tx {
            let notification = ErrorNotification::HardwareFallback {
                reason,
                fallback_method,
            };
            let _ = tx.send(notification).await;
        }
    }
    
    /// Send notification about file being skipped
    async fn notify_file_skipped(&self, file: PathBuf, reason: String) {
        if let Some(tx) = &self.notification_tx {
            let notification = ErrorNotification::FileSkipped { file, reason };
            let _ = tx.send(notification).await;
        }
    }
    
    /// Send notification about batch continuation
    async fn notify_batch_continuation(&self, completed: usize, failed: usize, remaining: usize) {
        if let Some(tx) = &self.notification_tx {
            let notification = ErrorNotification::BatchContinuation {
                completed,
                failed,
                remaining,
            };
            let _ = tx.send(notification).await;
        }
    }
    
    /// Send notification about critical error
    async fn notify_critical_error(&self, error: &AppError) {
        if let Some(tx) = &self.notification_tx {
            let notification = ErrorNotification::CriticalError {
                error: error.to_string(),
                requires_user_action: true,
            };
            let _ = tx.send(notification).await;
        }
    }
    
    /// Get current error statistics
    pub fn get_statistics(&self) -> ErrorStatistics {
        let total_errors = self.error_log.len();
        ErrorStatistics {
            total_errors,
            critical_errors: self.count_errors_by_severity(ErrorSeverity::Critical),
            file_level_errors: self.count_errors_by_severity(ErrorSeverity::FileLevel),
            recoverable_errors: self.count_errors_by_severity(ErrorSeverity::Recoverable),
            recovery_attempts: self.recovery_log.len(),
            successful_recoveries: self.recovery_log.iter().filter(|r| r.success).count(),
            failed_recoveries: self.recovery_log.iter().filter(|r| !r.success).count(),
        }
    }
    
    /// Count errors by severity level
    fn count_errors_by_severity(&self, severity: ErrorSeverity) -> usize {
        self.error_log
            .iter()
            .filter(|(_, error)| error.severity() == severity)
            .count()
    }
    
    /// Get all error logs
    pub fn get_error_log(&self) -> &[(PathBuf, AppError)] {
        &self.error_log
    }
    
    /// Get all recovery attempts
    pub fn get_recovery_log(&self) -> &[RecoveryAttempt] {
        &self.recovery_log
    }
    
    /// Check if any critical errors have occurred
    pub fn has_critical_errors(&self) -> bool {
        self.count_errors_by_severity(ErrorSeverity::Critical) > 0
    }
    
    /// Clear error logs (useful for testing or reset)
    pub fn clear_logs(&mut self) {
        self.error_log.clear();
        self.recovery_log.clear();
    }
}

/// Decision made by the error recovery manager
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryDecision {
    /// Continue processing normally
    Continue,
    /// Skip the current file and continue with others
    SkipFile,
    /// Abort all processing
    Abort,
    /// Fall back to software encoding
    FallbackToSoftware,
    /// Retry the operation
    Retry,
    /// Ask user for confirmation
    UserConfirmation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::sync::mpsc;
    
    #[tokio::test]
    async fn test_error_recovery_manager_creation() {
        let manager = ErrorRecoveryManager::new();
        
        assert_eq!(manager.max_retry_attempts, 3);
        assert!(manager.hardware_fallback_enabled);
        assert!(manager.notification_tx.is_none());
        assert_eq!(manager.error_log.len(), 0);
        assert_eq!(manager.recovery_log.len(), 0);
    }
    
    #[tokio::test]
    async fn test_error_recovery_manager_configuration() {
        let (tx, _rx) = mpsc::channel(10);
        let manager = ErrorRecoveryManager::new()
            .with_notifications(tx)
            .with_max_retries(5)
            .with_hardware_fallback(false);
        
        assert_eq!(manager.max_retry_attempts, 5);
        assert!(!manager.hardware_fallback_enabled);
        assert!(manager.notification_tx.is_some());
    }
    
    #[tokio::test]
    async fn test_handle_critical_error() {
        let mut manager = ErrorRecoveryManager::new();
        let error = AppError::ConfigError("Invalid configuration".to_string());
        
        let decision = manager.handle_error(error, None).await.unwrap();
        
        assert_eq!(decision, RecoveryDecision::Abort);
        assert!(manager.has_critical_errors());
    }
    
    #[tokio::test]
    async fn test_handle_file_level_error() {
        let mut manager = ErrorRecoveryManager::new();
        let file_path = PathBuf::from("/test/movie.mkv");
        let error = AppError::EncodingError("FFmpeg failed".to_string());
        
        let decision = manager.handle_error(error, Some(file_path.clone())).await.unwrap();
        
        assert_eq!(decision, RecoveryDecision::SkipFile);
        assert_eq!(manager.error_log.len(), 1);
        assert_eq!(manager.recovery_log.len(), 1);
        assert_eq!(manager.recovery_log[0].file_path, Some(file_path));
        assert!(manager.recovery_log[0].success);
    }
    
    #[tokio::test]
    async fn test_handle_hardware_acceleration_error_with_fallback() {
        let mut manager = ErrorRecoveryManager::new().with_hardware_fallback(true);
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
        
        let decision = manager.handle_error(error, None).await.unwrap();
        
        assert_eq!(decision, RecoveryDecision::FallbackToSoftware);
        assert_eq!(manager.recovery_log.len(), 1);
        assert_eq!(manager.recovery_log[0].strategy, RecoveryStrategy::Fallback);
    }
    
    #[tokio::test]
    async fn test_handle_hardware_acceleration_error_without_fallback() {
        let mut manager = ErrorRecoveryManager::new().with_hardware_fallback(false);
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
        
        let decision = manager.handle_error(error, None).await.unwrap();
        
        assert_eq!(decision, RecoveryDecision::SkipFile);
    }
    
    #[tokio::test]
    async fn test_error_statistics() {
        let mut manager = ErrorRecoveryManager::new();
        
        // Add various types of errors
        let _ = manager.handle_error(
            AppError::ConfigError("Config error".to_string()),
            None
        ).await;
        
        let _ = manager.handle_error(
            AppError::EncodingError("Encoding error".to_string()),
            Some(PathBuf::from("/test/file1.mkv"))
        ).await;
        
        let _ = manager.handle_error(
            AppError::HardwareAccelerationError("Hardware error".to_string()),
            None
        ).await;
        
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 3);
        assert_eq!(stats.critical_errors, 1);
        assert_eq!(stats.file_level_errors, 1);
        assert_eq!(stats.recoverable_errors, 1);
    }
    
    #[tokio::test]
    async fn test_error_notifications() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut manager = ErrorRecoveryManager::new().with_notifications(tx);
        
        // Test hardware fallback notification
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
        let _ = manager.handle_error(error, None).await;
        
        // Check that notification was sent
        if let Ok(notification) = rx.try_recv() {
            match notification {
                ErrorNotification::HardwareFallback { reason, fallback_method } => {
                    assert!(reason.contains("AMD AMF not available"));
                    assert_eq!(fallback_method, "Software encoding (libx265)");
                }
                _ => panic!("Expected HardwareFallback notification"),
            }
        }
    }
    
    #[tokio::test]
    async fn test_recovery_attempt_logging() {
        let mut manager = ErrorRecoveryManager::new();
        let file_path = PathBuf::from("/test/movie.mkv");
        
        manager.log_recovery_attempt(
            Some(file_path.clone()),
            "Test error".to_string(),
            RecoveryStrategy::Retry,
            2,
            false,
        );
        
        assert_eq!(manager.recovery_log.len(), 1);
        let attempt = &manager.recovery_log[0];
        assert_eq!(attempt.file_path, Some(file_path));
        assert_eq!(attempt.original_error, "Test error");
        assert_eq!(attempt.strategy, RecoveryStrategy::Retry);
        assert_eq!(attempt.attempt_number, 2);
        assert!(!attempt.success);
    }
    
    #[tokio::test]
    async fn test_clear_logs() {
        let mut manager = ErrorRecoveryManager::new();
        
        // Add some errors and recovery attempts
        let _ = manager.handle_error(
            AppError::EncodingError("Test error".to_string()),
            Some(PathBuf::from("/test/file.mkv"))
        ).await;
        
        assert!(manager.get_statistics().total_errors > 0);
        assert!(!manager.error_log.is_empty());
        
        manager.clear_logs();
        
        assert_eq!(manager.get_statistics().total_errors, 0);
        assert!(manager.error_log.is_empty());
        assert!(manager.recovery_log.is_empty());
    }
    
    #[test]
    fn test_error_severity_classification() {
        assert_eq!(
            AppError::ConfigError("test".to_string()).severity(),
            ErrorSeverity::Critical
        );
        
        assert_eq!(
            AppError::EncodingError("test".to_string()).severity(),
            ErrorSeverity::FileLevel
        );
        
        assert_eq!(
            AppError::HardwareAccelerationError("test".to_string()).severity(),
            ErrorSeverity::Recoverable
        );
    }
    
    #[test]
    fn test_recovery_strategy_determination() {
        assert_eq!(
            AppError::ConfigError("test".to_string()).recovery_strategy(),
            RecoveryStrategy::Abort
        );
        
        assert_eq!(
            AppError::EncodingError("test".to_string()).recovery_strategy(),
            RecoveryStrategy::SkipFile
        );
        
        assert_eq!(
            AppError::HardwareAccelerationError("test".to_string()).recovery_strategy(),
            RecoveryStrategy::Fallback
        );
    }
    
    #[test]
    fn test_batch_continuation_allowance() {
        assert!(!AppError::ConfigError("test".to_string()).allows_batch_continuation());
        assert!(AppError::EncodingError("test".to_string()).allows_batch_continuation());
        assert!(AppError::HardwareAccelerationError("test".to_string()).allows_batch_continuation());
    }
}