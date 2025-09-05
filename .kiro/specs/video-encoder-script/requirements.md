# Requirements Document

## Introduction

This feature involves creating a high-performance video encoding script that converts H.264 encoded MKV and MP4 files to H.265 format using AMD hardware acceleration. The script provides a user-friendly TUI interface for managing batch encoding operations with configurable quality profiles and parallel processing capabilities.

## Requirements

### Requirement 1

**User Story:** As a user, I want to select a quality profile at script startup, so that I can control the encoding quality based on my needs.

#### Acceptance Criteria

1. WHEN the script starts THEN the system SHALL display three quality options: Quality, Medium, High Compression
2. WHEN a user selects a quality profile THEN the system SHALL apply the corresponding encoding parameters for all subsequent operations
3. IF no selection is made within a reasonable timeout THEN the system SHALL default to Medium quality

### Requirement 2

**User Story:** As a user, I want the script to only process files in the target directory without recursing into subdirectories, so that I have precise control over which files are processed.

#### Acceptance Criteria

1. WHEN the script is pointed to a folder THEN the system SHALL only scan files in the immediate directory
2. WHEN subdirectories exist THEN the system SHALL ignore all files within subdirectories
3. WHEN the script processes files THEN the system SHALL only consider .mkv and .mp4 file extensions

### Requirement 3

**User Story:** As a user, I want the script to intelligently filter files for encoding, so that only appropriate files are processed and duplicates are avoided.

#### Acceptance Criteria

1. WHEN a file has H.264 encoding THEN the system SHALL include it for potential encoding
2. WHEN a file already has H.265 encoding THEN the system SHALL skip it
3. WHEN a filename contains "x265" or "h265" (case insensitive) THEN the system SHALL skip it
4. WHEN a file is not .mkv or .mp4 format THEN the system SHALL ignore it

### Requirement 4

**User Story:** As a user, I want encoded files to be organized in a separate directory, so that my original files remain organized and new files are clearly identified.

#### Acceptance Criteria

1. WHEN encoding begins THEN the system SHALL create a new directory named "{current_directory_name}-encodes"
2. WHEN a file is encoded THEN the system SHALL save it with "-h265" appended to the original filename
3. WHEN encoding completes THEN the system SHALL preserve the original file unchanged
4. WHEN the output directory doesn't exist THEN the system SHALL create it automatically

### Requirement 5

**User Story:** As a user, I want to use AMD hardware acceleration for encoding, so that the process is fast and efficient on my RX6600 GPU.

#### Acceptance Criteria

1. WHEN encoding starts THEN the system SHALL use AMD AMF (Advanced Media Framework) hardware acceleration
2. WHEN AMD hardware is not available THEN the system SHALL fall back to software encoding with a warning
3. WHEN encoding H.265 THEN the system SHALL maintain the same resolution and audio streams as the original
4. WHEN encoding THEN the system SHALL only re-encode the video stream to H.265

### Requirement 6

**User Story:** As a user, I want to configure parallel encoding jobs, so that I can optimize performance based on my system capabilities.

#### Acceptance Criteria

1. WHEN the script starts THEN the system SHALL allow configuration of concurrent encoding jobs
2. WHEN multiple files are queued THEN the system SHALL process up to the configured number simultaneously
3. WHEN system resources are limited THEN the system SHALL respect the parallel job limit
4. IF no parallel job count is specified THEN the system SHALL default to 2 concurrent jobs

### Requirement 7

**User Story:** As a user, I want to see encoding progress for individual files and overall batch progress, so that I can monitor the operation status.

#### Acceptance Criteria

1. WHEN encoding is active THEN the system SHALL display progress percentage for each active encoding job
2. WHEN multiple files are being processed THEN the system SHALL show overall batch progress
3. WHEN encoding completes for a file THEN the system SHALL update the progress display
4. WHEN all files complete THEN the system SHALL show final completion status

### Requirement 8

**User Story:** As a user, I want a colorful TUI interface to confirm encoding operations, so that I have clear control over which files are processed.

#### Acceptance Criteria

1. WHEN files are detected for encoding THEN the system SHALL display them in a colorful TUI list
2. WHEN reviewing files THEN the system SHALL allow individual confirmation for each file
3. WHEN a file is selected for encoding THEN the system SHALL visually indicate the selection
4. WHEN all selections are made THEN the system SHALL allow proceeding with batch encoding
5. IF no files are selected THEN the system SHALL exit gracefully

### Requirement 9

**User Story:** As a user, I want the script to be fast and lightweight, so that it doesn't consume excessive system resources during operation.

#### Acceptance Criteria

1. WHEN the script runs THEN the system SHALL use minimal memory footprint for the interface
2. WHEN processing files THEN the system SHALL efficiently manage system resources
3. WHEN encoding THEN the system SHALL prioritize hardware acceleration over CPU usage
4. WHEN idle THEN the system SHALL consume minimal CPU resources