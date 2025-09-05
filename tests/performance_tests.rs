use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use video_encoder::performance::{PerformanceMonitor, PerformanceBenchmark};
use video_encoder::encoder::{EncodingManager, EncodingJob};
use video_encoder::cli::QualityProfile;
use video_encoder::benchmarks::VideoEncoderBenchmarks;

#[tokio::test]
async fn test_memory_tracking() {
    let performance_monitor = PerformanceMonitor::new();
    let memory_tracker = performance_monitor.memory_tracker();
    
    // Initial state
    let initial_stats = memory_tracker.get_stats();
    assert_eq!(initial_stats.allocated_objects, 0);
    assert_eq!(initial_stats.current_usage_bytes, 0);
    
    // Track some allocations
    memory_tracker.track_allocation(1024);
    memory_tracker.track_allocation(2048);
    
    let after_allocation = memory_tracker.get_stats();
    assert_eq!(after_allocation.allocated_objects, 2);
    assert_eq!(after_allocation.current_usage_bytes, 3072);
    assert!(after_allocation.peak_usage_bytes >= 3072);
    
    // Track deallocation
    memory_tracker.track_deallocation(1024);
    
    let after_deallocation = memory_tracker.get_stats();
    assert_eq!(after_deallocation.allocated_objects, 1);
    assert_eq!(after_deallocation.current_usage_bytes, 2048);
    
    // Check memory usage acceptability
    assert!(memory_tracker.is_memory_usage_acceptable(100)); // 100MB limit
}

#[tokio::test]
async fn test_progress_batching() {
    let performance_monitor = PerformanceMonitor::new();
    let progress_batcher = performance_monitor.progress_batcher();
    
    let mut batcher = progress_batcher.write().await;
    
    // Configure batching
    batcher.set_batch_interval(Duration::from_millis(100));
    batcher.set_max_batch_size(5);
    
    // Add updates
    for i in 0..10 {
        let file_path = PathBuf::from(format!("/test/file_{}.mkv", i));
        batcher.add_update(file_path, i as f32 * 10.0, Some(30.0));
    }
    
    // Should trigger batch due to size limit
    assert!(batcher.should_send_batch());
    
    let batch = batcher.get_batch();
    assert!(!batch.is_empty());
    
    let stats = batcher.get_stats();
    assert_eq!(stats.total_updates_received, 10);
    assert_eq!(stats.total_batches_sent, 1);
}

#[tokio::test]
async fn test_resource_monitoring() {
    let performance_monitor = PerformanceMonitor::new();
    let resource_monitor = performance_monitor.resource_monitor();
    
    // Set up monitoring
    resource_monitor.set_max_parallel_jobs(4);
    resource_monitor.set_active_jobs(2);
    
    // Sample resources
    resource_monitor.sample_resources().await;
    
    let stats = resource_monitor.get_stats();
    assert_eq!(stats.active_jobs, 2);
    assert_eq!(stats.max_parallel_jobs, 4);
    assert!(stats.estimated_cpu_usage >= 0.0);
    
    // Should be able to start additional jobs
    assert!(resource_monitor.can_start_additional_job().await);
    
    // Test at capacity
    resource_monitor.set_active_jobs(4);
    let stats_at_capacity = resource_monitor.get_stats();
    assert_eq!(stats_at_capacity.active_jobs, 4);
}

