use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::signal;
use tokio::time::{sleep, Duration};

use video_encoder::{AppError, Result, ErrorSeverity, cli, scanner, encoder, tui, error_recovery};
use video_encoder::performance::PerformanceMonitor;
use video_encoder::benchmarks::VideoEncoderBenchmarks;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Set up graceful shutdown handling
    let shutdown_handler = setup_shutdown_handler();
    
    // Initialize performance monitoring
    let performance_monitor = PerformanceMonitor::new();
    
    // Parse CLI arguments
    let config = cli::Config::parse_args()?;
    
    if config.verbose {
        println!("Video Encoder - AMD Hardware Acceleration");
        println!("Target directory: {:?}", config.target_directory);
        println!("Quality profile: {:?} ({})", config.quality_profile, config.quality_profile.description());
        println!("Max parallel jobs: {}", config.max_parallel_jobs);
        println!("Output directory: {:?}", config.output_directory());
    }
    
    // Scan for video files with performance monitoring
    println!("Scanning directory: {:?}", config.target_directory);
    let all_files = scanner::scan_directory_with_cache(&config.target_directory, Some(&performance_monitor))?;
    let encodable_files = scanner::filter_encodable_files(all_files);
    
    if encodable_files.is_empty() {
        println!("No H.264 files found for encoding.");
        return Ok(());
    }
    
    println!("Found {} files that can be encoded to H.265", encodable_files.len());
    
    // Launch TUI for file selection
    if config.verbose {
        println!("Launching TUI for file selection...");
    }
    
    let selected_files = match tui::TuiManager::new(encodable_files.clone()) {
        Ok(mut tui_manager) => {
            if config.verbose {
                println!("TUI manager created, running file selection...");
            }
            tui_manager.run_file_selection()?
        }
        Err(e) => {
            if config.verbose {
                println!("TUI initialization failed ({}), auto-selecting all files for headless mode", e);
            } else {
                println!("Running in headless mode, auto-selecting all {} files", encodable_files.len());
            }
            encodable_files.clone()
        }
    };
    
    if config.verbose {
        println!("File selection completed, {} files selected", selected_files.len());
    }
    
    if selected_files.is_empty() {
        println!("No files selected for encoding. Exiting.");
        return Ok(());
    }
    
    if config.verbose {
        println!("Selected {} files for encoding:", selected_files.len());
        for file in &selected_files {
            println!("  - {}", file.filename);
        }
    }
    
    // Create output directory
    let output_dir = config.output_directory();
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)
            .map_err(AppError::from)?;
        
        if config.verbose {
            println!("Created output directory: {:?}", output_dir);
        }
    }
    
    // Set up encoding manager with error recovery and progress tracking
    let (mut encoding_manager, progress_rx, error_notification_rx) = encoder::EncodingManager::new_with_error_recovery(config.max_parallel_jobs);
    
    // Configure performance monitoring
    encoding_manager.get_performance_monitor().resource_monitor().set_max_parallel_jobs(config.max_parallel_jobs);
    
    // Create encoding jobs
    let encoding_jobs: Vec<encoder::EncodingJob> = selected_files
        .iter()
        .map(|file| {
            let output_filename = generate_output_filename(&file.filename);
            let output_path = output_dir.join(output_filename);
            
            encoder::EncodingJob {
                input_path: file.path.clone(),
                output_path,
                quality_profile: config.quality_profile.clone(),
            }
        })
        .collect();
    
    // Queue all encoding jobs
    encoding_manager.queue_jobs(encoding_jobs);
    
    // Run encoding with progress display and error handling
    let (encoding_result, final_encoding_manager) = run_encoding_with_progress(
        encoding_manager,
        progress_rx,
        error_notification_rx,
        selected_files,
        config.verbose,
        shutdown_handler,
    ).await;
    
    match encoding_result {
        Ok(summary) => {
            println!("\nEncoding completed!");
            println!("Successfully encoded: {} files", summary.completed_jobs);
            if summary.failed_jobs > 0 {
                println!("Failed to encode: {} files", summary.failed_jobs);
            }
            
            // Print performance statistics if verbose
            if config.verbose {
                println!("\n=== Performance Statistics ===");
                let perf_stats = final_encoding_manager.get_performance_stats().await;
                println!("Memory usage: {:.1} MB", perf_stats.memory.current_usage_bytes as f64 / (1024.0 * 1024.0));
                println!("Peak memory: {:.1} MB", perf_stats.memory.peak_usage_bytes as f64 / (1024.0 * 1024.0));
                println!("Progress batching efficiency: {:.1}%", perf_stats.progress_batching.batching_efficiency_percent);
                println!("Metadata cache hit rate: {:.1}%", perf_stats.metadata_cache.hit_rate_percent);
            }
        }
        Err(e) => {
            eprintln!("Encoding failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

/// Run performance benchmarks (hidden CLI option for development)
pub async fn run_benchmarks(test_directory: &std::path::Path) -> Result<()> {
    println!("Running performance benchmarks...");
    
    let benchmarks = VideoEncoderBenchmarks::new();
    
    // Run comprehensive benchmarks
    let suite = benchmarks.run_all_benchmarks(test_directory).await?;
    suite.print_results();
    
    // Validate lightweight requirements
    let validation = benchmarks.validate_lightweight_requirements().await?;
    validation.print_validation();
    
    Ok(())
}

/// Set up graceful shutdown handling for Ctrl+C and other signals
fn setup_shutdown_handler() -> Arc<AtomicBool> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to create SIGTERM handler");
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .expect("Failed to create SIGINT handler");
        
        tokio::select! {
            _ = sigterm.recv() => {
                println!("\nReceived SIGTERM, shutting down gracefully...");
                shutdown_clone.store(true, Ordering::SeqCst);
            }
            _ = sigint.recv() => {
                println!("\nReceived SIGINT (Ctrl+C), shutting down gracefully...");
                shutdown_clone.store(true, Ordering::SeqCst);
            }
        }
    });
    
    shutdown
}

