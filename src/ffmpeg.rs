use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use crate::{AppError, Result};
use crate::cli::QualityProfile;

#[derive(Debug)]
pub struct EncodingConfig {
    pub quality_profile: QualityProfile,
    pub use_hardware_acceleration: bool,
    pub preserve_audio: bool,
    pub preserve_subtitles: bool,
    pub output_suffix: String,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            quality_profile: QualityProfile::Medium,
            use_hardware_acceleration: true,
            preserve_audio: true,
            preserve_subtitles: true,
            output_suffix: "-h265".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub progress_percent: f32,
    pub current_fps: Option<f32>,
    pub estimated_time_remaining: Option<std::time::Duration>,
}

pub struct FFmpegEncoder {
    config: EncodingConfig,
}

impl FFmpegEncoder {
    pub fn new(quality_profile: QualityProfile) -> Self {
        let config = EncodingConfig {
            quality_profile,
            ..Default::default()
        };
        
        Self { config }
    }
    
    /// Check if AMD AMF hardware acceleration is available
    pub async fn check_amd_hardware_support(&self) -> bool {
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .await;
            
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("h264_amf") || stdout.contains("hevc_amf")
            }
            Err(_) => false,
        }
    }
    
    /// Generate FFmpeg command arguments for AMD hardware encoding
    fn generate_amd_command_args(&self, input: &Path, output: &Path) -> Vec<String> {
        let mut args = vec![
            "-hide_banner".to_string(),
            "-y".to_string(), // Overwrite output files
            "-i".to_string(),
            input.to_string_lossy().to_string(),
        ];
        
        // Video encoding settings with AMD AMF H.265
        args.extend([
            "-c:v".to_string(),
            "hevc_amf".to_string(), // AMD AMF HEVC encoder
            "-quality".to_string(),
            "quality".to_string(), // Use quality mode instead of CQP
            "-rc".to_string(),
            "cqp".to_string(), // Constant Quality Parameter mode
            "-qp_i".to_string(),
            self.get_crf_value().to_string(),
            "-qp_p".to_string(),
            self.get_crf_value().to_string(),
            "-qp_b".to_string(),
            self.get_crf_value().to_string(),
        ]);
        
        // Audio and subtitle preservation
        if self.config.preserve_audio {
            args.extend(["-c:a".to_string(), "copy".to_string()]);
        }
        
        if self.config.preserve_subtitles {
            args.extend(["-c:s".to_string(), "copy".to_string()]);
        }
        
        // Progress reporting
        args.extend([
            "-progress".to_string(),
            "pipe:1".to_string(),
        ]);
        
        args.push(output.to_string_lossy().to_string());
        args
    }
    
    /// Generate FFmpeg command arguments for software encoding fallback
    fn generate_software_command_args(&self, input: &Path, output: &Path) -> Vec<String> {
        let mut args = vec![
            "-hide_banner".to_string(),
            "-y".to_string(), // Overwrite output files
            "-i".to_string(),
            input.to_string_lossy().to_string(),
        ];
        
        // Video encoding settings
        args.extend([
            "-c:v".to_string(),
            "libx265".to_string(),
            "-crf".to_string(),
            self.get_crf_value().to_string(),
            "-preset".to_string(),
            "medium".to_string(),
        ]);
        
        // Audio and subtitle preservation
        if self.config.preserve_audio {
            args.extend(["-c:a".to_string(), "copy".to_string()]);
        }
        
        if self.config.preserve_subtitles {
            args.extend(["-c:s".to_string(), "copy".to_string()]);
        }
        
        // Progress reporting
        args.extend([
            "-progress".to_string(),
            "pipe:1".to_string(),
        ]);
        
        args.push(output.to_string_lossy().to_string());
        args
    }
    
    /// Get video duration in seconds for progress calculation
    async fn get_video_duration(&self, input: &Path) -> Result<f64> {
        let output = Command::new("ffprobe")
            .args([
                "-v", "quiet",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                &input.to_string_lossy()
            ])
            .output()
            .await
            .map_err(|e| AppError::EncodingError(format!("Failed to probe video duration: {}", e)))?;
            
        let duration_str = String::from_utf8_lossy(&output.stdout);
        duration_str.trim().parse::<f64>()
            .map_err(|e| AppError::EncodingError(format!("Failed to parse video duration: {}", e)))
    }
    
    /// Parse FFmpeg progress output and calculate percentage
    fn parse_progress_line(&self, line: &str, total_duration: f64) -> Option<ProgressUpdate> {
        if line.starts_with("out_time_ms=") {
            if let Ok(time_ms) = line[12..].parse::<u64>() {
                let current_time = time_ms as f64 / 1_000_000.0; // Convert microseconds to seconds
                let progress_percent = ((current_time / total_duration) * 100.0).min(100.0) as f32;
                
                return Some(ProgressUpdate {
                    progress_percent,
                    current_fps: None,
                    estimated_time_remaining: None,
                });
            }
        }
        None
    }
    
    /// Encode video with progress callback and comprehensive error handling
    pub async fn encode_video<F>(
        &self,
        input: &Path,
        output: &Path,
        progress_callback: F,
    ) -> Result<()>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        self.encode_video_with_recovery(input, output, progress_callback, true).await
    }
    
    /// Encode video with optional hardware acceleration fallback
    pub async fn encode_video_with_recovery<F>(
        &self,
        input: &Path,
        output: &Path,
        progress_callback: F,
        allow_fallback: bool,
    ) -> Result<()>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        // Validate input file exists and is readable
        if !input.exists() {
            return Err(AppError::file_error(
                input.to_path_buf(),
                "Input file does not exist"
            ));
        }
        
        if !input.is_file() {
            return Err(AppError::file_error(
                input.to_path_buf(),
                "Input path is not a file"
            ));
        }
        
        // Get video duration for progress calculation
        let total_duration = match self.get_video_duration(input).await {
            Ok(duration) => duration,
            Err(e) => {
                return Err(AppError::file_error(
                    input.to_path_buf(),
                    format!("Failed to get video duration: {}", e)
                ));
            }
        };
        
        // Check for AMD hardware support
        let hardware_available = self.check_amd_hardware_support().await;
        let _use_hardware = self.config.use_hardware_acceleration && hardware_available;
        
        // First attempt: try hardware acceleration if requested and available
        if self.config.use_hardware_acceleration && hardware_available {
            println!("Using AMD AMF hardware acceleration for H.265 encoding...");
            match self.encode_with_hardware(input, output, &progress_callback, total_duration).await {
                Ok(()) => {
                    println!("Hardware encoding completed successfully");
                    return Ok(());
                },
                Err(e) => {
                    if allow_fallback {
                        eprintln!("Hardware encoding failed: {}. Falling back to software encoding.", e);
                        // Continue to software fallback
                    } else {
                        return Err(AppError::hardware_error(format!(
                            "Hardware encoding failed and fallback disabled: {}", e
                        )));
                    }
                }
            }
        } else if self.config.use_hardware_acceleration && !hardware_available {
            if allow_fallback {
                eprintln!("AMD hardware acceleration not available, using software encoding");
            } else {
                return Err(AppError::hardware_error(
                    "Hardware acceleration requested but not available, and fallback disabled"
                ));
            }
        }
        
        // Software encoding fallback
        println!("Using software encoding (libx265)...");
        let result = self.encode_with_software(input, output, &progress_callback, total_duration).await;
        if result.is_ok() {
            println!("Software encoding completed successfully");
        }
        result
    }
    
    /// Encode using AMD hardware acceleration
    async fn encode_with_hardware<F>(
        &self,
        input: &Path,
        output: &Path,
        progress_callback: &F,
        total_duration: f64,
    ) -> Result<()>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        let args = self.generate_amd_command_args(input, output);
        
        self.execute_ffmpeg_command(args, progress_callback, total_duration, "hardware").await
            .map_err(|e| AppError::hardware_error(format!("AMD hardware encoding failed: {}", e)))
    }
    
    /// Encode using software (libx265)
    async fn encode_with_software<F>(
        &self,
        input: &Path,
        output: &Path,
        progress_callback: &F,
        total_duration: f64,
    ) -> Result<()>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        let args = self.generate_software_command_args(input, output);
        
        self.execute_ffmpeg_command(args, progress_callback, total_duration, "software").await
            .map_err(|e| AppError::ffmpeg_error(format!("Software encoding failed: {}", e)))
    }
    
    /// Execute FFmpeg command with comprehensive error handling
    async fn execute_ffmpeg_command<F>(
        &self,
        args: Vec<String>,
        progress_callback: &F,
        total_duration: f64,
        encoding_type: &str,
    ) -> Result<()>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        // Spawn FFmpeg process
        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::ffmpeg_error(format!("Failed to spawn FFmpeg process: {}", e)))?;
        
        // Handle progress updates and error capture
        let progress_callback = Arc::new(progress_callback);
        let mut stderr_output = Vec::new();
        
        // Read stdout for progress
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(progress) = self.parse_progress_line(&line, total_duration) {
                    progress_callback(progress);
                }
            }
        }
        
        // Read stderr for error information
        if let Some(stderr) = child.stderr.take() {
            let mut stderr_reader = BufReader::new(stderr);
            let mut stderr_line = String::new();
            while stderr_reader.read_line(&mut stderr_line).await.unwrap_or(0) > 0 {
                stderr_output.push(stderr_line.clone());
                stderr_line.clear();
            }
        }
        
        // Wait for process completion
        let status = child.wait().await
            .map_err(|e| AppError::ffmpeg_error(format!("FFmpeg process failed: {}", e)))?;
        
        if !status.success() {
            let error_details = if !stderr_output.is_empty() {
                let stderr_text = stderr_output.join("\n").trim().to_string();
                eprintln!("FFmpeg {} encoding failed. Error output:", encoding_type);
                eprintln!("{}", stderr_text);
                format!("FFmpeg {} encoding failed with exit code {:?}. Error output: {}",
                    encoding_type,
                    status.code(),
                    stderr_text
                )
            } else {
                format!("FFmpeg {} encoding failed with exit code {:?}",
                    encoding_type,
                    status.code()
                )
            };
            
            return Err(AppError::ffmpeg_error(error_details));
        }
        
        // Final progress update
        progress_callback(ProgressUpdate {
            progress_percent: 100.0,
            current_fps: None,
            estimated_time_remaining: None,
        });
        
        Ok(())
    }
    
    pub fn get_crf_value(&self) -> u32 {
        match self.config.quality_profile {
            QualityProfile::Quality => 20,
            QualityProfile::Medium => 23,
            QualityProfile::HighCompression => 28,
        }
    }
    
    /// Check if AMD AMF hardware acceleration is available (alias for compatibility)
    pub async fn check_amd_hardware_availability(&self) -> Result<bool> {
        Ok(self.check_amd_hardware_support().await)
    }
    
    /// Generate encoding command arguments (public interface for tests)
    pub fn generate_encoding_command(&self, input: &Path, output: &Path, use_hardware: bool) -> Vec<String> {
        if use_hardware {
            self.generate_amd_command_args(input, output)
        } else {
            self.generate_software_command_args(input, output)
        }
    }
}
#
[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    fn create_test_encoder(quality: QualityProfile) -> FFmpegEncoder {
        FFmpegEncoder::new(quality)
    }
    
    #[test]
    fn test_encoder_creation() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        assert_eq!(encoder.config.quality_profile, QualityProfile::Medium);
        assert!(encoder.config.use_hardware_acceleration);
        assert!(encoder.config.preserve_audio);
        assert!(encoder.config.preserve_subtitles);
        assert_eq!(encoder.config.output_suffix, "-h265");
    }
    
    #[test]
    fn test_crf_value_mapping() {
        let quality_encoder = create_test_encoder(QualityProfile::Quality);
        let medium_encoder = create_test_encoder(QualityProfile::Medium);
        let high_compression_encoder = create_test_encoder(QualityProfile::HighCompression);
        
        assert_eq!(quality_encoder.get_crf_value(), 20);
        assert_eq!(medium_encoder.get_crf_value(), 23);
        assert_eq!(high_compression_encoder.get_crf_value(), 28);
    }
    
    #[test]
    fn test_amd_command_generation() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        let input = PathBuf::from("/test/input.mkv");
        let output = PathBuf::from("/test/output.mkv");
        
        let args = encoder.generate_amd_command_args(&input, &output);
        
        // Check essential AMD hardware acceleration arguments
        assert!(args.contains(&"-hwaccel".to_string()));
        assert!(args.contains(&"amf".to_string()));
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"hevc_amf".to_string()));
        assert!(args.contains(&"-rc".to_string()));
        assert!(args.contains(&"cqp".to_string()));
        
        // Check quality parameter
        assert!(args.contains(&"23".to_string())); // Medium CRF value
        
        // Check audio/subtitle preservation
        assert!(args.contains(&"-c:a".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"-c:s".to_string()));
        
        // Check progress reporting
        assert!(args.contains(&"-progress".to_string()));
        assert!(args.contains(&"pipe:1".to_string()));
        
        // Check input/output files
        assert!(args.contains(&input.to_string_lossy().to_string()));
        assert!(args.contains(&output.to_string_lossy().to_string()));
    }
    
    #[test]
    fn test_software_command_generation() {
        let encoder = create_test_encoder(QualityProfile::Quality);
        let input = PathBuf::from("/test/input.mp4");
        let output = PathBuf::from("/test/output.mp4");
        
        let args = encoder.generate_software_command_args(&input, &output);
        
        // Check software encoding arguments
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"20".to_string())); // Quality CRF value
        assert!(args.contains(&"-preset".to_string()));
        assert!(args.contains(&"medium".to_string()));
        
        // Check audio/subtitle preservation
        assert!(args.contains(&"-c:a".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"-c:s".to_string()));
        
        // Check progress reporting
        assert!(args.contains(&"-progress".to_string()));
        assert!(args.contains(&"pipe:1".to_string()));
        
        // Check input/output files
        assert!(args.contains(&input.to_string_lossy().to_string()));
        assert!(args.contains(&output.to_string_lossy().to_string()));
    }
    
    #[test]
    fn test_quality_profile_parameter_mapping() {
        let quality_encoder = create_test_encoder(QualityProfile::Quality);
        let medium_encoder = create_test_encoder(QualityProfile::Medium);
        let high_compression_encoder = create_test_encoder(QualityProfile::HighCompression);
        
        let input = PathBuf::from("/test/input.mkv");
        let output = PathBuf::from("/test/output.mkv");
        
        let quality_args = quality_encoder.generate_amd_command_args(&input, &output);
        let medium_args = medium_encoder.generate_amd_command_args(&input, &output);
        let high_compression_args = high_compression_encoder.generate_amd_command_args(&input, &output);
        
        // Check that different quality profiles generate different CRF values
        assert!(quality_args.contains(&"20".to_string()));
        assert!(medium_args.contains(&"23".to_string()));
        assert!(high_compression_args.contains(&"28".to_string()));
    }
    
    #[test]
    fn test_progress_parsing() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        let total_duration = 100.0; // 100 seconds
        
        // Test valid progress line
        let progress_line = "out_time_ms=25000000"; // 25 seconds in microseconds
        let progress = encoder.parse_progress_line(progress_line, total_duration);
        
        assert!(progress.is_some());
        let progress = progress.unwrap();
        assert_eq!(progress.progress_percent, 25.0);
        
        // Test invalid progress line
        let invalid_line = "frame=123";
        let progress = encoder.parse_progress_line(invalid_line, total_duration);
        assert!(progress.is_none());
        
        // Test progress at completion
        let completion_line = "out_time_ms=100000000"; // 100 seconds
        let progress = encoder.parse_progress_line(completion_line, total_duration);
        assert!(progress.is_some());
        let progress = progress.unwrap();
        assert_eq!(progress.progress_percent, 100.0);
        
        // Test progress beyond completion (should cap at 100%)
        let beyond_line = "out_time_ms=150000000"; // 150 seconds
        let progress = encoder.parse_progress_line(beyond_line, total_duration);
        assert!(progress.is_some());
        let progress = progress.unwrap();
        assert_eq!(progress.progress_percent, 100.0);
    }
    
    #[test]
    fn test_encoding_config_default() {
        let config = EncodingConfig::default();
        
        assert_eq!(config.quality_profile, QualityProfile::Medium);
        assert!(config.use_hardware_acceleration);
        assert!(config.preserve_audio);
        assert!(config.preserve_subtitles);
        assert_eq!(config.output_suffix, "-h265");
    }
    
    #[test]
    fn test_progress_update_creation() {
        let progress = ProgressUpdate {
            progress_percent: 50.0,
            current_fps: Some(30.0),
            estimated_time_remaining: Some(std::time::Duration::from_secs(60)),
        };
        
        assert_eq!(progress.progress_percent, 50.0);
        assert_eq!(progress.current_fps, Some(30.0));
        assert_eq!(progress.estimated_time_remaining, Some(std::time::Duration::from_secs(60)));
    }
    
    #[test]
    fn test_command_args_overwrite_flag() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        let input = PathBuf::from("/test/input.mkv");
        let output = PathBuf::from("/test/output.mkv");
        
        let amd_args = encoder.generate_amd_command_args(&input, &output);
        let software_args = encoder.generate_software_command_args(&input, &output);
        
        // Both command types should include the overwrite flag
        assert!(amd_args.contains(&"-y".to_string()));
        assert!(software_args.contains(&"-y".to_string()));
    }
    
    #[test]
    fn test_command_args_hide_banner() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        let input = PathBuf::from("/test/input.mkv");
        let output = PathBuf::from("/test/output.mkv");
        
        let amd_args = encoder.generate_amd_command_args(&input, &output);
        let software_args = encoder.generate_software_command_args(&input, &output);
        
        // Both command types should hide the banner
        assert!(amd_args.contains(&"-hide_banner".to_string()));
        assert!(software_args.contains(&"-hide_banner".to_string()));
    }
    
    #[tokio::test]
    async fn test_amd_hardware_support_check() {
        let encoder = create_test_encoder(QualityProfile::Medium);
        
        // This test will check if the method runs without panicking
        // The actual result depends on the system's FFmpeg installation
        let _result = encoder.check_amd_hardware_support().await;
        
        // We can't assert the specific result since it depends on the system,
        // but we can ensure the method completes successfully
        assert!(true);
    }
}