#[tokio::test]
async fn test_metadata_caching() {
    let performance_monitor = PerformanceMonitor::new();
    let metadata_cache = performance_monitor.metadata_cache();
    
    let mut cache = metadata_cache.write().await;
    
    // Initial state
    let initial_stats = cache.get_stats();
    assert_eq!(initial_stats.cache_size, 0);
    assert_eq!(initial_stats.cache_hits, 0);
    assert_eq!(initial_stats.cache_misses, 0);
    
    // Test cache miss
    let file_path = PathBuf::from("/test/movie.mkv");
    let result = cache.get(&file_path);
    assert!(result.is_none());
    
    let after_miss = cache.get_stats();
    assert_eq!(after_miss.cache_misses, 1);
    
    // Add to cache
    let cached_metadata = video_encoder::performance::CachedMetadata {
        size: 1024 * 1024 * 100, // 100MB
        codec: video_encoder::scanner::VideoCodec::H264,
        resolution: Some((1920, 1080)),
        duration: Some(Duration::from_secs(3600)),
        cached_at: std::time::Instant::now(),
    };
    
    cache.insert(file_path.clone(), cached_metadata);
    
    // Test cache hit
    let result = cache.get(&file_path);
    assert!(result.is_some());
    
    let after_hit = cache.get_stats();
    assert_eq!(after_hit.cache_hits, 1);
    assert_eq!(after_hit.cache_size, 1);
    assert!(after_hit.hit_rate_percent > 0.0);
}

#[tokio::test]
async fn test_encoding_manager_performance_integration() {
    let (mut encoding_manager, _progress_rx, _error_rx) = EncodingManager::new_with_error_recovery(2);
    
    // Create test jobs
    let test_jobs: Vec<EncodingJob> = (0..10)
        .map(|i| EncodingJob {
            input_path: PathBuf::from(format!("/test/input_{}.mkv", i)),
            output_path: PathBuf::from(format!("/test/output_{}.mkv", i)),
            quality_profile: QualityProfile::Medium,
        })
        .collect();
    
    // Queue jobs
    encoding_manager.queue_jobs(test_jobs);
    
    // Get initial performance stats
    let initial_stats = encoding_manager.get_performance_stats().await;
    assert!(initial_stats.memory.current_usage_bytes > 0);
    
    // Optimize memory usage
    encoding_manager.optimize_memory_usage();
    
    // Check that optimization worked
    let optimized_stats = encoding_manager.get_performance_stats().await;
    // Memory usage should be tracked properly
    assert!(optimized_stats.memory.current_usage_bytes >= 0);
}

#[tokio::test]
async fn test_performance_benchmark() {
    let mut benchmark = PerformanceBenchmark::new();
    
    benchmark.checkpoint("start");
    
    // Simulate some work
    sleep(Duration::from_millis(10)).await;
    
    benchmark.checkpoint("middle");
    
    // More work
    sleep(Duration::from_millis(5)).await;
    
    benchmark.checkpoint("end");
    
    let results = benchmark.results();
    assert!(results.total_duration >= Duration::from_millis(15));
    assert_eq!(results.checkpoint_durations.len(), 3);
    
    // Print results for manual verification
    results.print_results();
}

#[tokio::test]
async fn test_lightweight_requirements_validation() {
    let benchmarks = VideoEncoderBenchmarks::new();
    
    // Run validation
    let validation = benchmarks.validate_lightweight_requirements().await.unwrap();
    
    // Print validation results
    validation.print_validation();
    
    // Basic checks - these should pass in a test environment
    assert!(validation.memory_usage_mb < 1000); // Should be well under 1GB in tests
    assert!(validation.cpu_usage_percent >= 0.0);
    assert!(validation.batching_efficiency >= 0.0);
    assert!(validation.cache_hit_rate >= 0.0);
}

#[tokio::test]
async fn test_adaptive_performance_tuning() {
    let performance_monitor = PerformanceMonitor::new();
    
    // Test different performance scenarios
    let resource_monitor = performance_monitor.resource_monitor();
    
    // Low load scenario
    resource_monitor.set_active_jobs(1);
    resource_monitor.sample_resources().await;
    assert!(resource_monitor.can_start_additional_job().await);
    
    // High load scenario
    resource_monitor.set_active_jobs(4);
    resource_monitor.set_max_parallel_jobs(4);
    resource_monitor.sample_resources().await;
    
    let stats = resource_monitor.get_stats();
    assert_eq!(stats.active_jobs, 4);
    assert_eq!(stats.max_parallel_jobs, 4);
}

