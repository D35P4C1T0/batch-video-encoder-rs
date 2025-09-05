# Design Document

## Overview

The video encoding script will be implemented in Rust to achieve optimal performance and resource efficiency. The application follows a modular architecture with clear separation between file discovery, encoding management, progress tracking, and user interface components. The design leverages AMD AMF hardware acceleration through FFmpeg and provides a rich terminal user interface for interactive file selection and progress monitoring.

## Architecture

The application uses an event-driven architecture with the following main components:

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   CLI Parser    │───▶│  File Scanner   │───▶│   TUI Manager   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ Progress Tracker│◀───│ Encoding Manager│◀───│ User Selections │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐    ┌─────────────────┐
│   TUI Display   │    │  FFmpeg Worker  │
└─────────────────┘    └─────────────────┘
```

## Components and Interfaces

### 1. CLI Parser (`cli.rs`)
- **Purpose**: Handle command-line arguments and quality profile selection
- **Interface**: 
  ```rust
  pub struct Config {
      pub target_directory: PathBuf,
      pub quality_profile: QualityProfile,
      pub max_parallel_jobs: usize,
  }
  
  pub enum QualityProfile {
      Quality,    // CRF 18-20
      Medium,     // CRF 23-25  
      HighCompression, // CRF 28-30
  }
  ```

### 2. File Scanner (`scanner.rs`)
- **Purpose**: Discover and filter video files in the target directory
- **Interface**:
  ```rust
  pub struct VideoFile {
      pub path: PathBuf,
      pub codec: VideoCodec,
      pub size: u64,
      pub duration: Option<Duration>,
  }
  
  pub enum VideoCodec {
      H264,
      H265,
      Unknown,
  }
  
  pub fn scan_directory(path: &Path) -> Result<Vec<VideoFile>, ScanError>;
  pub fn filter_encodable_files(files: Vec<VideoFile>) -> Vec<VideoFile>;
  ```

### 3. TUI Manager (`tui.rs`)
- **Purpose**: Provide interactive file selection and progress display
- **Interface**:
  ```rust
  pub struct TuiManager {
      terminal: Terminal<CrosstermBackend<Stdout>>,
      app_state: AppState,
  }
  
  pub struct AppState {
      pub files: Vec<VideoFile>,
      pub selected_files: HashSet<usize>,
      pub encoding_progress: HashMap<PathBuf, f32>,
      pub overall_progress: f32,
  }
  ```

### 4. Encoding Manager (`encoder.rs`)
- **Purpose**: Coordinate parallel encoding operations
- **Interface**:
  ```rust
  pub struct EncodingManager {
      max_parallel: usize,
      active_jobs: HashMap<PathBuf, JoinHandle<EncodingResult>>,
      progress_tx: mpsc::Sender<ProgressUpdate>,
  }
  
  pub struct EncodingJob {
      pub input_path: PathBuf,
      pub output_path: PathBuf,
      pub quality_profile: QualityProfile,
  }
  ```

### 5. FFmpeg Worker (`ffmpeg.rs`)
- **Purpose**: Execute video encoding with AMD hardware acceleration
- **Interface**:
  ```rust
  pub struct FFmpegEncoder {
      quality_profile: QualityProfile,
  }
  
  pub async fn encode_video(
      &self,
      input: &Path,
      output: &Path,
      progress_callback: impl Fn(f32),
  ) -> Result<(), EncodingError>;
  ```

## Data Models

### Video File Metadata
```rust
#[derive(Debug, Clone)]
pub struct VideoFile {
    pub path: PathBuf,
    pub filename: String,
    pub size: u64,
    pub codec: VideoCodec,
    pub resolution: Option<(u32, u32)>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u32>,
}
```

### Encoding Configuration
```rust
#[derive(Debug, Clone)]
pub struct EncodingConfig {
    pub quality_profile: QualityProfile,
    pub use_hardware_acceleration: bool,
    pub preserve_audio: bool,
    pub preserve_subtitles: bool,
    pub output_suffix: String, // "-h265"
}
```

### Progress Tracking
```rust
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub file_path: PathBuf,
    pub progress_percent: f32,
    pub estimated_time_remaining: Option<Duration>,
    pub current_fps: Option<f32>,
}
```

## Error Handling

The application implements comprehensive error handling with custom error types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("File scanning error: {0}")]
    ScanError(#[from] ScanError),
    
    #[error("Encoding error: {0}")]
    EncodingError(#[from] EncodingError),
    
    #[error("TUI error: {0}")]
    TuiError(#[from] TuiError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

Error recovery strategies:
- **Hardware acceleration failure**: Automatic fallback to software encoding
- **File access errors**: Skip problematic files and continue with batch
- **Encoding failures**: Log error and continue with remaining files
- **TUI crashes**: Graceful cleanup and error reporting

## Testing Strategy

### Unit Tests
- **File scanner**: Test codec detection, filtering logic, and edge cases
- **FFmpeg wrapper**: Mock FFmpeg calls and test parameter generation
- **Progress tracking**: Verify progress calculation and update mechanisms
- **Configuration parsing**: Test quality profile mapping and validation

### Integration Tests
- **End-to-end encoding**: Test complete encoding workflow with sample files
- **Hardware acceleration**: Verify AMD AMF detection and fallback behavior
- **Parallel processing**: Test concurrent encoding with various job counts
- **Directory management**: Verify output directory creation and file naming

### Performance Tests
- **Memory usage**: Monitor memory consumption during large batch operations
- **CPU utilization**: Verify efficient resource usage with hardware acceleration
- **Encoding speed**: Benchmark against reference implementations

## Implementation Details

### AMD Hardware Acceleration
The application will use FFmpeg with AMF support:
```bash
ffmpeg -hwaccel amf -i input.mkv -c:v h264_amf -c:a copy output-h265.mkv
```

Fallback command for software encoding:
```bash
ffmpeg -i input.mkv -c:v libx265 -crf 23 -c:a copy output-h265.mkv
```

### Quality Profile Mapping
- **Quality**: CRF 18-20, higher bitrate preservation
- **Medium**: CRF 23-25, balanced size/quality
- **High Compression**: CRF 28-30, maximum compression

### TUI Layout
```
┌─────────────────────────────────────────────────────────────┐
│ Video Encoder - AMD Hardware Acceleration                   │
├─────────────────────────────────────────────────────────────┤
│ Quality Profile: Medium | Parallel Jobs: 2                 │
├─────────────────────────────────────────────────────────────┤
│ Files to Encode:                                            │
│ [✓] movie1.mkv (H.264, 2.1GB) ████████████░░░░ 75%        │
│ [ ] movie2.mp4 (H.264, 1.8GB) ░░░░░░░░░░░░░░░░  0%        │
│ [✓] movie3.mkv (H.264, 3.2GB) ██████████████░░ 90%        │
├─────────────────────────────────────────────────────────────┤
│ Overall Progress: ████████░░░░░░░░░░ 55% (2/3 files)       │
│ Active Jobs: 2 | Completed: 1 | Remaining: 1               │
└─────────────────────────────────────────────────────────────┘
```

### Dependencies
```toml
[dependencies]
tokio = { version = "1.0", features = ["full"] }
ratatui = "0.24"
crossterm = "0.27"
clap = { version = "4.0", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
indicatif = "0.17"
ffmpeg-next = "6.0"
```