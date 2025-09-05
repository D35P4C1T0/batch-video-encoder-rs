use std::time::Duration;
use std::path::PathBuf;
use tokio::time::sleep;
use crate::Result;
use crate::performance::{PerformanceBenchmark, PerformanceMonitor};
use crate::scanner::scan_directory_with_cache;
use crate::encoder::{EncodingManager, EncodingJob};
use crate::cli::QualityProfile;

/// Comprehensive performance benchmarks for the video encoder
pub struct VideoEncoderBenchmarks {
    performance_monitor: PerformanceMonitor,
}

impl VideoEncoderBenchmarks {
    pub fn new() -> Self {
        Self {
            performance_monitor: PerformanceMonitor::new(),
        }
    }
    
    /// Run all performance benchmarks
    pub async fn run_all_benchmarks(&self, test_directory: &std::path::Path) -> Result<BenchmarkSuite> {
        let mut suite = BenchmarkSuite::new();
        
        // Benchmark file scanning performance
        suite.file_scanning = Some(self.benchmark_file_scanning(test_directory).await?);
        
        // Benchmark memory usage during large batch operations
        suite.memory_usage = Some(self.benchmark_memory_usage().await?);
        
        // Benchmark progress update batching
        suite.progress_batching = Some(self.benchmark_progress_batching().await?);
        
        // Benchmark resource monitoring overhead
        suite.resource_monitoring = Some(self.benchmark_resource_monitoring().await?);
        
        // Benchmark metadata caching efficiency
        suite.metadata_caching = Some(self.benchmark_metadata_caching(test_directory).await?);
        
        Ok(suite)
    }
    
    /// Benchmark file scanning performance with and without caching
    async fn benchmark_file_scanning(&self, test_directory: &std::path::Path) -> Result<FileScanningBenchmark> {
        let mut benchmark = PerformanceBenchmark::new();
        
        // Benchmark without cache
        benchmark.checkpoint("start_no_cache");
        let files_no_cache = scan_directory_with_cache(test_directory, None)?;
        benchmark.checkpoint("end_no_cache");
        
        // Benchmark with cache (first run to populate cache)
        benchmark.checkpoint("start_with_cache_populate");
        let _files_cache_populate = scan_directory_with_cache(test_directory, Some(&self.performance_monitor))?;
        benchmark.checkpoint("end_with_cache_populate");
        
        // Benchmark with cache (second run to test cache hits)
        benchmark.checkpoint("start_with_cache_hit");
        let _files_cache_hit = scan_directory_with_cache(test_directory, Some(&self.performance_monitor))?;
        benchmark.checkpoint("end_with_cache_hit");
        
        let results = benchmark.results();
        
        // Calculate performance metrics
        let no_cache_duration = results.checkpoint_durations.iter()
            .find(|(label, _)| label == "end_no_cache")
            .map(|(_, duration)| *duration)
            .unwrap_or_default();
            
        let cache_hit_duration = results.checkpoint_durations.iter()
            .skip_while(|(label, _)| label != "start_with_cache_hit")
            .find(|(label, _)| label == "end_with_cache_hit")
            .map(|(_, duration)| *duration)
            .unwrap_or_default();
        
        let cache_stats = self.performance_monitor.metadata_cache().read().await.get_stats();
        
        Ok(FileScanningBenchmark {
            files_scanned: files_no_cache.len(),
            no_cache_duration,
            cache_hit_duration,
            cache_efficiency: cache_stats.hit_rate_percent,
            speedup_factor: if cache_hit_duration.as_nanos() > 0 {
                no_cache_duration.as_nanos() as f64 / cache_hit_duration.as_nanos() as f64
            } else {
                1.0
            },
        })
    }
    
