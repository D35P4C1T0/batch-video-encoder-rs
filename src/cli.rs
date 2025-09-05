use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use crate::{AppError, Result};

/// Quality profiles for video encoding with corresponding CRF values
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum QualityProfile {
    /// High quality encoding (CRF 18-20)
    Quality,
    /// Balanced quality and size (CRF 23-25)
    Medium,
    /// Maximum compression (CRF 28-30)
    HighCompression,
}

impl QualityProfile {
    /// Get the CRF value range for this quality profile
    pub fn crf_value(&self) -> u8 {
        match self {
            QualityProfile::Quality => 20,
            QualityProfile::Medium => 23,
            QualityProfile::HighCompression => 28,
        }
    }
    
    /// Get a human-readable description of the quality profile
    pub fn description(&self) -> &'static str {
        match self {
            QualityProfile::Quality => "High quality (larger file size)",
            QualityProfile::Medium => "Balanced quality and size",
            QualityProfile::HighCompression => "Maximum compression (smaller file size)",
        }
    }
}

/// Configuration for the video encoding application
#[derive(Debug, Parser)]
#[command(name = "video-encoder")]
#[command(about = "A video encoding tool with AMD hardware acceleration")]
#[command(version = "0.1.0")]
#[command(author = "Video Encoder Team")]
pub struct Config {
    /// Target directory containing video files to encode
    #[arg(help = "Directory path containing .mkv and .mp4 files")]
    pub target_directory: PathBuf,
    
    /// Quality profile for encoding
    #[arg(
        short = 'q',
        long = "quality",
        value_enum,
        default_value = "medium",
        help = "Encoding quality profile"
    )]
    pub quality_profile: QualityProfile,
    
    /// Maximum number of parallel encoding jobs
    #[arg(
        short = 'j',
        long = "jobs",
        default_value = "2",
        value_parser = validate_parallel_jobs,
        help = "Number of parallel encoding jobs (1-8)"
    )]
    pub max_parallel_jobs: usize,
    
    /// Enable verbose output
    #[arg(short = 'v', long = "verbose", help = "Enable verbose logging")]
    pub verbose: bool,
}

impl Config {
    /// Parse command line arguments and validate configuration
    pub fn parse_args() -> Result<Self> {
        let config = Config::parse();
        config.validate()?;
        Ok(config)
    }
    
