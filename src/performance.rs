use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use tokio::sync::RwLock;
// Performance monitoring utilities

/// Performance monitoring and optimization utilities
#[derive(Debug, Clone)]
pub struct PerformanceMonitor {
    memory_tracker: Arc<MemoryTracker>,
    progress_batcher: Arc<RwLock<ProgressBatcher>>,
    resource_monitor: Arc<ResourceMonitor>,
    metadata_cache: Arc<RwLock<MetadataCache>>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            memory_tracker: Arc::new(MemoryTracker::new()),
            progress_batcher: Arc::new(RwLock::new(ProgressBatcher::new())),
            resource_monitor: Arc::new(ResourceMonitor::new()),
            metadata_cache: Arc::new(RwLock::new(MetadataCache::new())),
        }
    }
    
    /// Get memory tracker for monitoring memory usage
    pub fn memory_tracker(&self) -> Arc<MemoryTracker> {
        self.memory_tracker.clone()
    }
    
    /// Get progress batcher for efficient TUI updates
    pub fn progress_batcher(&self) -> Arc<RwLock<ProgressBatcher>> {
        self.progress_batcher.clone()
    }
    
    /// Get resource monitor for system resource tracking
    pub fn resource_monitor(&self) -> Arc<ResourceMonitor> {
        self.resource_monitor.clone()
    }
    
    /// Get metadata cache for file metadata caching
    pub fn metadata_cache(&self) -> Arc<RwLock<MetadataCache>> {
        self.metadata_cache.clone()
    }
    
    /// Get comprehensive performance statistics
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        let memory_stats = self.memory_tracker.get_stats();
        let progress_stats = self.progress_batcher.read().await.get_stats();
        let resource_stats = self.resource_monitor.get_stats();
        let cache_stats = self.metadata_cache.read().await.get_stats();
        
        PerformanceStats {
            memory: memory_stats,
            progress_batching: progress_stats,
            resource_usage: resource_stats,
            metadata_cache: cache_stats,
        }
    }
}

/// Memory usage tracking and optimization
#[derive(Debug)]
pub struct MemoryTracker {
    allocated_objects: AtomicUsize,
    peak_memory_usage: AtomicU64,
    current_memory_usage: AtomicU64,
    file_data_size: AtomicU64,
    progress_data_size: AtomicU64,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self {
            allocated_objects: AtomicUsize::new(0),
            peak_memory_usage: AtomicU64::new(0),
            current_memory_usage: AtomicU64::new(0),
            file_data_size: AtomicU64::new(0),
            progress_data_size: AtomicU64::new(0),
        }
    }
    
    /// Track allocation of a new object
    pub fn track_allocation(&self, size_bytes: u64) {
        self.allocated_objects.fetch_add(1, Ordering::Relaxed);
        let new_usage = self.current_memory_usage.fetch_add(size_bytes, Ordering::Relaxed) + size_bytes;
        
        // Update peak usage if necessary
        let mut peak = self.peak_memory_usage.load(Ordering::Relaxed);
        while new_usage > peak {
            match self.peak_memory_usage.compare_exchange_weak(
                peak, new_usage, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }
    }
    
    /// Track deallocation of an object
    pub fn track_deallocation(&self, size_bytes: u64) {
        self.allocated_objects.fetch_sub(1, Ordering::Relaxed);
        self.current_memory_usage.fetch_sub(size_bytes, Ordering::Relaxed);
    }
    
    /// Track file data memory usage
    pub fn track_file_data(&self, size_bytes: u64) {
        self.file_data_size.store(size_bytes, Ordering::Relaxed);
    }
    
    /// Track progress data memory usage
    pub fn track_progress_data(&self, size_bytes: u64) {
        self.progress_data_size.store(size_bytes, Ordering::Relaxed);
    }
    
    /// Get current memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            allocated_objects: self.allocated_objects.load(Ordering::Relaxed),
            current_usage_bytes: self.current_memory_usage.load(Ordering::Relaxed),
            peak_usage_bytes: self.peak_memory_usage.load(Ordering::Relaxed),
            file_data_bytes: self.file_data_size.load(Ordering::Relaxed),
            progress_data_bytes: self.progress_data_size.load(Ordering::Relaxed),
        }
    }
    
    /// Check if memory usage is within acceptable limits
    pub fn is_memory_usage_acceptable(&self, max_memory_mb: u64) -> bool {
        let current_mb = self.current_memory_usage.load(Ordering::Relaxed) / (1024 * 1024);
        current_mb <= max_memory_mb
    }
}