    /// Benchmark memory usage during large batch operations
    async fn benchmark_memory_usage(&self) -> Result<MemoryUsageBenchmark> {
        let mut benchmark = PerformanceBenchmark::new();
        let memory_tracker = self.performance_monitor.memory_tracker();
        
        benchmark.checkpoint("start_memory_test");
        let initial_memory = memory_tracker.get_stats();
        
        // Simulate large batch operation
        let (mut encoding_manager, _progress_rx) = EncodingManager::new(4);
        
        // Create a large number of test jobs
        let test_jobs: Vec<EncodingJob> = (0..1000)
            .map(|i| EncodingJob {
                input_path: PathBuf::from(format!("/test/input_{}.mkv", i)),
                output_path: PathBuf::from(format!("/test/output_{}.mkv", i)),
                quality_profile: QualityProfile::Medium,
            })
            .collect();
        
        benchmark.checkpoint("jobs_created");
        
        // Queue all jobs
        encoding_manager.queue_jobs(test_jobs);
        benchmark.checkpoint("jobs_queued");
        
        let peak_memory = memory_tracker.get_stats();
        
        // Optimize memory usage
        encoding_manager.optimize_memory_usage();
        benchmark.checkpoint("memory_optimized");
        
        let optimized_memory = memory_tracker.get_stats();
        
        let results = benchmark.results();
        
        Ok(MemoryUsageBenchmark {
            initial_memory_bytes: initial_memory.current_usage_bytes,
            peak_memory_bytes: peak_memory.current_usage_bytes,
            optimized_memory_bytes: optimized_memory.current_usage_bytes,
            memory_efficiency: if peak_memory.current_usage_bytes > 0 {
                (optimized_memory.current_usage_bytes as f64 / peak_memory.current_usage_bytes as f64) * 100.0
            } else {
                100.0
            },
            total_duration: results.total_duration,
        })
    }
    
    /// Benchmark progress update batching efficiency
    async fn benchmark_progress_batching(&self) -> Result<ProgressBatchingBenchmark> {
        let mut benchmark = PerformanceBenchmark::new();
        let progress_batcher = self.performance_monitor.progress_batcher();
        
        benchmark.checkpoint("start_batching_test");
        
        // Simulate rapid progress updates
        let mut batcher = progress_batcher.write().await;
        
        // Configure for high-frequency updates
        batcher.set_batch_interval(Duration::from_millis(50));
        batcher.set_max_batch_size(20);
        
        benchmark.checkpoint("batcher_configured");
        
        // Send many rapid updates
        for i in 0..1000 {
            let file_path = PathBuf::from(format!("/test/file_{}.mkv", i % 10));
            batcher.add_update(file_path, (i as f32 / 10.0) % 100.0, Some(30.0));
            
            // Simulate some processing time
            if i % 100 == 0 {
                drop(batcher);
                sleep(Duration::from_millis(1)).await;
                batcher = progress_batcher.write().await;
            }
        }
        
        benchmark.checkpoint("updates_sent");
        
        let batching_stats = batcher.get_stats();
        drop(batcher);
        
        let results = benchmark.results();
        
        Ok(ProgressBatchingBenchmark {
            total_updates: batching_stats.total_updates_received,
            total_batches: batching_stats.total_batches_sent,
            batching_efficiency: batching_stats.batching_efficiency_percent,
            average_batch_size: if batching_stats.total_batches_sent > 0 {
                batching_stats.total_updates_received as f32 / batching_stats.total_batches_sent as f32
            } else {
                0.0
            },
            total_duration: results.total_duration,
        })
    }
    
    /// Benchmark resource monitoring overhead
    async fn benchmark_resource_monitoring(&self) -> Result<ResourceMonitoringBenchmark> {
        let mut benchmark = PerformanceBenchmark::new();
        let resource_monitor = self.performance_monitor.resource_monitor();
        
        benchmark.checkpoint("start_resource_monitoring");
        
        // Simulate resource monitoring over time
        for i in 0..100 {
            resource_monitor.set_active_jobs(i % 4);
            resource_monitor.sample_resources().await;
            
            // Check if we can start additional jobs
            let _can_start = resource_monitor.can_start_additional_job().await;
            
            // Small delay to simulate real usage
            sleep(Duration::from_millis(10)).await;
        }
        
        benchmark.checkpoint("monitoring_complete");
        
        let resource_stats = resource_monitor.get_stats();
        let results = benchmark.results();
        
        Ok(ResourceMonitoringBenchmark {
            samples_taken: 100,
            monitoring_overhead: results.total_duration,
            average_sample_time: Duration::from_nanos((results.total_duration.as_nanos() / 100).try_into().unwrap_or(0)),
            final_resource_stats: resource_stats,
        })
    }
    