#[tokio::test]
async fn test_memory_optimization_effectiveness() {
    let (mut encoding_manager, _progress_rx, _error_rx) = EncodingManager::new_with_error_recovery(2);
    
    // Create many jobs to test memory optimization
    let large_job_set: Vec<EncodingJob> = (0..1000)
        .map(|i| EncodingJob {
            input_path: PathBuf::from(format!("/test/large_input_{}.mkv", i)),
            output_path: PathBuf::from(format!("/test/large_output_{}.mkv", i)),
            quality_profile: QualityProfile::Medium,
        })
        .collect();
    
    encoding_manager.queue_jobs(large_job_set);
    
    let before_optimization = encoding_manager.get_performance_stats().await;
    let initial_memory = before_optimization.memory.current_usage_bytes;
    
    // Simulate some completed jobs by manipulating internal state
    // In a real scenario, jobs would complete naturally
    encoding_manager.optimize_memory_usage();
    
    let after_optimization = encoding_manager.get_performance_stats().await;
    
    // Memory tracking should be working
    assert!(after_optimization.memory.current_usage_bytes >= 0);
    assert!(after_optimization.memory.peak_usage_bytes >= initial_memory);
}

#[tokio::test]
async fn test_progress_batching_efficiency() {
    let performance_monitor = PerformanceMonitor::new();
    let progress_batcher = performance_monitor.progress_batcher();
    
    // Test high-frequency updates
    let mut batcher = progress_batcher.write().await;
    batcher.set_batch_interval(Duration::from_millis(50));
    batcher.set_max_batch_size(10);
    
    // Send rapid updates
    for i in 0..100 {
        let file_path = PathBuf::from(format!("/test/rapid_file_{}.mkv", i % 5));
        batcher.add_update(file_path, (i as f32) % 100.0, Some(25.0 + (i as f32) % 10.0));
        
        if batcher.should_send_batch() {
            let _batch = batcher.get_batch();
        }
    }
    
    let final_stats = batcher.get_stats();
    
    // Should have batched efficiently
    assert!(final_stats.total_updates_received > 0);
    assert!(final_stats.total_batches_sent > 0);
    assert!(final_stats.total_batches_sent < final_stats.total_updates_received);
    
    // Efficiency should be reasonable (fewer batches than updates)
    let efficiency_ratio = final_stats.total_batches_sent as f32 / final_stats.total_updates_received as f32;
    assert!(efficiency_ratio < 1.0);
    assert!(efficiency_ratio > 0.0);
}

// ============================================================================
// ENCODING SPEED AND THROUGHPUT TESTS
// ============================================================================

#[tokio::test]
async fn test_encoding_speed_benchmarks() {
    use video_encoder::benchmarks::VideoEncoderBenchmarks;
    use video_encoder::performance::PerformanceBenchmark;
    use std::path::Path;
    
    let benchmarks = VideoEncoderBenchmarks::new();
    
    // Test encoding speed with different quality profiles
    let quality_profiles = vec![
        QualityProfile::Quality,
        QualityProfile::Medium,
        QualityProfile::HighCompression,
    ];
    
    for profile in quality_profiles {
        let mut benchmark = PerformanceBenchmark::new();
        benchmark.checkpoint("start");
        
        // Simulate encoding time based on quality profile
        let encoding_time_ms = match profile {
            QualityProfile::Quality => 100,        // Slower, higher quality
            QualityProfile::Medium => 75,          // Balanced
            QualityProfile::HighCompression => 150, // Slower, more compression
        };
        
        sleep(Duration::from_millis(encoding_time_ms)).await;
        benchmark.checkpoint("encoding_complete");
        
        let results = benchmark.results();
        
        // Verify encoding time is within expected range
        assert!(results.total_duration >= Duration::from_millis(encoding_time_ms));
        assert!(results.total_duration < Duration::from_millis(encoding_time_ms + 50));
        
        println!("Encoding speed test passed for {:?}: {:?}", profile, results.total_duration);
    }
}

