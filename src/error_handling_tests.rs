#[cfg(test)]
mod error_handling_tests {
    use crate::{AppError, ErrorSeverity};
    use crate::error_recovery::{ErrorRecoveryManager, ErrorNotification, RecoveryDecision, RecoveryStrategy};
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    #[test]
    fn test_error_severity_classification() {
        // Test critical errors
        assert_eq!(
            AppError::ConfigError("Invalid config".to_string()).severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(
            AppError::UserCancellation.severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(
            AppError::SystemResourceError("Out of memory".to_string()).severity(),
            ErrorSeverity::Critical
        );

        // Test file-level errors
        assert_eq!(
            AppError::EncodingError("FFmpeg failed".to_string()).severity(),
            ErrorSeverity::FileLevel
        );
        assert_eq!(
            AppError::IoError("File not found".to_string()).severity(),
            ErrorSeverity::FileLevel
        );
        assert_eq!(
            AppError::FileProcessingError {
                file: PathBuf::from("/test/file.mkv"),
                error: "Processing failed".to_string()
            }.severity(),
            ErrorSeverity::FileLevel
        );

        // Test recoverable errors
        assert_eq!(
            AppError::HardwareAccelerationError("AMD AMF not available".to_string()).severity(),
            ErrorSeverity::Recoverable
        );

        // Test warnings
        assert_eq!(
            AppError::BatchProcessingError { completed: 5, failed: 2 }.severity(),
            ErrorSeverity::Warning
        );
    }

    #[test]
    fn test_recovery_strategy_determination() {
        // Test abort strategies
        assert_eq!(
            AppError::ConfigError("Invalid config".to_string()).recovery_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            AppError::UserCancellation.recovery_strategy(),
            RecoveryStrategy::Abort
        );

        // Test skip file strategies
        assert_eq!(
            AppError::EncodingError("FFmpeg failed".to_string()).recovery_strategy(),
            RecoveryStrategy::SkipFile
        );
        assert_eq!(
            AppError::FileProcessingError {
                file: PathBuf::from("/test/file.mkv"),
                error: "Processing failed".to_string()
            }.recovery_strategy(),
            RecoveryStrategy::SkipFile
        );

        // Test fallback strategies
        assert_eq!(
            AppError::HardwareAccelerationError("AMD AMF not available".to_string()).recovery_strategy(),
            RecoveryStrategy::Fallback
        );

        // Test user confirmation strategies
        assert_eq!(
            AppError::BatchProcessingError { completed: 5, failed: 2 }.recovery_strategy(),
            RecoveryStrategy::UserConfirmation
        );
    }

    #[test]
    fn test_batch_continuation_allowance() {
        // Critical errors should not allow batch continuation
        assert!(!AppError::ConfigError("Invalid config".to_string()).allows_batch_continuation());
        assert!(!AppError::UserCancellation.allows_batch_continuation());
        assert!(!AppError::SystemResourceError("Out of memory".to_string()).allows_batch_continuation());

        // File-level errors should allow batch continuation
        assert!(AppError::EncodingError("FFmpeg failed".to_string()).allows_batch_continuation());
        assert!(AppError::FileProcessingError {
            file: PathBuf::from("/test/file.mkv"),
            error: "Processing failed".to_string()
        }.allows_batch_continuation());

        // Recoverable errors should allow batch continuation
        assert!(AppError::HardwareAccelerationError("AMD AMF not available".to_string()).allows_batch_continuation());

        // Warnings should allow batch continuation
        assert!(AppError::BatchProcessingError { completed: 5, failed: 2 }.allows_batch_continuation());
    }

    #[test]
    fn test_error_creation_helpers() {
        let file_path = PathBuf::from("/test/movie.mkv");
        let error = AppError::file_error(file_path.clone(), "Encoding failed");
        
        match error {
            AppError::FileProcessingError { file, error } => {
                assert_eq!(file, file_path);
                assert_eq!(error, "Encoding failed");
            }
            _ => panic!("Expected FileProcessingError"),
        }

        let hw_error = AppError::hardware_error("AMD AMF initialization failed");
        match hw_error {
            AppError::HardwareAccelerationError(msg) => {
                assert_eq!(msg, "AMD AMF initialization failed");
            }
            _ => panic!("Expected HardwareAccelerationError"),
        }

        let ffmpeg_error = AppError::ffmpeg_error("Process exited with code 1");
        match ffmpeg_error {
            AppError::FFmpegError(msg) => {
                assert_eq!(msg, "Process exited with code 1");
            }
            _ => panic!("Expected FFmpegError"),
        }
    }

    #[tokio::test]
    async fn test_error_recovery_manager_critical_error_handling() {
        let mut manager = ErrorRecoveryManager::new();
        let error = AppError::ConfigError("Invalid configuration file".to_string());

        let decision = manager.handle_error(error, None).await.unwrap();

        assert_eq!(decision, RecoveryDecision::Abort);
        assert!(manager.has_critical_errors());
        
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 1);
        assert_eq!(stats.critical_errors, 1);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_file_level_error_handling() {
        let mut manager = ErrorRecoveryManager::new();
        let file_path = PathBuf::from("/test/movie.mkv");
        let error = AppError::EncodingError("FFmpeg encoding failed".to_string());

        let decision = manager.handle_error(error, Some(file_path.clone())).await.unwrap();

        assert_eq!(decision, RecoveryDecision::SkipFile);
        assert!(!manager.has_critical_errors());
        
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 1);
        assert_eq!(stats.file_level_errors, 1);
        
        let error_log = manager.get_error_log();
        assert_eq!(error_log.len(), 1);
        assert_eq!(error_log[0].0, file_path);
        
        let recovery_log = manager.get_recovery_log();
        assert_eq!(recovery_log.len(), 1);
        assert_eq!(recovery_log[0].file_path, Some(file_path));
        assert!(recovery_log[0].success);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_hardware_fallback_enabled() {
        let mut manager = ErrorRecoveryManager::new().with_hardware_fallback(true);
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());

        let decision = manager.handle_error(error, None).await.unwrap();

        assert_eq!(decision, RecoveryDecision::FallbackToSoftware);
        
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 1);
        assert_eq!(stats.recoverable_errors, 1);
        
        let recovery_log = manager.get_recovery_log();
        assert_eq!(recovery_log.len(), 1);
        assert_eq!(recovery_log[0].strategy, RecoveryStrategy::Fallback);
        assert!(recovery_log[0].success);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_hardware_fallback_disabled() {
        let mut manager = ErrorRecoveryManager::new().with_hardware_fallback(false);
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());

        let decision = manager.handle_error(error, None).await.unwrap();

        assert_eq!(decision, RecoveryDecision::SkipFile);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_notifications() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut manager = ErrorRecoveryManager::new().with_notifications(tx);

        // Test hardware fallback notification
        let error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
        let _ = manager.handle_error(error, None).await;

        let notification = rx.recv().await.unwrap();
        match notification {
            ErrorNotification::HardwareFallback { reason, fallback_method } => {
                assert!(reason.contains("AMD AMF not available"));
                assert_eq!(fallback_method, "Software encoding (libx265)");
            }
            _ => panic!("Expected HardwareFallback notification"),
        }

        // Test file skipped notification
        let file_path = PathBuf::from("/test/movie.mkv");
        let error = AppError::EncodingError("FFmpeg failed".to_string());
        let _ = manager.handle_error(error, Some(file_path.clone())).await;

        let notification = rx.recv().await.unwrap();
        match notification {
            ErrorNotification::FileSkipped { file, reason } => {
                assert_eq!(file, file_path);
                assert!(reason.contains("FFmpeg failed"));
            }
            _ => panic!("Expected FileSkipped notification"),
        }
    }