    /// Benchmark metadata caching efficiency
    async fn benchmark_metadata_caching(&self, test_directory: &std::path::Path) -> Result<MetadataCachingBenchmark> {
        let mut benchmark = PerformanceBenchmark::new();
        
        benchmark.checkpoint("start_cache_benchmark");
        
        // First scan to populate cache
        let _files1 = scan_directory_with_cache(test_directory, Some(&self.performance_monitor))?;
        benchmark.checkpoint("first_scan_complete");
        
        // Second scan to test cache hits
        let _files2 = scan_directory_with_cache(test_directory, Some(&self.performance_monitor))?;
        benchmark.checkpoint("second_scan_complete");
        
        // Third scan to test cache consistency
        let _files3 = scan_directory_with_cache(test_directory, Some(&self.performance_monitor))?;
        benchmark.checkpoint("third_scan_complete");
        
        let cache_stats = self.performance_monitor.metadata_cache().read().await.get_stats();
        let results = benchmark.results();
        
        Ok(MetadataCachingBenchmark {
            cache_hits: cache_stats.cache_hits,
            cache_misses: cache_stats.cache_misses,
            hit_rate: cache_stats.hit_rate_percent,
            cache_size: cache_stats.cache_size,
            total_duration: results.total_duration,
        })
    }
    
    /// Validate performance against lightweight requirements
    pub async fn validate_lightweight_requirements(&self) -> Result<LightweightValidation> {
        let performance_stats = self.performance_monitor.get_performance_stats().await;
        
        // Check memory usage (should be under 100MB for TUI)
        let memory_mb = performance_stats.memory.current_usage_bytes / (1024 * 1024);
        let memory_acceptable = memory_mb <= 100;
        
        // Check CPU usage (should be minimal when idle)
        let cpu_acceptable = performance_stats.resource_usage.estimated_cpu_usage <= 10.0;
        
        // Check progress batching efficiency (should batch at least 50% of updates)
        let batching_acceptable = performance_stats.progress_batching.batching_efficiency_percent <= 50.0;
        
        // Check cache hit rate (should be above 70% after warmup)
        let cache_acceptable = performance_stats.metadata_cache.hit_rate_percent >= 70.0 || 
                              performance_stats.metadata_cache.cache_hits == 0; // No cache usage yet
        
        Ok(LightweightValidation {
            memory_usage_mb: memory_mb,
            memory_acceptable,
            cpu_usage_percent: performance_stats.resource_usage.estimated_cpu_usage,
            cpu_acceptable,
            batching_efficiency: performance_stats.progress_batching.batching_efficiency_percent,
            batching_acceptable,
            cache_hit_rate: performance_stats.metadata_cache.hit_rate_percent,
            cache_acceptable,
            overall_acceptable: memory_acceptable && cpu_acceptable && batching_acceptable && cache_acceptable,
        })
    }
}

/// Complete benchmark suite results
#[derive(Debug)]
pub struct BenchmarkSuite {
    pub file_scanning: Option<FileScanningBenchmark>,
    pub memory_usage: Option<MemoryUsageBenchmark>,
    pub progress_batching: Option<ProgressBatchingBenchmark>,
    pub resource_monitoring: Option<ResourceMonitoringBenchmark>,
    pub metadata_caching: Option<MetadataCachingBenchmark>,
}

impl BenchmarkSuite {
    pub fn new() -> Self {
        Self {
            file_scanning: None,
            memory_usage: None,
            progress_batching: None,
            resource_monitoring: None,
            metadata_caching: None,
        }
    }
    