    /// Validate the configuration parameters
    pub fn validate(&self) -> Result<()> {
        // Validate target directory exists
        if !self.target_directory.exists() {
            return Err(AppError::ConfigError(
                format!("Target directory does not exist: {:?}", self.target_directory)
            ));
        }
        
        // Validate target is a directory
        if !self.target_directory.is_dir() {
            return Err(AppError::ConfigError(
                format!("Target path is not a directory: {:?}", self.target_directory)
            ));
        }
        
        // Validate directory is readable
        match std::fs::read_dir(&self.target_directory) {
            Ok(_) => {},
            Err(e) => {
                return Err(AppError::ConfigError(
                    format!("Cannot read target directory {:?}: {}", self.target_directory, e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Get the output directory path based on the target directory
    pub fn output_directory(&self) -> PathBuf {
        let dir_name = self.target_directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("output");
        
        self.target_directory
            .parent()
            .unwrap_or(&self.target_directory)
            .join(format!("{}-encodes", dir_name))
    }
}

/// Validate parallel jobs parameter
fn validate_parallel_jobs(s: &str) -> std::result::Result<usize, String> {
    let jobs: usize = s.parse()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    
    if jobs == 0 {
        return Err("Number of parallel jobs must be at least 1".to_string());
    }
    
    if jobs > 8 {
        return Err("Number of parallel jobs cannot exceed 8".to_string());
    }
    
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    fn create_test_directory() -> TempDir {
        TempDir::new().expect("Failed to create temporary directory")
    }
    
    #[test]
    fn test_quality_profile_crf_values() {
        assert_eq!(QualityProfile::Quality.crf_value(), 20);
        assert_eq!(QualityProfile::Medium.crf_value(), 23);
        assert_eq!(QualityProfile::HighCompression.crf_value(), 28);
    }
    
    #[test]
    fn test_quality_profile_descriptions() {
        assert_eq!(QualityProfile::Quality.description(), "High quality (larger file size)");
        assert_eq!(QualityProfile::Medium.description(), "Balanced quality and size");
        assert_eq!(QualityProfile::HighCompression.description(), "Maximum compression (smaller file size)");
    }
    
    #[test]
    fn test_validate_parallel_jobs_valid() {
        assert_eq!(validate_parallel_jobs("1").unwrap(), 1);
        assert_eq!(validate_parallel_jobs("2").unwrap(), 2);
        assert_eq!(validate_parallel_jobs("4").unwrap(), 4);
        assert_eq!(validate_parallel_jobs("8").unwrap(), 8);
    }
    
    #[test]
    fn test_validate_parallel_jobs_invalid() {
        assert!(validate_parallel_jobs("0").is_err());
        assert!(validate_parallel_jobs("9").is_err());
        assert!(validate_parallel_jobs("abc").is_err());
        assert!(validate_parallel_jobs("-1").is_err());
    }
    
    #[test]
    fn test_config_validation_valid_directory() {
        let temp_dir = create_test_directory();
        let config = Config {
            target_directory: temp_dir.path().to_path_buf(),
            quality_profile: QualityProfile::Medium,
            max_parallel_jobs: 2,
            verbose: false,
        };
        
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_validation_nonexistent_directory() {
        let config = Config {
            target_directory: PathBuf::from("/nonexistent/directory"),
            quality_profile: QualityProfile::Medium,
            max_parallel_jobs: 2,
            verbose: false,
        };
        
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_config_validation_file_not_directory() {
        let temp_dir = create_test_directory();
        let file_path = temp_dir.path().join("test_file.txt");
        fs::write(&file_path, "test content").expect("Failed to create test file");
        
        let config = Config {
            target_directory: file_path,
            quality_profile: QualityProfile::Medium,
            max_parallel_jobs: 2,
            verbose: false,
        };
        
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_output_directory_generation() {
        let temp_dir = create_test_directory();
        let target_path = temp_dir.path().join("videos");
        fs::create_dir(&target_path).expect("Failed to create target directory");
        
        let config = Config {
            target_directory: target_path.clone(),
            quality_profile: QualityProfile::Medium,
            max_parallel_jobs: 2,
            verbose: false,
        };
        
        let expected_output = temp_dir.path().join("videos-encodes");
        assert_eq!(config.output_directory(), expected_output);
    }
    
    #[test]
    fn test_output_directory_root_path() {
        let config = Config {
            target_directory: PathBuf::from("/"),
            quality_profile: QualityProfile::Medium,
            max_parallel_jobs: 2,
            verbose: false,
        };
        
        // Should handle root directory gracefully
        let output_dir = config.output_directory();
        assert!(output_dir.to_string_lossy().contains("-encodes"));
    }
    
    #[test]
    fn test_quality_profile_equality() {
        assert_eq!(QualityProfile::Quality, QualityProfile::Quality);
        assert_eq!(QualityProfile::Medium, QualityProfile::Medium);
        assert_eq!(QualityProfile::HighCompression, QualityProfile::HighCompression);
        
        assert_ne!(QualityProfile::Quality, QualityProfile::Medium);
        assert_ne!(QualityProfile::Medium, QualityProfile::HighCompression);
    }
    
    #[test]
    fn test_config_debug_format() {
        let temp_dir = create_test_directory();
        let config = Config {
            target_directory: temp_dir.path().to_path_buf(),
            quality_profile: QualityProfile::Quality,
            max_parallel_jobs: 4,
            verbose: true,
        };
        
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Quality"));
        assert!(debug_str.contains("max_parallel_jobs: 4"));
        assert!(debug_str.contains("verbose: true"));
    }
}