/// Progress update batching to minimize TUI refresh overhead
#[derive(Debug)]
pub struct ProgressBatcher {
    pending_updates: HashMap<PathBuf, BatchedProgressUpdate>,
    last_batch_time: Instant,
    batch_interval: Duration,
    max_batch_size: usize,
    total_updates_received: usize,
    total_batches_sent: usize,
}

impl ProgressBatcher {
    pub fn new() -> Self {
        Self {
            pending_updates: HashMap::new(),
            last_batch_time: Instant::now(),
            batch_interval: Duration::from_millis(100), // 10 FPS max
            max_batch_size: 50,
            total_updates_received: 0,
            total_batches_sent: 0,
        }
    }
    
    /// Add a progress update to the batch
    pub fn add_update(&mut self, file_path: PathBuf, progress_percent: f32, fps: Option<f32>) {
        self.total_updates_received += 1;
        
        let update = BatchedProgressUpdate {
            progress_percent,
            fps,
            last_updated: Instant::now(),
        };
        
        self.pending_updates.insert(file_path, update);
    }
    
    /// Check if a batch should be sent based on time or size limits
    pub fn should_send_batch(&self) -> bool {
        let time_elapsed = self.last_batch_time.elapsed() >= self.batch_interval;
        let size_limit_reached = self.pending_updates.len() >= self.max_batch_size;
        
        !self.pending_updates.is_empty() && (time_elapsed || size_limit_reached)
    }
    
    /// Get and clear pending updates for batching
    pub fn get_batch(&mut self) -> HashMap<PathBuf, BatchedProgressUpdate> {
        self.total_batches_sent += 1;
        self.last_batch_time = Instant::now();
        std::mem::take(&mut self.pending_updates)
    }
    
    /// Get batching statistics
    pub fn get_stats(&self) -> ProgressBatchingStats {
        let efficiency = if self.total_updates_received > 0 {
            (self.total_batches_sent as f32 / self.total_updates_received as f32) * 100.0
        } else {
            0.0
        };
        
        ProgressBatchingStats {
            pending_updates: self.pending_updates.len(),
            total_updates_received: self.total_updates_received,
            total_batches_sent: self.total_batches_sent,
            batching_efficiency_percent: efficiency,
            batch_interval_ms: self.batch_interval.as_millis() as u64,
        }
    }
    
    /// Configure batch interval for different performance requirements
    pub fn set_batch_interval(&mut self, interval: Duration) {
        self.batch_interval = interval;
    }
    
    /// Configure maximum batch size
    pub fn set_max_batch_size(&mut self, size: usize) {
        self.max_batch_size = size;
    }
}

#[derive(Debug, Clone)]
pub struct BatchedProgressUpdate {
    pub progress_percent: f32,
    pub fps: Option<f32>,
    pub last_updated: Instant,
}