    #[tokio::test]
    async fn test_error_recovery_manager_statistics_tracking() {
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

        let _ = manager.handle_error(
            AppError::BatchProcessingError { completed: 5, failed: 2 },
            None
        ).await;

        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 4);
        assert_eq!(stats.critical_errors, 1);
        assert_eq!(stats.file_level_errors, 1);
        assert_eq!(stats.recoverable_errors, 1);
        assert!(stats.recovery_attempts > 0);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_configuration() {
        let (tx, _rx) = mpsc::channel(10);
        let mut manager = ErrorRecoveryManager::new()
            .with_notifications(tx)
            .with_max_retries(5)
            .with_hardware_fallback(false);

        // Test that configuration is applied by testing behavior
        // Hardware fallback disabled should result in SkipFile instead of FallbackToSoftware
        let hw_error = AppError::HardwareAccelerationError("Test error".to_string());
        let decision = manager.handle_error(hw_error, None).await.unwrap();
        assert_eq!(decision, RecoveryDecision::SkipFile);
    }

    #[tokio::test]
    async fn test_error_recovery_manager_log_clearing() {
        let mut manager = ErrorRecoveryManager::new();

        // Add some errors
        let _ = manager.handle_error(
            AppError::EncodingError("Test error".to_string()),
            Some(PathBuf::from("/test/file.mkv"))
        ).await;

        assert!(manager.get_statistics().total_errors > 0);
        assert!(!manager.get_error_log().is_empty());

        manager.clear_logs();

        assert_eq!(manager.get_statistics().total_errors, 0);
        assert!(manager.get_error_log().is_empty());
        assert!(manager.get_recovery_log().is_empty());
    }