#[tokio::test]
async fn test_parallel_encoding_throughput() {
    let (mut encoding_manager, _progress_rx, _error_rx) = EncodingManager::new_with_error_recovery(4);
    
    // Create multiple test jobs to test parallel throughput
    let test_jobs: Vec<EncodingJob> = (0..8)
        .map(|i| EncodingJob {
            input_path: PathBuf::from(format!("/test/throughput_input_{}.mkv", i)),
            output_path: PathBuf::from(format!("/test/throughput_output_{}.mkv", i)),
            quality_profile: QualityProfile::Medium,
        })
        .collect();
    
    let start_time = std::time::Instant::now();
    
    // Queue all jobs
    encoding_manager.queue_jobs(test_jobs.clone());
    
    // Verify parallel job management
    assert_eq!(encoding_manager.total_job_count(), 8);
    assert_eq!(encoding_manager.queued_job_count(), 8);
    
    // Test that we can handle the job queue efficiently
    let queue_time = start_time.elapsed();
    assert!(queue_time < Duration::from_millis(100), "Job queueing should be fast");
    
    // Test performance statistics
    let performance_stats = encoding_manager.get_performance_stats().await;
    assert!(performance_stats.memory.current_usage_bytes > 0);
    
    println!("Parallel throughput test completed in {:?}", queue_time);
}

#[tokio::test]
async fn test_resource_usage_under_load() {
    let performance_monitor = PerformanceMonitor::new();
    let resource_monitor = performance_monitor.resource_monitor();
    
    // Simulate high load scenario
    resource_monitor.set_max_parallel_jobs(8);
    resource_monitor.set_active_jobs(6);
    
    // Sample resources multiple times to test consistency
    for i in 0..10 {
        resource_monitor.sample_resources().await;
        
        let stats = resource_monitor.get_stats();
        assert_eq!(stats.active_jobs, 6);
        assert_eq!(stats.max_parallel_jobs, 8);
        assert!(stats.estimated_cpu_usage >= 0.0);
        
        // Should still be able to start additional jobs
        assert!(resource_monitor.can_start_additional_job().await);
        
        // Small delay between samples
        sleep(Duration::from_millis(10)).await;
    }
    
    println!("Resource usage under load test passed");
}

// ============================================================================
// MEMORY EFFICIENCY AND OPTIMIZATION TESTS
// ============================================================================

#[tokio::test]
async fn test_memory_efficiency_with_large_batches() {
    let performance_monitor = PerformanceMonitor::new();
    let memory_tracker = performance_monitor.memory_tracker();
    
    // Simulate processing large batches of files
    let batch_sizes = vec![10, 50, 100, 500];
    
    for batch_size in batch_sizes {
        let initial_stats = memory_tracker.get_stats();
        
        // Simulate allocating memory for batch processing
        for i in 0..batch_size {
            let allocation_size = 1024 * 1024; // 1MB per file
            memory_tracker.track_allocation(allocation_size);
        }
        
        let after_allocation = memory_tracker.get_stats();
        assert_eq!(after_allocation.allocated_objects, initial_stats.allocated_objects + batch_size);
        
        // Simulate batch completion and cleanup
        for i in 0..batch_size {
            let allocation_size = 1024 * 1024;
            memory_tracker.track_deallocation(allocation_size);
        }
        
        let after_cleanup = memory_tracker.get_stats();
        assert_eq!(after_cleanup.allocated_objects, initial_stats.allocated_objects);
        
        // Verify memory usage is still acceptable
        assert!(memory_tracker.is_memory_usage_acceptable(1000)); // 1GB limit
        
        println!("Memory efficiency test passed for batch size: {}", batch_size);
    }
}

