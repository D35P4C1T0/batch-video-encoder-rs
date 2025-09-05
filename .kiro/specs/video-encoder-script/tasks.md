# Implementation Plan

- [x] 1. Set up project structure and core dependencies
  - Create Rust project with Cargo.toml including tokio, ratatui, crossterm, clap, thiserror, and ffmpeg-next dependencies
  - Set up main.rs with basic module structure and error handling framework
  - Create module files: cli.rs, scanner.rs, tui.rs, encoder.rs, ffmpeg.rs
  - _Requirements: 9.1, 9.2_

- [x] 2. Implement CLI argument parsing and configuration
  - Create Config struct with target directory, quality profile, and parallel job settings
  - Implement QualityProfile enum with Quality, Medium, HighCompression variants
  - Write clap-based CLI parser to handle directory input and configuration options
  - Add unit tests for configuration parsing and validation
  - _Requirements: 1.1, 1.2, 1.3, 6.4_

- [x] 3. Create video file scanning and filtering logic
  - Implement VideoFile struct with path, codec, size, and metadata fields
  - Write directory scanning function that only processes immediate directory files
  - Create codec detection logic using ffmpeg-next to identify H.264 vs H.265 files
  - Implement filtering function to exclude H.265 files and files with x265/h265 in names
  - Add unit tests for file scanning, filtering, and codec detection
  - _Requirements: 2.1, 2.2, 2.3, 3.1, 3.2, 3.3, 3.4_

- [x] 4. Build FFmpeg encoding wrapper with AMD hardware acceleration
  - Create FFmpegEncoder struct with quality profile configuration
  - Implement AMD AMF hardware acceleration detection and command generation
  - Write software encoding fallback with libx265 when hardware unavailable
  - Create encoding function that preserves audio streams and only re-encodes video to H.265
  - Add progress callback mechanism for real-time encoding progress updates
  - Write unit tests for command generation and parameter mapping
  - _Requirements: 5.1, 5.2, 5.3, 5.4_

- [x] 5. Implement output directory management and file naming
  - Create function to generate output directory name with "-encodes" suffix
  - Implement automatic directory creation when it doesn't exist
  - Write file naming logic to append "-h265" to original filenames
  - Add validation to ensure original files are preserved during encoding
  - Create unit tests for directory creation and file naming logic
  - _Requirements: 4.1, 4.2, 4.3, 4.4_

- [x] 6. Create basic TUI framework and file selection interface
  - Set up ratatui terminal initialization and cleanup
  - Create AppState struct to manage file list and selection state
  - Implement colorful file list display with checkboxes for selection
  - Add keyboard navigation (arrow keys, space to select, enter to confirm)
  - Write TUI event handling loop for user interactions
  - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5_

- [x] 7. Implement parallel encoding manager with job coordination
  - Create EncodingManager struct with configurable parallel job limits
  - Implement job queue management using tokio async tasks
  - Write encoding job spawning and completion tracking logic
  - Add proper resource management to respect parallel job limits
  - Create unit tests for job scheduling and parallel execution limits
  - _Requirements: 6.1, 6.2, 6.3_

- [x] 8. Add progress tracking and real-time display updates
  - Create ProgressUpdate struct with file-level and batch-level progress data
  - Implement progress calculation for individual encoding jobs
  - Write overall batch progress tracking across multiple parallel jobs
  - Add real-time TUI updates to display encoding progress bars and statistics
  - Create progress persistence to handle TUI refresh and updates
  - _Requirements: 7.1, 7.2, 7.3, 7.4_

- [x] 9. Integrate all components and implement main application flow
  - Wire together CLI parsing, file scanning, TUI selection, and encoding manager
  - Implement complete application workflow from startup to completion
  - Add proper error handling and user feedback throughout the process
  - Create graceful shutdown handling for interrupted operations
  - Write integration tests for complete encoding workflow
  - _Requirements: 1.1, 2.1, 3.1, 4.1, 5.1, 6.1, 7.1, 8.1_

- [x] 10. Add comprehensive error handling and recovery mechanisms
  - Implement custom error types for different failure scenarios
  - Add hardware acceleration fallback with user notification
  - Create file-level error recovery to continue batch processing on individual failures
  - Implement TUI error display and user confirmation for error scenarios
  - Write unit tests for error handling and recovery paths
  - _Requirements: 5.2, 8.5, 9.3, 9.4_

- [x] 11. Optimize performance and resource usage
  - Profile memory usage during large batch operations and optimize data structures
  - Implement efficient progress update batching to minimize TUI refresh overhead
  - Add resource monitoring to prevent system overload during parallel encoding
  - Optimize file metadata caching to reduce repeated filesystem operations
  - Create performance benchmarks and validate against lightweight requirements
  - _Requirements: 9.1, 9.2, 9.4_

- [x] 12. Create comprehensive test suite and validation
  - Write integration tests with sample video files for end-to-end validation
  - Add performance tests to verify encoding speed and resource usage
  - Create test cases for AMD hardware detection and fallback scenarios
  - Implement automated testing for TUI interactions and user workflows
  - Add validation tests for output file quality and metadata preservation
  - _Requirements: 3.1, 5.1, 5.4, 7.4, 9.1_