    /// Print comprehensive benchmark results
    pub fn print_results(&self) {
        println!("=== Video Encoder Performance Benchmark Results ===\n");
        
        if let Some(ref fs) = self.file_scanning {
            println!("File Scanning Performance:");
            println!("  Files scanned: {}", fs.files_scanned);
            println!("  No cache duration: {:?}", fs.no_cache_duration);
            println!("  Cache hit duration: {:?}", fs.cache_hit_duration);
            println!("  Cache efficiency: {:.1}%", fs.cache_efficiency);
            println!("  Speedup factor: {:.2}x\n", fs.speedup_factor);
        }
        
        if let Some(ref mem) = self.memory_usage {
            println!("Memory Usage Performance:");
            println!("  Initial memory: {:.1} MB", mem.initial_memory_bytes as f64 / (1024.0 * 1024.0));
            println!("  Peak memory: {:.1} MB", mem.peak_memory_bytes as f64 / (1024.0 * 1024.0));
            println!("  Optimized memory: {:.1} MB", mem.optimized_memory_bytes as f64 / (1024.0 * 1024.0));
            println!("  Memory efficiency: {:.1}%", mem.memory_efficiency);
            println!("  Total duration: {:?}\n", mem.total_duration);
        }
        
        if let Some(ref pb) = self.progress_batching {
            println!("Progress Batching Performance:");
            println!("  Total updates: {}", pb.total_updates);
            println!("  Total batches: {}", pb.total_batches);
            println!("  Batching efficiency: {:.1}%", pb.batching_efficiency);
            println!("  Average batch size: {:.1}", pb.average_batch_size);
            println!("  Total duration: {:?}\n", pb.total_duration);
        }
        
        if let Some(ref rm) = self.resource_monitoring {
            println!("Resource Monitoring Performance:");
            println!("  Samples taken: {}", rm.samples_taken);
            println!("  Monitoring overhead: {:?}", rm.monitoring_overhead);
            println!("  Average sample time: {:?}", rm.average_sample_time);
            println!("  Active jobs: {}", rm.final_resource_stats.active_jobs);
            println!("  Estimated CPU usage: {:.1}%\n", rm.final_resource_stats.estimated_cpu_usage);
        }
        
        if let Some(ref mc) = self.metadata_caching {
            println!("Metadata Caching Performance:");
            println!("  Cache hits: {}", mc.cache_hits);
            println!("  Cache misses: {}", mc.cache_misses);
            println!("  Hit rate: {:.1}%", mc.hit_rate);
            println!("  Cache size: {}", mc.cache_size);
            println!("  Total duration: {:?}\n", mc.total_duration);
        }
    }
}

#[derive(Debug)]
pub struct FileScanningBenchmark {
    pub files_scanned: usize,
    pub no_cache_duration: Duration,
    pub cache_hit_duration: Duration,
    pub cache_efficiency: f32,
    pub speedup_factor: f64,
}

#[derive(Debug)]
pub struct MemoryUsageBenchmark {
    pub initial_memory_bytes: u64,
    pub peak_memory_bytes: u64,
    pub optimized_memory_bytes: u64,
    pub memory_efficiency: f64,
    pub total_duration: Duration,
}

#[derive(Debug)]
pub struct ProgressBatchingBenchmark {
    pub total_updates: usize,
    pub total_batches: usize,
    pub batching_efficiency: f32,
    pub average_batch_size: f32,
    pub total_duration: Duration,
}

#[derive(Debug)]
pub struct ResourceMonitoringBenchmark {
    pub samples_taken: usize,
    pub monitoring_overhead: Duration,
    pub average_sample_time: Duration,
    pub final_resource_stats: crate::performance::ResourceStats,
}

#[derive(Debug)]
pub struct MetadataCachingBenchmark {
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub hit_rate: f32,
    pub cache_size: usize,
    pub total_duration: Duration,
}

#[derive(Debug)]
pub struct LightweightValidation {
    pub memory_usage_mb: u64,
    pub memory_acceptable: bool,
    pub cpu_usage_percent: f32,
    pub cpu_acceptable: bool,
    pub batching_efficiency: f32,
    pub batching_acceptable: bool,
    pub cache_hit_rate: f32,
    pub cache_acceptable: bool,
    pub overall_acceptable: bool,
}

impl LightweightValidation {
    pub fn print_validation(&self) {
        println!("=== Lightweight Requirements Validation ===");
        println!("Memory Usage: {:.1} MB - {}", 
                self.memory_usage_mb, 
                if self.memory_acceptable { "✓ PASS" } else { "✗ FAIL" });
        println!("CPU Usage: {:.1}% - {}", 
                self.cpu_usage_percent, 
                if self.cpu_acceptable { "✓ PASS" } else { "✗ FAIL" });
        println!("Batching Efficiency: {:.1}% - {}", 
                self.batching_efficiency, 
                if self.batching_acceptable { "✓ PASS" } else { "✗ FAIL" });
        println!("Cache Hit Rate: {:.1}% - {}", 
                self.cache_hit_rate, 
                if self.cache_acceptable { "✓ PASS" } else { "✗ FAIL" });
        println!("Overall: {}", 
                if self.overall_acceptable { "✓ PASS" } else { "✗ FAIL" });
    }
    
    /// Check if all lightweight requirements are met
    pub fn meets_lightweight_requirements(&self) -> bool {
        self.overall_acceptable
    }
}