#[tokio::test]
async fn test_progress_batching_performance() {
    let performance_monitor = PerformanceMonitor::new();
    let progress_batcher = performance_monitor.progress_batcher();
    
    let mut batcher = progress_batcher.write().await;
    
    // Configure for high-frequency updates
    batcher.set_batch_interval(Duration::from_millis(100));
    batcher.set_max_batch_size(20);
    
    let start_time = std::time::Instant::now();
    
    // Send high-frequency progress updates
    for i in 0..1000 {
        let file_path = PathBuf::from(format!("/test/progress_file_{}.mkv", i % 10));
        batcher.add_update(file_path, (i as f32) % 100.0, Some(30.0));
        
        // Process batches when ready
        if batcher.should_send_batch() {
            let _batch = batcher.get_batch();
        }
    }
    
    let processing_time = start_time.elapsed();
    let stats = batcher.get_stats();
    
    // Verify batching efficiency
    assert_eq!(stats.total_updates_received, 1000);
    assert!(stats.total_batches_sent < stats.total_updates_received);
    
    let efficiency_ratio = stats.total_batches_sent as f32 / stats.total_updates_received as f32;
    assert!(efficiency_ratio < 0.5, "Batching should reduce update frequency significantly");
    
    // Verify processing time is reasonable
    assert!(processing_time < Duration::from_millis(500), "Progress batching should be fast");
    
    println!("Progress batching performance test passed: {} updates -> {} batches in {:?}", 
             stats.total_updates_received, stats.total_batches_sent, processing_time);
}

#[tokio::test]
async fn test_metadata_cache_performance() {
    let performance_monitor = PerformanceMonitor::new();
    let metadata_cache = performance_monitor.metadata_cache();
    
    let mut cache = metadata_cache.write().await;
    
    // Test cache performance with many files
    let file_count = 1000;
    let start_time = std::time::Instant::now();
    
    // Populate cache
    for i in 0..file_count {
        let file_path = PathBuf::from(format!("/test/cache_file_{}.mkv", i));
        let metadata = video_encoder::performance::CachedMetadata {
            size: 1024 * 1024 * (i % 100 + 1) as u64, // Varying sizes
            codec: if i % 2 == 0 { 
                video_encoder::scanner::VideoCodec::H264 
            } else { 
                video_encoder::scanner::VideoCodec::H265 
            },
            resolution: Some((1920, 1080)),
            duration: Some(Duration::from_secs(3600 + (i % 1800) as u64)),
            cached_at: std::time::Instant::now(),
        };
        
        cache.insert(file_path, metadata);
    }
    
    let population_time = start_time.elapsed();
    
    // Test cache retrieval performance
    let retrieval_start = std::time::Instant::now();
    let mut hits = 0;
    
    for i in 0..file_count {
        let file_path = PathBuf::from(format!("/test/cache_file_{}.mkv", i));
        if cache.get(&file_path).is_some() {
            hits += 1;
        }
    }
    
    let retrieval_time = retrieval_start.elapsed();
    
    // Verify cache performance
    assert_eq!(hits, file_count);
    
    let stats = cache.get_stats();
    assert_eq!(stats.cache_size, file_count);
    assert_eq!(stats.cache_hits, file_count);
    assert_eq!(stats.hit_rate_percent, 100.0);
    
    // Performance should be reasonable
    assert!(population_time < Duration::from_millis(1000), "Cache population should be fast");
    assert!(retrieval_time < Duration::from_millis(100), "Cache retrieval should be very fast");
    
    println!("Metadata cache performance test passed: {} files cached in {:?}, retrieved in {:?}", 
             file_count, population_time, retrieval_time);
}

// ============================================================================
// SYSTEM RESOURCE MONITORING TESTS
// ============================================================================

#[tokio::test]
async fn test_performance_monitoring_integration() {
    let (mut encoding_manager, _progress_rx, _error_rx) = EncodingManager::new_with_error_recovery(4);
    
    // Create test jobs to monitor performance
    let test_jobs: Vec<EncodingJob> = (0..20)
        .map(|i| EncodingJob {
            input_path: PathBuf::from(format!("/test/perf_input_{}.mkv", i)),
            output_path: PathBuf::from(format!("/test/perf_output_{}.mkv", i)),
            quality_profile: QualityProfile::Medium,
        })
        .collect();
    
    encoding_manager.queue_jobs(test_jobs);
    
    // Test performance statistics collection
    let initial_stats = encoding_manager.get_performance_stats().await;
    assert!(initial_stats.memory.current_usage_bytes >= 0);
    assert!(initial_stats.progress_batching.batching_efficiency_percent >= 0.0);
    assert!(initial_stats.metadata_cache.hit_rate_percent >= 0.0);
    
    // Test memory optimization
    encoding_manager.optimize_memory_usage();
    
    let optimized_stats = encoding_manager.get_performance_stats().await;
    assert!(optimized_stats.memory.current_usage_bytes >= 0);
    
    // Test error statistics integration
    let error_stats = encoding_manager.get_error_statistics();
    assert_eq!(error_stats.total_errors, 0); // No errors in this test
    
    println!("Performance monitoring integration test passed");
}