    #[test]
    fn test_error_notification_types() {
        // Test HardwareFallback notification
        let hw_notification = ErrorNotification::HardwareFallback {
            reason: "AMD AMF not available".to_string(),
            fallback_method: "Software encoding".to_string(),
        };
        
        match hw_notification {
            ErrorNotification::HardwareFallback { reason, fallback_method } => {
                assert_eq!(reason, "AMD AMF not available");
                assert_eq!(fallback_method, "Software encoding");
            }
            _ => panic!("Expected HardwareFallback notification"),
        }

        // Test FileSkipped notification
        let file_notification = ErrorNotification::FileSkipped {
            file: PathBuf::from("/test/movie.mkv"),
            reason: "Encoding failed".to_string(),
        };
        
        match file_notification {
            ErrorNotification::FileSkipped { file, reason } => {
                assert_eq!(file, PathBuf::from("/test/movie.mkv"));
                assert_eq!(reason, "Encoding failed");
            }
            _ => panic!("Expected FileSkipped notification"),
        }

        // Test CriticalError notification
        let critical_notification = ErrorNotification::CriticalError {
            error: "System failure".to_string(),
            requires_user_action: true,
        };
        
        match critical_notification {
            ErrorNotification::CriticalError { error, requires_user_action } => {
                assert_eq!(error, "System failure");
                assert!(requires_user_action);
            }
            _ => panic!("Expected CriticalError notification"),
        }
    }

    #[test]
    fn test_recovery_decision_types() {
        assert_eq!(RecoveryDecision::Continue, RecoveryDecision::Continue);
        assert_eq!(RecoveryDecision::SkipFile, RecoveryDecision::SkipFile);
        assert_eq!(RecoveryDecision::Abort, RecoveryDecision::Abort);
        assert_eq!(RecoveryDecision::FallbackToSoftware, RecoveryDecision::FallbackToSoftware);
        assert_eq!(RecoveryDecision::Retry, RecoveryDecision::Retry);
        assert_eq!(RecoveryDecision::UserConfirmation, RecoveryDecision::UserConfirmation);

        assert_ne!(RecoveryDecision::Continue, RecoveryDecision::Abort);
        assert_ne!(RecoveryDecision::SkipFile, RecoveryDecision::Retry);
    }

