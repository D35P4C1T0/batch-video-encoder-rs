pub mod cli;
pub mod scanner;
pub mod tui;
pub mod encoder;
pub mod ffmpeg;

use std::process;
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

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Parse CLI arguments
    let config = cli::Config::parse_args()?;
    
    if config.verbose {
        println!("Video Encoder - AMD Hardware Acceleration");
        println!("Target directory: {:?}", config.target_directory);
        println!("Quality profile: {:?} ({})", config.quality_profile, config.quality_profile.description());
        println!("Max parallel jobs: {}", config.max_parallel_jobs);
        println!("Output directory: {:?}", config.output_directory());
    }
    
    // Scan for video files
    println!("Scanning directory: {:?}", config.target_directory);
    let all_files = scanner::scan_directory(&config.target_directory)?;
    let encodable_files = scanner::filter_encodable_files(all_files);
    
    if encodable_files.is_empty() {
        println!("No H.264 files found for encoding.");
        return Ok(());
    }
    
    println!("Found {} files that can be encoded to H.265", encodable_files.len());
    
    // Launch TUI for file selection
    let mut tui_manager = tui::TuiManager::new(encodable_files)?;
    let selected_files = tui_manager.run_file_selection()?;
    
    if selected_files.is_empty() {
        println!("No files selected for encoding. Exiting.");
    } else {
        println!("Selected {} files for encoding:", selected_files.len());
        for file in &selected_files {
            println!("  - {}", file.filename);
        }
        println!("Encoding functionality will be implemented in subsequent tasks.");
    }
    
    Ok(())
}