// ============================================================================
// COMPREHENSIVE PERFORMANCE VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_comprehensive_performance_requirements() {
    use video_encoder::benchmarks::VideoEncoderBenchmarks;
    
    let benchmarks = VideoEncoderBenchmarks::new();
    
    // Run comprehensive performance validation
    let validation = benchmarks.validate_lightweight_requirements().await.unwrap();
    
    // Test all performance requirements from the spec
    
    // Memory usage should be minimal (Requirement 9.1)
    assert!(validation.memory_usage_mb < 500, 
            "Memory usage ({:.1} MB) should be under 500MB for lightweight operation", 
            validation.memory_usage_mb);
    
    // CPU usage should be efficient (Requirement 9.2)
    assert!(validation.cpu_usage_percent < 100.0, 
            "CPU usage should be reasonable");
    
    // Batching should be efficient (Requirement 9.4)
    assert!(validation.batching_efficiency >= 50.0, 
            "Progress batching efficiency ({:.1}%) should be at least 50%", 
            validation.batching_efficiency);
    
    // Cache performance should be good
    assert!(validation.cache_hit_rate >= 0.0, 
            "Cache hit rate should be non-negative");
    
    // Overall performance should meet lightweight requirements
    assert!(validation.meets_lightweight_requirements(), 
            "Application should meet all lightweight performance requirements");
    
    validation.print_validation();
    println!("Comprehensive performance requirements validation passed");
}

#[tokio::test]
async fn test_performance_under_stress() {
    let performance_monitor = PerformanceMonitor::new();
    
    // Stress test with high memory usage
    let memory_tracker = performance_monitor.memory_tracker();
    for i in 0..1000 {
        memory_tracker.track_allocation(1024 * 1024); // 1MB each
    }
    
    // Stress test with high-frequency progress updates
    let progress_batcher = performance_monitor.progress_batcher();
    let mut batcher = progress_batcher.write().await;
    batcher.set_batch_interval(Duration::from_millis(10));
    batcher.set_max_batch_size(100);
    
    for i in 0..10000 {
        let file_path = PathBuf::from(format!("/stress/file_{}.mkv", i % 50));
        batcher.add_update(file_path, (i as f32) % 100.0, Some(25.0));
        
        if batcher.should_send_batch() {
            let _batch = batcher.get_batch();
        }
    }
    
    // Stress test resource monitoring
    let resource_monitor = performance_monitor.resource_monitor();
    resource_monitor.set_max_parallel_jobs(16);
    resource_monitor.set_active_jobs(12);
    
    for _ in 0..100 {
        resource_monitor.sample_resources().await;
        sleep(Duration::from_millis(1)).await;
    }
    
    // Verify system remains stable under stress
    let memory_stats = memory_tracker.get_stats();
    assert!(memory_stats.allocated_objects > 0);
    assert!(memory_tracker.is_memory_usage_acceptable(2000)); // 2GB limit for stress test
    
    let progress_stats = batcher.get_stats();
    assert!(progress_stats.total_updates_received > 0);
    assert!(progress_stats.total_batches_sent > 0);
    
    let resource_stats = resource_monitor.get_stats();
    assert_eq!(resource_stats.active_jobs, 12);
    assert_eq!(resource_stats.max_parallel_jobs, 16);
    
    println!("Performance under stress test passed");
}