    #[tokio::test]
    async fn test_multiple_error_scenarios() {
        let mut manager = ErrorRecoveryManager::new().with_hardware_fallback(true);

        // Scenario 1: Hardware failure followed by file error
        let hw_error = AppError::HardwareAccelerationError("AMD driver issue".to_string());
        let hw_decision = manager.handle_error(hw_error, None).await.unwrap();
        assert_eq!(hw_decision, RecoveryDecision::FallbackToSoftware);

        let file_error = AppError::EncodingError("Corrupted file".to_string());
        let file_path = PathBuf::from("/test/corrupted.mkv");
        let file_decision = manager.handle_error(file_error, Some(file_path)).await.unwrap();
        assert_eq!(file_decision, RecoveryDecision::SkipFile);

        // Check that both errors are tracked
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 2);
        assert_eq!(stats.recoverable_errors, 1);
        assert_eq!(stats.file_level_errors, 1);
        assert_eq!(stats.recovery_attempts, 2);
        assert_eq!(stats.successful_recoveries, 2);
    }

    #[tokio::test]
    async fn test_error_recovery_with_batch_processing() {
        let mut manager = ErrorRecoveryManager::new();

        // Simulate batch processing with mixed results
        let batch_error = AppError::BatchProcessingError {
            completed: 8,
            failed: 2,
        };

        let decision = manager.handle_error(batch_error, None).await.unwrap();
        assert_eq!(decision, RecoveryDecision::Continue);

        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 1);
        assert!(stats.total_errors > 0);
    }

    #[test]
    fn test_error_display_formatting() {
        let config_error = AppError::ConfigError("Invalid target directory".to_string());
        assert!(config_error.to_string().contains("Configuration error"));
        assert!(config_error.to_string().contains("Invalid target directory"));

        let file_error = AppError::FileProcessingError {
            file: PathBuf::from("/test/movie.mkv"),
            error: "Encoding failed".to_string(),
        };
        assert!(file_error.to_string().contains("File processing error"));
        assert!(file_error.to_string().contains("movie.mkv"));
        assert!(file_error.to_string().contains("Encoding failed"));

        let hw_error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
        assert!(hw_error.to_string().contains("Hardware acceleration error"));
        assert!(hw_error.to_string().contains("AMD AMF not available"));
    }

    #[test]
    fn test_error_recovery_decision_mapping() {
        
        // Test that different error types map to appropriate recovery decisions
        let critical_error = AppError::ConfigError("Invalid config".to_string());
        assert_eq!(critical_error.recovery_strategy(), RecoveryStrategy::Abort);
        
        let file_error = AppError::EncodingError("FFmpeg failed".to_string());
        assert_eq!(file_error.recovery_strategy(), RecoveryStrategy::SkipFile);
        
        let hw_error = AppError::HardwareAccelerationError("AMD not available".to_string());
        assert_eq!(hw_error.recovery_strategy(), RecoveryStrategy::Fallback);
        
        let batch_error = AppError::BatchProcessingError { completed: 5, failed: 2 };
        assert_eq!(batch_error.recovery_strategy(), RecoveryStrategy::UserConfirmation);
    }

    #[tokio::test]
    async fn test_comprehensive_error_recovery_workflow() {
        let mut manager = ErrorRecoveryManager::new()
            .with_hardware_fallback(true)
            .with_max_retries(2);

        // Scenario: Hardware failure -> File error -> Batch completion
        
        // 1. Hardware acceleration fails, should fallback
        let hw_error = AppError::HardwareAccelerationError("AMD driver crash".to_string());
        let decision1 = manager.handle_error(hw_error, None).await.unwrap();
        assert_eq!(decision1, RecoveryDecision::FallbackToSoftware);

        // 2. Individual file fails, should skip
        let file_error = AppError::EncodingError("Corrupted video stream".to_string());
        let file_path = PathBuf::from("/test/corrupted.mkv");
        let decision2 = manager.handle_error(file_error, Some(file_path)).await.unwrap();
        assert_eq!(decision2, RecoveryDecision::SkipFile);

        // 3. Batch processing completes with mixed results
        let batch_error = AppError::BatchProcessingError { completed: 8, failed: 2 };
        let decision3 = manager.handle_error(batch_error, None).await.unwrap();
        assert_eq!(decision3, RecoveryDecision::Continue);

        // Verify comprehensive statistics
        let stats = manager.get_statistics();
        assert_eq!(stats.total_errors, 3);
        assert_eq!(stats.recoverable_errors, 1);
        assert_eq!(stats.file_level_errors, 1);
        assert!(stats.recovery_attempts >= 3);
        assert!(stats.successful_recoveries >= 2);
    }

    #[tokio::test]
    async fn test_error_notification_comprehensive_coverage() {
        let (tx, mut rx) = mpsc::channel(20);
        let mut manager = ErrorRecoveryManager::new().with_notifications(tx);

        // Test all notification types
        let errors = vec![
            AppError::HardwareAccelerationError("AMD AMF initialization failed".to_string()),
            AppError::EncodingError("FFmpeg process crashed".to_string()),
            AppError::BatchProcessingError { completed: 5, failed: 3 },
        ];

        let file_paths = vec![
            None,
            Some(PathBuf::from("/test/video1.mkv")),
            None,
        ];

        for (error, file_path) in errors.into_iter().zip(file_paths.into_iter()) {
            let _ = manager.handle_error(error, file_path).await;
        }

        // Collect all notifications
        let mut notifications = Vec::new();
        while let Ok(notification) = rx.try_recv() {
            notifications.push(notification);
        }

        // Should have received notifications for hardware fallback and file skip
        assert!(!notifications.is_empty());
        
        // Check for hardware fallback notification
        let has_hw_fallback = notifications.iter().any(|n| {
            matches!(n, ErrorNotification::HardwareFallback { .. })
        });
        assert!(has_hw_fallback);

        // Check for file skipped notification
        let has_file_skip = notifications.iter().any(|n| {
            matches!(n, ErrorNotification::FileSkipped { .. })
        });
        assert!(has_file_skip);
    }

    #[test]
    fn test_error_severity_and_recovery_consistency() {
        // Test that error severity and recovery strategy are consistent
        let test_cases = vec![
            (AppError::ConfigError("test".to_string()), ErrorSeverity::Critical, RecoveryStrategy::Abort),
            (AppError::UserCancellation, ErrorSeverity::Critical, RecoveryStrategy::Abort),
            (AppError::EncodingError("test".to_string()), ErrorSeverity::FileLevel, RecoveryStrategy::SkipFile),
            (AppError::HardwareAccelerationError("test".to_string()), ErrorSeverity::Recoverable, RecoveryStrategy::Fallback),
            (AppError::BatchProcessingError { completed: 1, failed: 1 }, ErrorSeverity::Warning, RecoveryStrategy::UserConfirmation),
        ];

        for (error, expected_severity, expected_strategy) in test_cases {
            assert_eq!(error.severity(), expected_severity, "Severity mismatch for {:?}", error);
            assert_eq!(error.recovery_strategy(), expected_strategy, "Recovery strategy mismatch for {:?}", error);
            
            // Verify batch continuation allowance is consistent with severity
            let allows_continuation = error.allows_batch_continuation();
            let should_allow = !matches!(expected_severity, ErrorSeverity::Critical);
            assert_eq!(allows_continuation, should_allow, "Batch continuation allowance inconsistent for {:?}", error);
        }
    }

    #[tokio::test]
    async fn test_error_recovery_with_retry_limits() {
        let mut manager = ErrorRecoveryManager::new().with_max_retries(2);

        // Simulate multiple failures of the same type
        let file_path = PathBuf::from("/test/problematic.mkv");
        
        for attempt in 1..=3 {
            let error = AppError::EncodingError(format!("Attempt {} failed", attempt));
            let decision = manager.handle_error(error, Some(file_path.clone())).await.unwrap();
            
            // All should result in skipping the file (no retry logic implemented yet)
            assert_eq!(decision, RecoveryDecision::SkipFile);
        }

        // Verify that multiple attempts are logged
        let recovery_log = manager.get_recovery_log();
        assert_eq!(recovery_log.len(), 3);
        
        // All attempts should be for the same file
        for attempt in recovery_log {
            assert_eq!(attempt.file_path, Some(file_path.clone()));
            assert_eq!(attempt.strategy, RecoveryStrategy::SkipFile);
        }
    }

    #[test]
    fn test_error_helper_functions() {
        // Test error creation helpers
        let file_path = PathBuf::from("/test/video.mkv");
        
        let file_error = AppError::file_error(file_path.clone(), "Custom error message");
        match file_error {
            AppError::FileProcessingError { file, error } => {
                assert_eq!(file, file_path);
                assert_eq!(error, "Custom error message");
            }
            _ => panic!("Expected FileProcessingError"),
        }

        let hw_error = AppError::hardware_error("Custom hardware error");
        match hw_error {
            AppError::HardwareAccelerationError(msg) => {
                assert_eq!(msg, "Custom hardware error");
            }
            _ => panic!("Expected HardwareAccelerationError"),
        }

        let ffmpeg_error = AppError::ffmpeg_error("Custom FFmpeg error");
        match ffmpeg_error {
            AppError::FFmpegError(msg) => {
                assert_eq!(msg, "Custom FFmpeg error");
            }
            _ => panic!("Expected FFmpegError"),
        }
    }
}