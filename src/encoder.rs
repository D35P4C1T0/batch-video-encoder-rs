use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use crate::{AppError, Result};
use crate::cli::QualityProfile;

#[derive(Debug, Clone)]
pub struct EncodingJob {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub quality_profile: QualityProfile,
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub file_path: PathBuf,
    pub progress_percent: f32,
    pub estimated_time_remaining: Option<Duration>,
    pub current_fps: Option<f32>,
}

pub type EncodingResult = Result<PathBuf>;

pub struct EncodingManager {
    max_parallel: usize,
    active_jobs: HashMap<PathBuf, JoinHandle<EncodingResult>>,
    progress_tx: mpsc::Sender<ProgressUpdate>,
}

impl EncodingManager {
    pub fn new(max_parallel: usize) -> (Self, mpsc::Receiver<ProgressUpdate>) {
        let (progress_tx, progress_rx) = mpsc::channel(100);
        
        let manager = Self {
            max_parallel,
            active_jobs: HashMap::new(),
            progress_tx,
        };
        
        (manager, progress_rx)
    }
    
    pub fn can_start_job(&self) -> bool {
        self.active_jobs.len() < self.max_parallel
    }
    
    pub fn active_job_count(&self) -> usize {
        self.active_jobs.len()
    }
}