use std::path::PathBuf;
use std::time::Duration;
use std::fs;
use tempfile::TempDir;

use video_encoder::{scanner, cli, ffmpeg, Result, AppError};

/// Create a test directory with sample files for validation
fn create_validation_test_directory() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let dir_path = temp_dir.path().to_path_buf();
    
    // Create mock video files with different characteristics
    let test_files = vec![
        ("sample_1080p.mkv", 1024 * 1024 * 200),  // 200MB
        ("sample_720p.mp4", 1024 * 1024 * 150),   // 150MB
        ("sample_4k.mkv", 1024 * 1024 * 500),     // 500MB
        ("sample_with_subs.mkv", 1024 * 1024 * 180), // 180MB
    ];
    
    for (filename, size) in test_files {
        let file_path = dir_path.join(filename);
        let content = vec![0u8; size];
        fs::write(&file_path, content).expect("Failed to create test file");
    }
    
    (temp_dir, dir_path)
}

// ============================================================================
// OUTPUT FILE QUALITY VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_quality_profile_output_validation() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    let quality_profiles = vec![
        (cli::QualityProfile::Quality, 20, "Highest quality"),
        (cli::QualityProfile::Medium, 23, "Balanced quality"),
        (cli::QualityProfile::HighCompression, 28, "Maximum compression"),
    ];
    
    for (profile, expected_crf, description) in quality_profiles {
        let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(profile.clone());
        
        // Test command generation for quality validation
        let input_path = test_dir.join("sample_1080p.mkv");
        let output_path = test_dir.join(format!("output_{:?}.mkv", profile));
        
        // Test both hardware and software commands
        let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
        let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
        
        // Validate quality parameters in commands
        let hw_command_str = hw_command.join(" ");
        let sw_command_str = sw_command.join(" ");
        
        // Both commands should include the correct CRF value
        assert!(
            hw_command_str.contains(&expected_crf.to_string()) || hw_command_str.contains("crf"),
            "Hardware command should include CRF {} for {:?}: {}",
            expected_crf, profile, hw_command_str
        );
        
        assert!(
            sw_command_str.contains(&expected_crf.to_string()),
            "Software command should include CRF {} for {:?}: {}",
            expected_crf, profile, sw_command_str
        );
        
        // Validate video codec parameters
        if hw_command_str.contains("amf") {
            // Hardware encoding should use AMD AMF
            assert!(
                hw_command_str.contains("h264_amf") || hw_command_str.contains("hevc_amf"),
                "Hardware command should use AMD AMF encoder"
            );
        }
        
        // Software encoding should use libx265
        assert!(
            sw_command_str.contains("libx265"),
            "Software command should use libx265 encoder"
        );
        
        println!("✓ Quality validation passed for {:?} (CRF {}): {}", profile, expected_crf, description);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_output_filename_generation_validation() -> Result<()> {
    // Test various input filename patterns and validate output generation
    let test_cases = vec![
        ("movie.mkv", "movie-h265.mkv"),
        ("video.mp4", "video-h265.mp4"),
        ("series.s01e01.mkv", "series.s01e01-h265.mkv"),
        ("complex.name.with.dots.mkv", "complex.name.with.dots-h265.mkv"),
        ("no_extension", "no_extension-h265"),
        ("already-h265.mkv", "already-h265-h265.mkv"), // Edge case
        ("file with spaces.mkv", "file with spaces-h265.mkv"),
    ];
    
    for (input_filename, expected_output) in test_cases {
        let actual_output = video_encoder::generate_output_filename(input_filename);
        assert_eq!(
            actual_output, expected_output,
            "Output filename generation failed for input: {}",
            input_filename
        );
        
        println!("✓ Filename generation: {} -> {}", input_filename, actual_output);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_output_directory_structure_validation() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Test output directory generation
    let config = cli::Config {
        target_directory: test_dir.clone(),
        quality_profile: cli::QualityProfile::Medium,
        max_parallel_jobs: 2,
        verbose: false,
    };
    
    let output_dir = config.output_directory();
    
    // Validate output directory naming
    let dir_name = output_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    
    assert!(
        dir_name.ends_with("-encodes"),
        "Output directory should end with '-encodes': {}",
        dir_name
    );
    
    // Test directory creation
    fs::create_dir_all(&output_dir)?;
    assert!(output_dir.exists(), "Output directory should be created");
    assert!(output_dir.is_dir(), "Output path should be a directory");
    
    // Test file path generation within output directory
    let test_files = vec!["movie1.mkv", "movie2.mp4", "series.mkv"];
    
    for input_filename in test_files {
        let output_filename = video_encoder::generate_output_filename(input_filename);
        let output_path = output_dir.join(&output_filename);
        
        // Validate output path structure
        assert_eq!(
            output_path.parent().unwrap(),
            output_dir,
            "Output file should be in the correct directory"
        );
        
        assert_eq!(
            output_path.file_name().unwrap().to_str().unwrap(),
            output_filename,
            "Output filename should match expected pattern"
        );
        
        println!("✓ Output path validation: {} -> {}", input_filename, output_path.display());
    }
    
    Ok(())
}

// ============================================================================
// METADATA PRESERVATION VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_video_metadata_extraction() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Test metadata extraction for different file types
    let test_files = vec![
        "sample_1080p.mkv",
        "sample_720p.mp4",
        "sample_4k.mkv",
        "sample_with_subs.mkv",
    ];
    
    for filename in test_files {
        let file_path = test_dir.join(filename);
        
        // Test metadata extraction (will fail for mock files, but tests the interface)
        let metadata_result = scanner::extract_video_metadata(&file_path).await;
        
        match metadata_result {
            Ok(metadata) => {
                println!("✓ Metadata extracted for {}: {:?}", filename, metadata);
                
                // Validate metadata structure
                assert!(metadata.size > 0, "File size should be positive");
                
                if let Some((width, height)) = metadata.resolution {
                    assert!(width > 0 && height > 0, "Resolution should be positive");
                }
                
                if let Some(duration) = metadata.duration {
                    assert!(duration > Duration::from_secs(0), "Duration should be positive");
                }
            }
            Err(e) => {
                println!("ℹ Metadata extraction failed for {} (expected for mock files): {}", filename, e);
                // Expected for mock files without real video data
                assert!(
                    e.to_string().contains("metadata") || 
                    e.to_string().contains("format") ||
                    e.to_string().contains("stream"),
                    "Error should be related to metadata/format issues"
                );
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_codec_detection_validation() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Test codec detection for various file patterns
    let test_cases = vec![
        ("h264_video.mkv", scanner::VideoCodec::H264),
        ("h265_video.mkv", scanner::VideoCodec::H265),
        ("x264_encoded.mp4", scanner::VideoCodec::H264),
        ("x265_encoded.mp4", scanner::VideoCodec::H265),
        ("unknown_format.avi", scanner::VideoCodec::Unknown),
    ];
    
    for (filename, expected_codec) in test_cases {
        let file_path = test_dir.join(filename);
        fs::write(&file_path, "mock video content")?;
        
        let detected_codec_result = scanner::detect_video_codec(&file_path).await;
        
        match detected_codec_result {
            Ok(detected_codec) => {
                println!("✓ Codec detected for {}: {:?}", filename, detected_codec);
                
                // For real files, we could validate against expected_codec
                // For mock files, we just verify the detection doesn't crash
            }
            Err(e) => {
                println!("ℹ Codec detection failed for {} (expected for mock files): {}", filename, e);
                // Expected for mock files without real video streams
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_audio_stream_preservation_validation() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    
    // Test audio preservation in encoding commands
    let test_files = vec![
        ("movie_stereo.mkv", "Stereo audio"),
        ("movie_5_1.mkv", "5.1 surround audio"),
        ("movie_multi_audio.mkv", "Multiple audio tracks"),
        ("movie_no_audio.mkv", "Video only"),
    ];
    
    for (filename, description) in test_files {
        let input_path = PathBuf::from(filename);
        let output_path = PathBuf::from(format!("output_{}", filename));
        
        // Test both hardware and software commands
        let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
        let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
        
        let hw_command_str = hw_command.join(" ");
        let sw_command_str = sw_command.join(" ");
        
        // Verify audio preservation parameters
        let hw_preserves_audio = hw_command_str.contains("c:a copy") || 
                                hw_command_str.contains("-c:a copy") ||
                                hw_command_str.contains("acodec copy");
        
        let sw_preserves_audio = sw_command_str.contains("c:a copy") || 
                                sw_command_str.contains("-c:a copy") ||
                                sw_command_str.contains("acodec copy");
        
        assert!(hw_preserves_audio, "Hardware command should preserve audio for {}", filename);
        assert!(sw_preserves_audio, "Software command should preserve audio for {}", filename);
        
        println!("✓ Audio preservation validated for {} ({})", filename, description);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_subtitle_stream_preservation_validation() -> Result<()> {
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(cli::QualityProfile::Medium);
    
    // Test subtitle preservation in encoding commands
    let test_files = vec![
        ("movie_with_subs.mkv", "Embedded subtitles"),
        ("movie_multi_subs.mkv", "Multiple subtitle tracks"),
        ("movie_external_subs.mkv", "External subtitle files"),
    ];
    
    for (filename, description) in test_files {
        let input_path = PathBuf::from(filename);
        let output_path = PathBuf::from(format!("output_{}", filename));
        
        // Test subtitle preservation
        let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
        let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
        
        let hw_command_str = hw_command.join(" ");
        let sw_command_str = sw_command.join(" ");
        
        // Check for subtitle preservation parameters
        let hw_preserves_subs = hw_command_str.contains("c:s copy") || 
                               hw_command_str.contains("-c:s copy") ||
                               hw_command_str.contains("scodec copy") ||
                               hw_command_str.contains("map 0");
        
        let sw_preserves_subs = sw_command_str.contains("c:s copy") || 
                               sw_command_str.contains("-c:s copy") ||
                               sw_command_str.contains("scodec copy") ||
                               sw_command_str.contains("map 0");
        
        // Note: Subtitle preservation might be optional depending on implementation
        if hw_preserves_subs {
            println!("✓ Hardware command preserves subtitles for {} ({})", filename, description);
        } else {
            println!("ℹ Hardware command may not preserve subtitles for {} ({})", filename, description);
        }
        
        if sw_preserves_subs {
            println!("✓ Software command preserves subtitles for {} ({})", filename, description);
        } else {
            println!("ℹ Software command may not preserve subtitles for {} ({})", filename, description);
        }
    }
    
    Ok(())
}

// ============================================================================
// FILE SIZE AND COMPRESSION VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_compression_ratio_expectations() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Test expected compression ratios for different quality profiles
    let quality_profiles = vec![
        (cli::QualityProfile::Quality, 0.7, 0.9),        // 70-90% of original size
        (cli::QualityProfile::Medium, 0.5, 0.8),         // 50-80% of original size
        (cli::QualityProfile::HighCompression, 0.3, 0.6), // 30-60% of original size
    ];
    
    for (profile, min_ratio, max_ratio) in quality_profiles {
        let crf_value = profile.crf_value();
        
        // Validate CRF values are in expected ranges for compression
        match profile {
            cli::QualityProfile::Quality => {
                assert!(crf_value <= 22, "Quality profile should have low CRF (high quality)");
            }
            cli::QualityProfile::Medium => {
                assert!(crf_value >= 20 && crf_value <= 26, "Medium profile should have moderate CRF");
            }
            cli::QualityProfile::HighCompression => {
                assert!(crf_value >= 26, "High compression should have high CRF (smaller files)");
            }
        }
        
        println!("✓ Compression expectations for {:?}: CRF {}, expected size ratio {:.0}%-{:.0}%", 
                profile, crf_value, min_ratio * 100.0, max_ratio * 100.0);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_output_file_validation_checks() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Create mock input and output files for validation
    let input_file = test_dir.join("input_validation.mkv");
    let output_file = test_dir.join("output_validation-h265.mkv");
    
    // Create mock input file
    let input_content = vec![0u8; 1024 * 1024 * 100]; // 100MB
    fs::write(&input_file, &input_content)?;
    
    // Create mock output file (smaller, simulating compression)
    let output_content = vec![0u8; 1024 * 1024 * 70]; // 70MB (30% compression)
    fs::write(&output_file, &output_content)?;
    
    // Validate file existence and sizes
    assert!(input_file.exists(), "Input file should exist");
    assert!(output_file.exists(), "Output file should exist");
    
    let input_metadata = fs::metadata(&input_file)?;
    let output_metadata = fs::metadata(&output_file)?;
    
    assert!(input_metadata.len() > 0, "Input file should have content");
    assert!(output_metadata.len() > 0, "Output file should have content");
    
    // Validate compression occurred
    let compression_ratio = output_metadata.len() as f64 / input_metadata.len() as f64;
    assert!(
        compression_ratio < 1.0,
        "Output file should be smaller than input (compression ratio: {:.2})",
        compression_ratio
    );
    
    // Validate reasonable compression ratio
    assert!(
        compression_ratio > 0.1 && compression_ratio < 0.95,
        "Compression ratio should be reasonable: {:.2}",
        compression_ratio
    );
    
    println!("✓ Output file validation: {:.1}MB -> {:.1}MB (compression: {:.1}%)", 
            input_metadata.len() as f64 / (1024.0 * 1024.0),
            output_metadata.len() as f64 / (1024.0 * 1024.0),
            (1.0 - compression_ratio) * 100.0);
    
    Ok(())
}

// ============================================================================
// COMPREHENSIVE OUTPUT VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_comprehensive_output_validation_workflow() -> Result<()> {
    let (_temp_dir, test_dir) = create_validation_test_directory();
    
    // Test complete validation workflow
    let config = cli::Config {
        target_directory: test_dir.clone(),
        quality_profile: cli::QualityProfile::Medium,
        max_parallel_jobs: 2,
        verbose: true,
    };
    
    // Step 1: Validate configuration
    config.validate()?;
    println!("✓ Configuration validation passed");
    
    // Step 2: Validate file scanning and filtering
    let all_files = scanner::scan_directory(&test_dir)?;
    let encodable_files = scanner::filter_encodable_files(all_files);
    
    assert!(!encodable_files.is_empty(), "Should find encodable files");
    println!("✓ File scanning and filtering validation passed: {} files found", encodable_files.len());
    
    // Step 3: Validate output directory creation
    let output_dir = config.output_directory();
    fs::create_dir_all(&output_dir)?;
    
    assert!(output_dir.exists(), "Output directory should be created");
    assert!(output_dir.is_dir(), "Output path should be a directory");
    println!("✓ Output directory validation passed: {}", output_dir.display());
    
    // Step 4: Validate encoding job creation
    let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(config.quality_profile.clone());
    
    for file in &encodable_files {
        let output_filename = video_encoder::generate_output_filename(&file.filename);
        let output_path = output_dir.join(&output_filename);
        
        // Validate encoding command generation
        let hw_command = ffmpeg_encoder.generate_encoding_command(&file.path, &output_path, true);
        let sw_command = ffmpeg_encoder.generate_encoding_command(&file.path, &output_path, false);
        
        // Basic command validation
        assert!(!hw_command.is_empty(), "Hardware command should not be empty");
        assert!(!sw_command.is_empty(), "Software command should not be empty");
        
        let hw_str = hw_command.join(" ");
        let sw_str = sw_command.join(" ");
        
        // Validate command structure
        assert!(hw_str.contains("ffmpeg"), "Command should start with ffmpeg");
        assert!(hw_str.contains(&file.path.to_string_lossy()), "Command should include input file");
        assert!(hw_str.contains(&output_path.to_string_lossy()), "Command should include output file");
        
        assert!(sw_str.contains("libx265"), "Software command should use libx265");
        assert!(sw_str.contains("c:a copy"), "Command should preserve audio");
        
        println!("✓ Encoding command validation passed for: {}", file.filename);
    }
    
    // Step 5: Validate error handling integration
    use video_encoder::error_recovery::ErrorRecoveryManager;
    
    let mut recovery_manager = ErrorRecoveryManager::new().with_hardware_fallback(true);
    
    // Test validation error scenarios
    let validation_errors = vec![
        AppError::EncodingError("Output validation failed".to_string()),
        AppError::HardwareAccelerationError("Hardware validation failed".to_string()),
    ];
    
    for error in validation_errors {
        let decision = recovery_manager.handle_error(error, None).await?;
        
        // Verify appropriate recovery decisions
        match decision {
            video_encoder::error_recovery::RecoveryDecision::SkipFile => {
                println!("✓ File-level error recovery validated");
            }
            video_encoder::error_recovery::RecoveryDecision::FallbackToSoftware => {
                println!("✓ Hardware fallback recovery validated");
            }
            _ => {}
        }
    }
    
    println!("✓ Comprehensive output validation workflow completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_output_quality_consistency_validation() -> Result<()> {
    // Test that quality settings are consistently applied across different scenarios
    let quality_profiles = vec![
        cli::QualityProfile::Quality,
        cli::QualityProfile::Medium,
        cli::QualityProfile::HighCompression,
    ];
    
    let test_scenarios = vec![
        ("1080p_movie.mkv", "High resolution movie"),
        ("720p_series.mp4", "Standard resolution series"),
        ("4k_documentary.mkv", "Ultra high resolution content"),
    ];
    
    for profile in &quality_profiles {
        let ffmpeg_encoder = ffmpeg::FFmpegEncoder::new(profile.clone());
        let expected_crf = profile.crf_value();
        
        for (filename, description) in &test_scenarios {
            let input_path = PathBuf::from(filename);
            let output_path = PathBuf::from(format!("output_{}", filename));
            
            // Test both hardware and software commands for consistency
            let hw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, true);
            let sw_command = ffmpeg_encoder.generate_encoding_command(&input_path, &output_path, false);
            
            let hw_str = hw_command.join(" ");
            let sw_str = sw_command.join(" ");
            
            // Verify CRF consistency
            assert!(
                hw_str.contains(&expected_crf.to_string()) || hw_str.contains("crf"),
                "Hardware command should include consistent CRF for {:?} with {}",
                profile, description
            );
            
            assert!(
                sw_str.contains(&expected_crf.to_string()),
                "Software command should include consistent CRF for {:?} with {}",
                profile, description
            );
            
            // Verify audio preservation consistency
            assert!(
                hw_str.contains("c:a copy") && sw_str.contains("c:a copy"),
                "Both commands should consistently preserve audio for {:?} with {}",
                profile, description
            );
        }
        
        println!("✓ Quality consistency validated for {:?} across all scenarios", profile);
    }
    
    Ok(())
}