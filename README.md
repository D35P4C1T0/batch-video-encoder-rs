# H.265 Video Encoder

A high-performance video encoding tool with AMD hardware acceleration support, designed for batch processing H.264 videos to H.265 format with intelligent fallback and comprehensive error handling.

## 🚀 Features

### Core Functionality
- **Batch H.264 to H.265 Conversion**: Efficiently encode multiple video files
- **AMD Hardware Acceleration**: Automatic AMD AMF detection with software fallback
- **Smart File Filtering**: Automatically skips already encoded H.265 files
- **Multiple Quality Profiles**: Quality, Medium, and High Compression presets
- **Parallel Processing**: Configurable concurrent encoding jobs (1-8)
- **Progress Tracking**: Real-time TUI with progress bars and ETA

### Advanced Features
- **Intelligent Error Recovery**: Automatic hardware fallback and file-level error handling
- **Performance Monitoring**: Memory usage tracking and resource optimization
- **Metadata Preservation**: Maintains audio streams, subtitles, and video metadata
- **Graceful Shutdown**: Ctrl+C handling with proper cleanup
- **Comprehensive Logging**: Verbose mode with detailed encoding statistics

## 📋 Requirements

- **Rust**: 1.70+ (2021 edition)
- **FFmpeg**: With AMD AMF support (for hardware acceleration)
- **Operating System**: Linux (tested), Windows and macOS (should work)
- **Hardware**: AMD GPU with AMF support (optional, software fallback available)

## 🛠️ Installation

### From Source
```bash
git clone https://github.com/yourusername/h265-video-encoder.git
cd h265-video-encoder
cargo build --release
```

### Dependencies
Ensure FFmpeg is installed with AMD AMF support:
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install ffmpeg

# Check for AMD AMF support
ffmpeg -encoders | grep amf
```

## 🎯 Usage

### Basic Usage
```bash
# Encode all H.264 videos in a directory
./target/release/video-encoder /path/to/videos

# Use high quality preset
./target/release/video-encoder -q quality /path/to/videos

# Enable verbose logging with 4 parallel jobs
./target/release/video-encoder -v -j 4 /path/to/videos
```

### Command Line Options
```
Usage: video-encoder [OPTIONS] <TARGET_DIRECTORY>

Arguments:
  <TARGET_DIRECTORY>  Directory containing .mkv and .mp4 files

Options:
  -q, --quality <PROFILE>     Encoding quality profile [default: medium]
                              • quality: High quality (CRF 20)
                              • medium: Balanced (CRF 23) 
                              • high-compression: Maximum compression (CRF 28)
  
  -j, --jobs <COUNT>          Parallel encoding jobs 1-8 [default: 2]
  -v, --verbose              Enable detailed logging
  -h, --help                 Show help information
  -V, --version              Show version
```

### Quality Profiles

| Profile | CRF | Use Case | File Size | Quality |
|---------|-----|----------|-----------|---------|
| `quality` | 20 | Archival, high-quality content | Larger | Excellent |
| `medium` | 23 | General use, streaming | Balanced | Very Good |
| `high-compression` | 28 | Storage-constrained, web | Smaller | Good |

## 🔧 Features in Detail

### Hardware Acceleration
- **Automatic Detection**: Detects AMD AMF availability at runtime
- **Intelligent Fallback**: Seamlessly switches to software encoding if hardware fails
- **Performance Optimization**: Uses hardware acceleration when available for faster encoding

### Error Handling
- **File-Level Recovery**: Skips problematic files and continues batch processing
- **Hardware Fallback**: Automatically switches to software encoding on hardware failures
- **Comprehensive Logging**: Detailed error reporting with recovery suggestions

### Performance Monitoring
- **Memory Tracking**: Monitors and optimizes memory usage during encoding
- **Progress Batching**: Efficient UI updates to minimize overhead
- **Resource Management**: Adaptive job scheduling based on system load

### File Management
- **Smart Filtering**: Automatically excludes files already containing "h265" or "x265"
- **Safe Processing**: Never overwrites original files
- **Organized Output**: Creates separate output directory with "-encodes" suffix

## 📊 Example Output

```
Video Encoder - AMD Hardware Acceleration
Target directory: "/home/user/videos"
Quality profile: Medium (Balanced quality and size)
Max parallel jobs: 2
Output directory: "/home/user/videos-encodes"

Scanning directory: "/home/user/videos"
Found 15 files that can be encoded to H.265

✓ AMD AMF hardware acceleration detected and available

Encoding Progress:
[████████████████████████████████████████] 100% (15/15)
├─ movie1.mkv ████████████████████████████████████████ 100% (28.5 fps)
├─ series_s01e01.mkv ████████████████████████████████████████ 100% (32.1 fps)
└─ documentary.mp4 ████████████████████████████████████████ 100% (25.8 fps)

Encoding completed!
Successfully encoded: 15 files
Total time: 2h 34m 12s
Average speed: 29.1 fps
Space saved: 2.3 GB (31% reduction)

=== Performance Statistics ===
Memory usage: 156.2 MB
Peak memory: 203.8 MB
Progress batching efficiency: 87.3%
Metadata cache hit rate: 94.1%
```

## 🧪 Testing

The project includes comprehensive test coverage:

```bash
# Run all tests
cargo test

# Run specific test suites
cargo test --test integration_tests
cargo test --test performance_tests
cargo test --test hardware_detection_tests
cargo test --test tui_interaction_tests
cargo test --test output_validation_tests

# Run with output
cargo test -- --nocapture
```

### Test Coverage
- **Integration Tests**: End-to-end workflow validation
- **Performance Tests**: Memory usage and encoding speed benchmarks
- **Hardware Tests**: AMD AMF detection and fallback scenarios
- **TUI Tests**: User interface and progress tracking
- **Output Tests**: Quality validation and metadata preservation

## 🏗️ Architecture

### Core Components
- **CLI Module**: Command-line argument parsing and validation
- **Scanner**: Directory scanning and video file detection
- **Encoder**: Job management and parallel processing coordination
- **FFmpeg**: Hardware/software encoding with progress tracking
- **TUI**: Terminal user interface with real-time progress
- **Error Recovery**: Comprehensive error handling and recovery strategies

### Key Design Principles
- **Fail-Safe Operation**: Never damages original files
- **Graceful Degradation**: Automatic fallback when hardware unavailable
- **Resource Efficiency**: Optimized memory usage and parallel processing
- **User Experience**: Clear progress indication and error reporting

## 🤝 Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development Setup
```bash
git clone https://github.com/yourusername/h265-video-encoder.git
cd h265-video-encoder
cargo build
cargo test
```

### Areas for Contribution
- Additional hardware acceleration support (NVIDIA NVENC, Intel QSV)
- More video format support
- GUI interface
- Docker containerization
- Performance optimizations

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **FFmpeg Team**: For the excellent multimedia framework
- **AMD**: For AMF hardware acceleration support
- **Rust Community**: For the amazing ecosystem and tools

## 📞 Support

If you encounter issues:

1. **Check FFmpeg Installation**: Ensure FFmpeg is properly installed with AMD AMF support
2. **Hardware Compatibility**: Verify your AMD GPU supports AMF encoding
3. **File Permissions**: Ensure read access to input directory and write access to output location
4. **System Resources**: Monitor available disk space and memory

For bug reports and feature requests, please open an issue on GitHub.

---

**Made with ❤️ and Rust** - Efficient video encoding for everyone!