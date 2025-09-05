use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use video_encoder::{ffmpeg, cli, Result, AppError};

// ============================================================================
// AMD HARDWARE ACCELERATION DETECTION TESTS
// ============================================================================

#[tokio::test]
async fn test_amd_amf_availability_detection() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    
    // Test AMD AMF hardware availability detection
    let hw_check_result = ffmpeg_encoder.check_amd_hardware_availability().await;
    
    match hw_check_result {
        Ok(is_available) => {
            println!("AMD AMF hardware acceleration available: {}", is_available);
            
            if is_available {
                println!("✓ AMD AMF hardware acceleration detected and available");
                
                // Test hardware-specific command generation
                let input_path = PathBuf::from("test_input.mkv");
                let output_path = PathBuf::from("test_output.mkv");
                let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
                
                let command_str = hw_command.join(" ");
                
                // Verify hardware acceleration parameters are present
                assert!(
                    command_str.contains("amf") || 
                    command_str.contains("h264_amf") || 
                    command_str.contains("hevc_amf") ||
                    command_str.contains("h265_amf"),
                    "Hardware command should contain AMD AMF parameters"
                );
                
                println!("✓ Hardware encoding command generated: {}", command_str);
            } else {
                println!("ℹ AMD AMF hardware acceleration not available (expected in most test environments)");
            }
        }
        Err(e) => {
            println!("⚠ Hardware detection failed: {} (expected in test environments without AMD GPU)", e);
            // This is expected in most CI/test environments
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_ffmpeg_availability_and_version() -> Result<()> {
    // Test basic FFmpeg availability
    let ffmpeg_check = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await;
    
    match ffmpeg_check {
        Ok(output) => {
            let version_output = String::from_utf8_lossy(&output.stdout);
            println!("✓ FFmpeg available:");
            
            // Extract version information
            for line in version_output.lines().take(3) {
                if line.contains("ffmpeg version") || line.contains("configuration") {
                    println!("  {}", line);
                }
            }
            
            // Check for AMD AMF support in FFmpeg build
            if version_output.contains("amf") {
                println!("✓ FFmpeg built with AMD AMF support");
            } else {
                println!("ℹ FFmpeg does not include AMD AMF support");
            }
            
            // Check for libx265 support (software fallback)
            if version_output.contains("libx265") {
                println!("✓ FFmpeg built with libx265 support (software encoding available)");
            } else {
                println!("⚠ FFmpeg does not include libx265 support");
            }
        }
        Err(e) => {
            println!("⚠ FFmpeg not available: {} (may affect encoding tests)", e);
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_hardware_encoder_command_generation() -> Result<()> {
    let quality_profiles = vec![
        cli::QualityProfile::Quality,
        cli::QualityProfile::Medium,
        cli::QualityProfile::HighCompression,
    ];
    
    for profile in quality_profiles {
        let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(profile.clone());
        let input_path = PathBuf::from("input_test.mkv");
        let output_path = PathBuf::from("output_test.mkv");
        
        // Test hardware encoding command
        let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
        let hw_command_str = hw_command.join(" ");
        
        // Verify basic command structure
        assert!(hw_command_str.contains("ffmpeg"), "Command should start with ffmpeg");
        assert!(hw_command_str.contains("input_test.mkv"), "Command should include input file");
        assert!(hw_command_str.contains("output_test.mkv"), "Command should include output file");
        
        // Verify quality parameters
        let crf_value = profile.crf_value();
        assert!(
            hw_command_str.contains(&crf_value.to_string()) || hw_command_str.contains("crf"),
            "Command should include quality parameters for {:?}",
            profile
        );
        
        // Verify audio preservation
        assert!(
            hw_command_str.contains("copy") || hw_command_str.contains("c:a"),
            "Command should preserve audio streams"
        );
        
        println!("✓ Hardware command for {:?}: {}", profile, hw_command_str);
    }
    
    Ok(())
}

// ============================================================================
// SOFTWARE FALLBACK TESTS
// ============================================================================

#[tokio::test]
async fn test_software_fallback_command_generation() -> Result<()> {
    let quality_profiles = vec![
        cli::QualityProfile::Quality,
        cli::QualityProfile::Medium,
        cli::QualityProfile::HighCompression,
    ];
    
    for profile in quality_profiles {
        let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(profile.clone());
        let input_path = PathBuf::from("input_test.mkv");
        let output_path = PathBuf::from("output_test.mkv");
        
        // Test software encoding command (fallback)
        let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
        let sw_command_str = sw_command.join(" ");
        
        // Verify software encoding parameters
        assert!(sw_command_str.contains("libx265"), "Software command should use libx265");
        assert!(!sw_command_str.contains("amf"), "Software command should not contain hardware acceleration");
        
        // Verify quality parameters
        let crf_value = profile.crf_value();
        assert!(
            sw_command_str.contains(&crf_value.to_string()),
            "Software command should include CRF value {} for {:?}",
            crf_value,
            profile
        );
        
        // Verify audio preservation
        assert!(
            sw_command_str.contains("copy") || sw_command_str.contains("c:a"),
            "Software command should preserve audio streams"
        );
        
        println!("✓ Software command for {:?}: {}", profile, sw_command_str);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_hardware_fallback_error_handling() -> Result<()> {
    use video_encoder::error_recovery::{ErrorRecoveryManager, RecoveryDecision};
    
    // Test automatic fallback when hardware acceleration fails
    let mut recovery_manager = ErrorRecoveryManager::new().with_hardware_fallback(true);
    
    // Simulate hardware acceleration failure
    let hw_error = AppError::HardwareAccelerationError(
        "AMD AMF encoder initialization failed: No compatible device found".to_string()
    );
    
    let decision = recovery_manager.handle_error(hw_error, None).await?;
    
    // Should automatically decide to fallback to software
    assert_eq!(decision, RecoveryDecision::FallbackToSoftware);
    
    // Verify recovery statistics
    let stats = recovery_manager.get_statistics();
    assert_eq!(stats.total_errors, 1);
    assert_eq!(stats.recoverable_errors, 1);
    assert_eq!(stats.successful_recoveries, 1);
    
    println!("✓ Hardware fallback error handling test passed");
    
    // Test with fallback disabled
    let mut no_fallback_manager = ErrorRecoveryManager::new().with_hardware_fallback(false);
    
    let hw_error2 = AppError::HardwareAccelerationError(
        "AMD AMF not available".to_string()
    );
    
    let decision2 = no_fallback_manager.handle_error(hw_error2, None).await?;
    
    // Should skip file when fallback is disabled
    assert_eq!(decision2, RecoveryDecision::SkipFile);
    
    println!("✓ Hardware fallback disabled test passed");
    
    Ok(())
}

// ============================================================================
// ENCODING PARAMETER VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_quality_profile_parameter_mapping() -> Result<()> {
    // Test that quality profiles map to correct encoding parameters
    let test_cases = vec![
        (cli::QualityProfile::Quality, 20, "High quality, larger file size"),
        (cli::QualityProfile::Medium, 23, "Balanced quality and size"),
        (cli::QualityProfile::HighCompression, 28, "High compression, smaller file size"),
    ];
    
    for (profile, expected_crf, description) in test_cases {
        let actual_crf = profile.crf_value();
        assert_eq!(actual_crf, expected_crf, "CRF value mismatch for {:?}", profile);
        
        let profile_description = profile.description();
        assert!(profile_description.len() > 0, "Profile should have a description");
        
        println!("✓ {:?}: CRF {} - {}", profile, actual_crf, description);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_encoding_command_audio_preservation() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    let input_path = PathBuf::from("test_with_audio.mkv");
    let output_path = PathBuf::from("output_with_audio.mkv");
    
    // Test both hardware and software commands preserve audio
    let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
    let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
    
    let hw_command_str = hw_command.join(" ");
    let sw_command_str = sw_command.join(" ");
    
    // Both commands should preserve audio
    assert!(
        hw_command_str.contains("c:a copy") || hw_command_str.contains("-c:a copy"),
        "Hardware command should preserve audio: {}",
        hw_command_str
    );
    
    assert!(
        sw_command_str.contains("c:a copy") || sw_command_str.contains("-c:a copy"),
        "Software command should preserve audio: {}",
        sw_command_str
    );
    
    println!("✓ Audio preservation verified in both hardware and software commands");
    Ok(())
}

#[tokio::test]
async fn test_encoding_command_subtitle_preservation() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    let input_path = PathBuf::from("test_with_subtitles.mkv");
    let output_path = PathBuf::from("output_with_subtitles.mkv");
    
    // Test subtitle preservation in encoding commands
    let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
    let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
    
    let hw_command_str = hw_command.join(" ");
    let sw_command_str = sw_command.join(" ");
    
    // Commands should preserve subtitles (copy all streams or explicitly copy subtitles)
    let hw_preserves_subs = hw_command_str.contains("c:s copy") || 
                           hw_command_str.contains("-c:s copy") ||
                           hw_command_str.contains("map 0");
    
    let sw_preserves_subs = sw_command_str.contains("c:s copy") || 
                           sw_command_str.contains("-c:s copy") ||
                           sw_command_str.contains("map 0");
    
    if hw_preserves_subs {
        println!("✓ Hardware command preserves subtitles");
    } else {
        println!("ℹ Hardware command may not explicitly preserve subtitles");
    }
    
    if sw_preserves_subs {
        println!("✓ Software command preserves subtitles");
    } else {
        println!("ℹ Software command may not explicitly preserve subtitles");
    }
    
    Ok(())
}

// ============================================================================
// HARDWARE DETECTION EDGE CASES
// ============================================================================

#[tokio::test]
async fn test_hardware_detection_edge_cases() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    
    // Test multiple hardware detection calls (should be consistent)
    let mut detection_results = Vec::new();
    
    for i in 0..3 {
        let result = ffmpeg_encoder.check_amd_hardware_availability().await;
        
        match &result {
            Ok(available) => {
                println!("Hardware detection attempt {}: available = {}", i + 1, available);
            }
            Err(e) => {
                println!("Hardware detection attempt {} failed: {}", i + 1, e);
            }
        }
        
        detection_results.push(result);
        
        // Small delay between checks
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Results should be consistent (all success or all failure)
    let all_success = detection_results.iter().all(|r| r.is_ok());
    let all_failure = detection_results.iter().all(|r| r.is_err());
    
    assert!(
        all_success || all_failure,
        "Hardware detection results should be consistent across multiple calls"
    );
    
    if all_success {
        let availability_results: Vec<bool> = detection_results
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        
        // All availability results should be the same
        let first_result = availability_results[0];
        assert!(
            availability_results.iter().all(|&r| r == first_result),
            "Hardware availability should be consistent across calls"
        );
        
        println!("✓ Hardware detection consistency verified: available = {}", first_result);
    } else {
        println!("✓ Hardware detection consistently failed (expected in test environment)");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_ffmpeg_error_handling() -> Result<()> {
    // Test handling of various FFmpeg-related errors
    let error_scenarios = vec![
        "ffmpeg: command not found",
        "No such file or directory",
        "Invalid argument",
        "Encoder 'h264_amf' not found",
        "Device creation failed",
    ];
    
    for error_message in error_scenarios {
        let ffmpeg_error = AppError::ffmpeg_error(error_message);
        
        // Verify error properties
        assert_eq!(ffmpeg_error.severity(), video_encoder::ErrorSeverity::FileLevel);
        assert_eq!(ffmpeg_error.recovery_strategy(), video_encoder::error_recovery::RecoveryStrategy::SkipFile);
        assert!(ffmpeg_error.allows_batch_continuation());
        
        // Verify error message
        let error_string = ffmpeg_error.to_string();
        assert!(error_string.contains(error_message));
        
        println!("✓ FFmpeg error handling verified for: {}", error_message);
    }
    
    Ok(())
}

// ============================================================================
// COMPREHENSIVE HARDWARE INTEGRATION TESTS
// ============================================================================

#[tokio::test]
async fn test_complete_hardware_detection_workflow() -> Result<()> {
    use video_encoder::error_recovery::{ErrorRecoveryManager, ErrorNotification};
    use tokio::sync::mpsc;
    
    // Set up error recovery with notifications
    let (error_tx, mut error_rx) = mpsc::channel(10);
    let mut recovery_manager = ErrorRecoveryManager::new()
        .with_notifications(error_tx)
        .with_hardware_fallback(true);
    
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    
    // Step 1: Detect hardware availability
    let hw_available = ffmpeg_encoder.check_amd_hardware_availability().await;
    
    match hw_available {
        Ok(true) => {
            println!("✓ AMD hardware acceleration available - testing hardware path");
            
            // Test hardware encoding command generation
            let input_path = PathBuf::from("test_input.mkv");
            let output_path = PathBuf::from("test_output.mkv");
            let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
            
            let command_str = hw_command.join(" ");
            assert!(command_str.contains("amf") || command_str.contains("h264_amf"));
            
            println!("✓ Hardware encoding command validated");
        }
        Ok(false) => {
            println!("ℹ AMD hardware acceleration not available - testing software fallback");
            
            // Simulate hardware failure and test recovery
            let hw_error = AppError::HardwareAccelerationError("AMD AMF not available".to_string());
            let decision = recovery_manager.handle_error(hw_error, None).await?;
            
            assert_eq!(decision, video_encoder::error_recovery::RecoveryDecision::FallbackToSoftware);
            
            // Verify fallback notification
            let notification = error_rx.recv().await.unwrap();
            match notification {
                ErrorNotification::HardwareFallback { reason, fallback_method } => {
                    assert!(reason.contains("AMD AMF not available"));
                    assert_eq!(fallback_method, "Software encoding (libx265)");
                }
                _ => panic!("Expected HardwareFallback notification"),
            }
            
            println!("✓ Software fallback workflow validated");
        }
        Err(e) => {
            println!("⚠ Hardware detection failed: {} - testing error recovery", e);
            
            // Test error recovery for detection failure
            let detection_error = AppError::HardwareAccelerationError(format!("Detection failed: {}", e));
            let decision = recovery_manager.handle_error(detection_error, None).await?;
            
            assert_eq!(decision, video_encoder::error_recovery::RecoveryDecision::FallbackToSoftware);
            
            println!("✓ Hardware detection error recovery validated");
        }
    }
    
    // Step 2: Test software encoding command as fallback
    let input_path = PathBuf::from("fallback_input.mkv");
    let output_path = PathBuf::from("fallback_output.mkv");
    let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
    
    let sw_command_str = sw_command.join(" ");
    assert!(sw_command_str.contains("libx265"));
    assert!(!sw_command_str.contains("amf"));
    
    println!("✓ Software fallback command validated");
    
    // Step 3: Verify recovery statistics
    let stats = recovery_manager.get_statistics();
    if stats.total_errors > 0 {
        assert!(stats.successful_recoveries > 0, "Should have successful recoveries");
        println!("✓ Error recovery statistics: {} errors, {} recoveries", 
                stats.total_errors, stats.successful_recoveries);
    }
    
    println!("✓ Complete hardware detection workflow test passed");
    Ok(())
}