/// System resource monitoring to prevent overload
#[derive(Debug)]
pub struct ResourceMonitor {
    cpu_usage_samples: RwLock<Vec<f32>>,
    memory_usage_samples: RwLock<Vec<u64>>,
    active_encoding_jobs: AtomicUsize,
    max_parallel_jobs: AtomicUsize,
    last_sample_time: RwLock<Instant>,
    sample_interval: Duration,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        Self {
            cpu_usage_samples: RwLock::new(Vec::with_capacity(60)), // 1 minute of samples
            memory_usage_samples: RwLock::new(Vec::with_capacity(60)),
            active_encoding_jobs: AtomicUsize::new(0),
            max_parallel_jobs: AtomicUsize::new(2),
            last_sample_time: RwLock::new(Instant::now()),
            sample_interval: Duration::from_secs(1),
        }
    }
    
    /// Update active job count
    pub fn set_active_jobs(&self, count: usize) {
        self.active_encoding_jobs.store(count, Ordering::Relaxed);
    }
    
    /// Set maximum parallel jobs limit
    pub fn set_max_parallel_jobs(&self, max: usize) {
        self.max_parallel_jobs.store(max, Ordering::Relaxed);
    }
    
    /// Sample current system resources (simplified implementation)
    pub async fn sample_resources(&self) {
        let mut last_sample = self.last_sample_time.write().await;
        if last_sample.elapsed() < self.sample_interval {
            return;
        }
        
        // Simplified resource sampling - in a real implementation,
        // this would use system APIs to get actual CPU and memory usage
        let estimated_cpu_usage = self.estimate_cpu_usage();
        let estimated_memory_usage = self.estimate_memory_usage();
        
        let mut cpu_samples = self.cpu_usage_samples.write().await;
        let mut memory_samples = self.memory_usage_samples.write().await;
        
        cpu_samples.push(estimated_cpu_usage);
        memory_samples.push(estimated_memory_usage);
        
        // Keep only recent samples
        if cpu_samples.len() > 60 {
            cpu_samples.remove(0);
        }
        if memory_samples.len() > 60 {
            memory_samples.remove(0);
        }
        
        *last_sample = Instant::now();
    }
    
    /// Estimate CPU usage based on active encoding jobs
    fn estimate_cpu_usage(&self) -> f32 {
        let active_jobs = self.active_encoding_jobs.load(Ordering::Relaxed) as f32;
        let max_jobs = self.max_parallel_jobs.load(Ordering::Relaxed) as f32;
        
        if max_jobs > 0.0 {
            (active_jobs / max_jobs) * 80.0 // Assume each job uses up to 80% CPU when at max capacity
        } else {
            0.0
        }
    }
    
    /// Estimate memory usage based on active jobs and data structures
    fn estimate_memory_usage(&self) -> u64 {
        let active_jobs = self.active_encoding_jobs.load(Ordering::Relaxed) as u64;
        let base_memory = 50 * 1024 * 1024; // 50MB base usage
        let per_job_memory = 100 * 1024 * 1024; // 100MB per active encoding job
        
        base_memory + (active_jobs * per_job_memory)
    }
    
    /// Check if system can handle additional encoding jobs
    pub async fn can_start_additional_job(&self) -> bool {
        let active_jobs = self.active_encoding_jobs.load(Ordering::Relaxed);
        let max_jobs = self.max_parallel_jobs.load(Ordering::Relaxed);
        
        if active_jobs >= max_jobs {
            return false;
        }
        
        // Check recent resource usage
        let cpu_samples = self.cpu_usage_samples.read().await;
        let memory_samples = self.memory_usage_samples.read().await;
        
        // Don't start new jobs if CPU usage is consistently high
        if let Some(recent_cpu) = cpu_samples.last() {
            if *recent_cpu > 90.0 {
                return false;
            }
        }
        
        // Don't start new jobs if memory usage is too high
        if let Some(recent_memory) = memory_samples.last() {
            let memory_gb = *recent_memory / (1024 * 1024 * 1024);
            if memory_gb > 8 { // Don't exceed 8GB total usage
                return false;
            }
        }
        
        true
    }
    
    /// Get resource monitoring statistics
    pub fn get_stats(&self) -> ResourceStats {
        ResourceStats {
            active_jobs: self.active_encoding_jobs.load(Ordering::Relaxed),
            max_parallel_jobs: self.max_parallel_jobs.load(Ordering::Relaxed),
            estimated_cpu_usage: self.estimate_cpu_usage(),
            estimated_memory_usage_bytes: self.estimate_memory_usage(),
        }
    }
}

/// File metadata caching to reduce filesystem operations
#[derive(Debug)]
pub struct MetadataCache {
    cache: HashMap<PathBuf, CachedMetadata>,
    cache_hits: usize,
    cache_misses: usize,
    max_cache_size: usize,
}