/// Generate output filename with H.265 suffix
pub fn generate_output_filename(input_filename: &str) -> String {
    if let Some(dot_pos) = input_filename.rfind('.') {
        let (name, ext) = input_filename.split_at(dot_pos);
        format!("{}-h265{}", name, ext)
    } else {
        format!("{}-h265", input_filename)
    }
}

/// Run encoding with progress display, error handling, and user feedback
async fn run_encoding_with_progress(
    mut encoding_manager: encoder::EncodingManager,
    mut progress_rx: mpsc::Receiver<encoder::ProgressUpdate>,
    mut error_notification_rx: mpsc::Receiver<error_recovery::ErrorNotification>,
    selected_files: Vec<scanner::VideoFile>,
    verbose: bool,
    shutdown_handler: Arc<AtomicBool>,
) -> (Result<encoder::EncodingSummary>, encoder::EncodingManager) {
    // Initialize TUI for progress display with error handling
    let mut tui_manager = match tui::TuiManager::new(selected_files.clone()) {
        Ok(manager) => manager,
        Err(e) => return (Err(e), encoding_manager),
    };
    
    if let Err(e) = tui_manager.initialize_encoding_mode(&selected_files) {
        return (Err(e), encoding_manager);
    }
    
    // Main encoding loop
    loop {
        // Check for shutdown signal
        if shutdown_handler.load(Ordering::SeqCst) {
            println!("Shutdown requested, cancelling remaining jobs...");
            encoding_manager.cancel_all_jobs().await;
            break;
        }
        
        // Process encoding jobs (start new ones, check completions)
        if let Err(e) = encoding_manager.process_jobs().await {
            let _ = tui_manager.cleanup();
            return (Err(e), encoding_manager);
        }
        
        // Optimize memory usage periodically
        static mut OPTIMIZATION_COUNTER: usize = 0;
        unsafe {
            OPTIMIZATION_COUNTER += 1;
            if OPTIMIZATION_COUNTER % 100 == 0 {
                encoding_manager.optimize_memory_usage();
                tui_manager.optimize_memory_usage().await;
            }
        }
        
        // Handle progress updates with batching
        while let Ok(progress_update) = progress_rx.try_recv() {
            encoding_manager.update_job_progress(&progress_update.file_path, progress_update.progress_percent);
            
            // Update TUI with progress (now uses batching internally)
            if let Err(e) = tui_manager.update_file_progress(&progress_update.file_path, &progress_update).await {
                let _ = tui_manager.cleanup();
                return (Err(e), encoding_manager);
            }
            
            if verbose {
                println!(
                    "Progress: {} - {:.1}%{}",
                    progress_update.file_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                    progress_update.progress_percent,
                    if let Some(fps) = progress_update.current_fps {
                        format!(" ({:.1} fps)", fps)
                    } else {
                        String::new()
                    }
                );
            }
        }
        
        // Handle error notifications
        while let Ok(error_notification) = error_notification_rx.try_recv() {
            // Update TUI with error information
            tui_manager.get_app_state_mut().add_error_notification(error_notification.clone());
            
            if verbose {
                match &error_notification {
                    error_recovery::ErrorNotification::HardwareFallback { reason, fallback_method } => {
                        println!("Hardware fallback: {} -> {}", reason, fallback_method);
                    }
                    error_recovery::ErrorNotification::FileSkipped { file, reason } => {
                        println!("Skipped file {}: {}", file.display(), reason);
                    }
                    error_recovery::ErrorNotification::CriticalError { error, .. } => {
                        eprintln!("Critical error: {}", error);
                    }
                    _ => {}
                }
            }
        }
        
        // Update TUI with current encoding summary and error statistics
        let summary = encoding_manager.get_summary();
        let error_stats = encoding_manager.get_error_statistics();
        if let Err(e) = tui_manager.update_batch_progress(&summary) {
            let _ = tui_manager.cleanup();
            return (Err(e), encoding_manager);
        }
        tui_manager.get_app_state_mut().update_error_statistics(error_stats);
        
        // Check if all jobs are complete
        if encoding_manager.is_complete() {
            break;
        }
        
        // Adaptive delay based on system load
        let performance_stats = encoding_manager.get_performance_stats().await;
        let delay_ms = if performance_stats.resource_usage.estimated_cpu_usage > 80.0 {
            200 // Longer delay when CPU is high
        } else if performance_stats.resource_usage.active_jobs == 0 {
            50  // Shorter delay when idle
        } else {
            100 // Normal delay
        };
        sleep(Duration::from_millis(delay_ms)).await;
    }
    
    // Clean up TUI
    if let Err(e) = tui_manager.cleanup() {
        return (Err(e), encoding_manager);
    }
    
    // Return final summary and encoding manager
    (Ok(encoding_manager.get_summary()), encoding_manager)
}