impl MetadataCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            cache_hits: 0,
            cache_misses: 0,
            max_cache_size: 1000, // Cache up to 1000 file metadata entries
        }
    }
    
    /// Get cached metadata or None if not cached
    pub fn get(&mut self, path: &PathBuf) -> Option<&CachedMetadata> {
        if let Some(metadata) = self.cache.get(path) {
            self.cache_hits += 1;
            Some(metadata)
        } else {
            self.cache_misses += 1;
            None
        }
    }
    
    /// Store metadata in cache
    pub fn insert(&mut self, path: PathBuf, metadata: CachedMetadata) {
        // Implement LRU eviction if cache is full
        if self.cache.len() >= self.max_cache_size {
            self.evict_oldest();
        }
        
        self.cache.insert(path, metadata);
    }
    
    /// Evict oldest entry (simplified LRU)
    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self.cache.keys().next().cloned() {
            self.cache.remove(&oldest_key);
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    
    /// Get cache statistics
    pub fn get_stats(&self) -> MetadataCacheStats {
        let hit_rate = if self.cache_hits + self.cache_misses > 0 {
            (self.cache_hits as f32 / (self.cache_hits + self.cache_misses) as f32) * 100.0
        } else {
            0.0
        };
        
        MetadataCacheStats {
            cache_size: self.cache.len(),
            max_cache_size: self.max_cache_size,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            hit_rate_percent: hit_rate,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedMetadata {
    pub size: u64,
    pub codec: crate::scanner::VideoCodec,
    pub resolution: Option<(u32, u32)>,
    pub duration: Option<Duration>,
    pub cached_at: Instant,
}

/// Comprehensive performance statistics
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub memory: MemoryStats,
    pub progress_batching: ProgressBatchingStats,
    pub resource_usage: ResourceStats,
    pub metadata_cache: MetadataCacheStats,
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub allocated_objects: usize,
    pub current_usage_bytes: u64,
    pub peak_usage_bytes: u64,
    pub file_data_bytes: u64,
    pub progress_data_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ProgressBatchingStats {
    pub pending_updates: usize,
    pub total_updates_received: usize,
    pub total_batches_sent: usize,
    pub batching_efficiency_percent: f32,
    pub batch_interval_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub active_jobs: usize,
    pub max_parallel_jobs: usize,
    pub estimated_cpu_usage: f32,
    pub estimated_memory_usage_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct MetadataCacheStats {
    pub cache_size: usize,
    pub max_cache_size: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub hit_rate_percent: f32,
}

/// Performance benchmarking utilities
pub struct PerformanceBenchmark {
    start_time: Instant,
    checkpoints: Vec<(String, Instant)>,
}

impl PerformanceBenchmark {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            checkpoints: Vec::new(),
        }
    }
    
    /// Add a checkpoint with a label
    pub fn checkpoint(&mut self, label: &str) {
        self.checkpoints.push((label.to_string(), Instant::now()));
    }
    
    /// Get benchmark results
    pub fn results(&self) -> BenchmarkResults {
        let total_duration = self.start_time.elapsed();
        let mut checkpoint_durations = Vec::new();
        
        let mut last_time = self.start_time;
        for (label, time) in &self.checkpoints {
            let duration = time.duration_since(last_time);
            checkpoint_durations.push((label.clone(), duration));
            last_time = *time;
        }
        
        BenchmarkResults {
            total_duration,
            checkpoint_durations,
        }
    }
}

#[derive(Debug)]
pub struct BenchmarkResults {
    pub total_duration: Duration,
    pub checkpoint_durations: Vec<(String, Duration)>,
}

impl BenchmarkResults {
    /// Print benchmark results in a readable format
    pub fn print_results(&self) {
        println!("Performance Benchmark Results:");
        println!("Total Duration: {:?}", self.total_duration);
        println!("Checkpoints:");
        for (label, duration) in &self.checkpoint_durations {
            println!("  {}: {:?}", label, duration);
        }
    }
}