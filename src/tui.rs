use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Gauge},
    Frame,
};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyEventKind},
};
use tokio::sync::mpsc;
use crate::{AppError, Result, ErrorSeverity};
use crate::scanner::{VideoFile, VideoCodec};
use crate::encoder::{self, ProgressUpdate, EncodingSummary, JobStatus, JobState};
use crate::error_recovery::{ErrorNotification, ErrorStatistics};
use crate::performance::PerformanceMonitor;

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Clone)]
pub struct FileProgressInfo {
    pub progress_percent: f32,
    pub current_fps: Option<f32>,
    pub estimated_time_remaining: Option<Duration>,
    pub status: JobState,
    pub start_time: Option<Instant>,
}

impl Default for FileProgressInfo {
    fn default() -> Self {
        Self {
            progress_percent: 0.0,
            current_fps: None,
            estimated_time_remaining: None,
            status: JobState::Queued,
            start_time: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatchProgressInfo {
    pub total_files: usize,
    pub completed_files: usize,
    pub failed_files: usize,
    pub active_files: usize,
    pub overall_progress: f32,
    pub batch_start_time: Option<Instant>,
    pub estimated_completion_time: Option<Instant>,
}

impl Default for BatchProgressInfo {
    fn default() -> Self {
        Self {
            total_files: 0,
            completed_files: 0,
            failed_files: 0,
            active_files: 0,
            overall_progress: 0.0,
            batch_start_time: None,
            estimated_completion_time: None,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub files: Vec<VideoFile>,
    pub selected_files: HashSet<usize>,
    pub file_progress: HashMap<PathBuf, FileProgressInfo>,
    pub batch_progress: BatchProgressInfo,
    pub current_selection: usize,
    pub list_state: ListState,
    pub should_quit: bool,
    pub confirmed: bool,
    pub encoding_active: bool,
    pub last_update: Instant,
    pub error_notifications: Vec<ErrorNotification>,
    pub error_statistics: ErrorStatistics,
    pub show_error_details: bool,
}

impl AppState {
    pub fn new(files: Vec<VideoFile>) -> Self {
        let mut list_state = ListState::default();
        if !files.is_empty() {
            list_state.select(Some(0));
        }
        
        Self {
            files,
            selected_files: HashSet::new(),
            file_progress: HashMap::new(),
            batch_progress: BatchProgressInfo::default(),
            current_selection: 0,
            list_state,
            should_quit: false,
            confirmed: false,
            encoding_active: false,
            last_update: Instant::now(),
            error_notifications: Vec::new(),
            error_statistics: ErrorStatistics::default(),
            show_error_details: false,
        }
    }
    
    pub fn next(&mut self) {
        if self.files.is_empty() {
            return;
        }
        
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.files.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.current_selection = i;
    }
    
    pub fn previous(&mut self) {
        if self.files.is_empty() {
            return;
        }
        
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.files.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.current_selection = i;
    }
    
    pub fn toggle_selection(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if self.selected_files.contains(&i) {
                self.selected_files.remove(&i);
            } else {
                self.selected_files.insert(i);
            }
        }
    }
    
    pub fn select_all(&mut self) {
        for i in 0..self.files.len() {
            self.selected_files.insert(i);
        }
    }
    
    pub fn deselect_all(&mut self) {
        self.selected_files.clear();
    }
    
    pub fn get_selected_files(&self) -> Vec<&VideoFile> {
        self.selected_files
            .iter()
            .filter_map(|&i| self.files.get(i))
            .collect()
    }
    
    /// Initialize progress tracking for selected files
    pub fn initialize_progress_tracking(&mut self, selected_files: &[VideoFile]) {
        self.file_progress.clear();
        
        for file in selected_files {
            self.file_progress.insert(file.path.clone(), FileProgressInfo::default());
        }
        
        self.batch_progress = BatchProgressInfo {
            total_files: selected_files.len(),
            completed_files: 0,
            failed_files: 0,
            active_files: 0,
            overall_progress: 0.0,
            batch_start_time: Some(Instant::now()),
            estimated_completion_time: None,
        };
        
        self.encoding_active = true;
    }
    
    /// Update progress for a specific file
    pub fn update_file_progress(&mut self, file_path: &PathBuf, progress: ProgressUpdate) {
        if let Some(file_info) = self.file_progress.get_mut(file_path) {
            file_info.progress_percent = progress.progress_percent;
            file_info.current_fps = progress.current_fps;
            file_info.estimated_time_remaining = progress.estimated_time_remaining;
            
            // Set start time if this is the first progress update
            if file_info.start_time.is_none() && progress.progress_percent > 0.0 {
                file_info.start_time = Some(Instant::now());
            }
        }
        
        self.last_update = Instant::now();
        self.update_batch_progress();
    }
    
    /// Update job status for a specific file
    pub fn update_job_status(&mut self, file_path: &PathBuf, status: &JobStatus) {
        if let Some(file_info) = self.file_progress.get_mut(file_path) {
            let old_status = file_info.status.clone();
            file_info.status = status.status.clone();
            file_info.progress_percent = status.progress;
            
            // Update batch counters when status changes
            match (&old_status, &status.status) {
                (JobState::Queued, JobState::Running) => {
                    self.batch_progress.active_files += 1;
                    if file_info.start_time.is_none() {
                        file_info.start_time = Some(Instant::now());
                    }
                }
                (JobState::Running, JobState::Completed) => {
                    self.batch_progress.active_files = self.batch_progress.active_files.saturating_sub(1);
                    self.batch_progress.completed_files += 1;
                    file_info.progress_percent = 100.0;
                }
                (JobState::Running, JobState::Failed(_)) => {
                    self.batch_progress.active_files = self.batch_progress.active_files.saturating_sub(1);
                    self.batch_progress.failed_files += 1;
                }
                (JobState::Queued, JobState::Completed) => {
                    // Direct transition from queued to completed
                    self.batch_progress.completed_files += 1;
                    file_info.progress_percent = 100.0;
                }
                (JobState::Queued, JobState::Failed(_)) => {
                    // Direct transition from queued to failed
                    self.batch_progress.failed_files += 1;
                }
                _ => {}
            }
        }
        
        self.last_update = Instant::now();
        self.update_batch_progress();
    }
    
    /// Update overall batch progress based on individual file progress
    fn update_batch_progress(&mut self) {
        if self.file_progress.is_empty() {
            return;
        }
        
        let total_progress: f32 = self.file_progress.values()
            .map(|info| match info.status {
                JobState::Completed => 100.0,
                JobState::Failed(_) => 0.0,
                _ => info.progress_percent,
            })
            .sum();
            
        self.batch_progress.overall_progress = total_progress / self.file_progress.len() as f32;
        
        // Estimate completion time based on current progress rate
        if let Some(start_time) = self.batch_progress.batch_start_time {
            let elapsed = start_time.elapsed();
            if self.batch_progress.overall_progress > 0.0 {
                let estimated_total_time = elapsed.as_secs_f64() * (100.0 / self.batch_progress.overall_progress as f64);
                self.batch_progress.estimated_completion_time = Some(start_time + Duration::from_secs_f64(estimated_total_time));
            }
        }
    }
    
    /// Check if encoding is complete
    pub fn is_encoding_complete(&self) -> bool {
        if !self.encoding_active {
            return false;
        }
        
        let total_processed = self.batch_progress.completed_files + self.batch_progress.failed_files;
        total_processed >= self.batch_progress.total_files && self.batch_progress.total_files > 0
    }
    
    /// Add an error notification
    pub fn add_error_notification(&mut self, notification: ErrorNotification) {
        self.error_notifications.push(notification);
        // Keep only the last 10 notifications to prevent memory bloat
        if self.error_notifications.len() > 10 {
            self.error_notifications.remove(0);
        }
    }
    
    /// Update error statistics
    pub fn update_error_statistics(&mut self, statistics: ErrorStatistics) {
        self.error_statistics = statistics;
    }
    
    /// Toggle error details display
    pub fn toggle_error_details(&mut self) {
        self.show_error_details = !self.show_error_details;
    }
    
    /// Clear error notifications
    pub fn clear_error_notifications(&mut self) {
        self.error_notifications.clear();
    }
    
    /// Get recent error notifications
    pub fn get_recent_errors(&self) -> &[ErrorNotification] {
        &self.error_notifications
    }
    
    /// Get encoding statistics for display
    pub fn get_encoding_stats(&self) -> EncodingStats {
        // Count active files directly from file progress
        let active_files_count = self.file_progress.values()
            .filter(|info| matches!(info.status, JobState::Running))
            .count();
            
        let active_files: Vec<_> = self.file_progress.iter()
            .filter(|(_, info)| matches!(info.status, JobState::Running))
            .collect();
            
        let average_fps = if !active_files.is_empty() {
            let fps_sum: f32 = active_files.iter()
                .filter_map(|(_, info)| info.current_fps)
                .sum();
            let fps_count = active_files.iter()
                .filter(|(_, info)| info.current_fps.is_some())
                .count();
            if fps_count > 0 {
                Some(fps_sum / fps_count as f32)
            } else {
                None
            }
        } else {
            None
        };
        
        let estimated_time_remaining = if let Some(completion_time) = self.batch_progress.estimated_completion_time {
            let now = Instant::now();
            if completion_time > now {
                Some(completion_time.duration_since(now))
            } else {
                None
            }
        } else {
            None
        };
        
        EncodingStats {
            total_files: self.batch_progress.total_files,
            completed_files: self.batch_progress.completed_files,
            failed_files: self.batch_progress.failed_files,
            active_files: active_files_count,
            overall_progress: self.batch_progress.overall_progress,
            average_fps,
            estimated_time_remaining,
            elapsed_time: self.batch_progress.batch_start_time.map(|start| start.elapsed()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodingStats {
    pub total_files: usize,
    pub completed_files: usize,
    pub failed_files: usize,
    pub active_files: usize,
    pub overall_progress: f32,
    pub average_fps: Option<f32>,
    pub estimated_time_remaining: Option<Duration>,
    pub elapsed_time: Option<Duration>,
}

pub struct TuiManager {
    terminal: TerminalType,
    app_state: AppState,
    performance_monitor: PerformanceMonitor,
    last_render_time: Instant,
    render_interval: Duration,
}

impl TuiManager {
    pub fn new(files: Vec<VideoFile>) -> Result<Self> {
        enable_raw_mode().map_err(|e| AppError::TuiError(e.to_string()))?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|e| AppError::TuiError(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| AppError::TuiError(e.to_string()))?;
        
        let app_state = AppState::new(files);
        
        Ok(Self {
            terminal,
            app_state,
            performance_monitor: PerformanceMonitor::new(),
            last_render_time: Instant::now(),
            render_interval: Duration::from_millis(100), // 10 FPS max
        })
    }
    
    pub fn run_file_selection(&mut self) -> Result<Vec<VideoFile>> {
        if self.app_state.files.is_empty() {
            return Ok(Vec::new());
        }
        
        loop {
            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3), // Header
                        Constraint::Min(0),    // File list
                        Constraint::Length(4), // Instructions
                    ])
                    .split(f.size());
                
                render_header(f, chunks[0], &self.app_state);
                render_file_list(f, chunks[1], &mut self.app_state);
                render_instructions(f, chunks[2]);
            }).map_err(|e| AppError::TuiError(e.to_string()))?;
            
            if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            self.app_state.should_quit = true;
                            break;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.app_state.next();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.app_state.previous();
                        }
                        KeyCode::Char(' ') => {
                            self.app_state.toggle_selection();
                        }
                        KeyCode::Char('a') => {
                            self.app_state.select_all();
                        }
                        KeyCode::Char('d') => {
                            self.app_state.deselect_all();
                        }
                        KeyCode::Enter => {
                            self.app_state.confirmed = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        
        if self.app_state.should_quit {
            Ok(Vec::new())
        } else {
            Ok(self.app_state.get_selected_files().into_iter().cloned().collect())
        }
    }
    
    /// Show error confirmation dialog for critical errors
    pub fn show_error_confirmation(&mut self, error: &AppError) -> Result<bool> {
        loop {
            self.terminal.draw(|f| {
                render_error_confirmation_dialog(f, error);
            }).map_err(|e| AppError::TuiError(e.to_string()))?;
            
            if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            return Ok(true);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            return Ok(false);
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            return Ok(false);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    
    /// Show hardware fallback confirmation dialog
    pub fn show_hardware_fallback_confirmation(&mut self, reason: &str, fallback_method: &str) -> Result<bool> {
        loop {
            self.terminal.draw(|f| {
                render_hardware_fallback_dialog(f, reason, fallback_method);
            }).map_err(|e| AppError::TuiError(e.to_string()))?;
            
            if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            return Ok(true);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            return Ok(false);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    
    /// Show batch processing error confirmation
    pub fn show_batch_error_confirmation(&mut self, completed: usize, failed: usize, remaining: usize) -> Result<bool> {
        loop {
            self.terminal.draw(|f| {
                render_batch_error_dialog(f, completed, failed, remaining);
            }).map_err(|e| AppError::TuiError(e.to_string()))?;
            
            if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            return Ok(true);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            return Ok(false);
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            // Show detailed error information
                            self.app_state.show_error_details = true;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Run encoding progress display with real-time updates and error handling
    pub async fn run_encoding_progress(
        &mut self,
        selected_files: Vec<VideoFile>,
        mut progress_rx: mpsc::Receiver<ProgressUpdate>,
        mut encoding_summary_rx: mpsc::Receiver<EncodingSummary>,
        mut error_notification_rx: Option<mpsc::Receiver<ErrorNotification>>,
    ) -> Result<()> {
        // Initialize progress tracking
        self.app_state.initialize_progress_tracking(&selected_files);
        
        // Set up non-blocking event reading
        let mut last_render = Instant::now();
        let render_interval = Duration::from_millis(100); // 10 FPS
        
        loop {
            // Check for progress updates (non-blocking)
            while let Ok(progress) = progress_rx.try_recv() {
                let file_path = progress.file_path.clone();
                self.app_state.update_file_progress(&file_path, progress);
            }
            
            // Check for encoding summary updates (non-blocking)
            if let Ok(summary) = encoding_summary_rx.try_recv() {
                self.update_from_encoding_summary(summary);
            }
            
            // Check for error notifications (non-blocking)
            if let Some(ref mut error_rx) = error_notification_rx {
                loop {
                    match error_rx.try_recv() {
                        Ok(notification) => {
                            self.app_state.add_error_notification(notification);
                        }
                        Err(_) => break,
                    }
                }
            }
            
            // Render at controlled intervals
            if last_render.elapsed() >= render_interval {
                let should_render = {
                    // Create a scope to limit the borrow
                    true
                };
                
                if should_render {
                    self.terminal.draw(|f| {
                        render_encoding_progress_ui(f, &self.app_state);
                    }).map_err(|e| AppError::TuiError(e.to_string()))?;
                }
                
                last_render = Instant::now();
            }
            
            // Handle user input (non-blocking)
            if event::poll(Duration::from_millis(50)).map_err(|e| AppError::TuiError(e.to_string()))? {
                if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.app_state.should_quit = true;
                                break;
                            }
                            KeyCode::Char('e') => {
                                self.app_state.toggle_error_details();
                            }
                            KeyCode::Char('c') => {
                                self.app_state.clear_error_notifications();
                            }
                            _ => {}
                        }
                    }
                }
            }
            
            // Check if encoding is complete
            if self.app_state.is_encoding_complete() {
                // Final render to show completion
                self.terminal.draw(|f| {
                    render_encoding_progress_ui(f, &self.app_state);
                }).map_err(|e| AppError::TuiError(e.to_string()))?;
                
                // Wait for user acknowledgment
                loop {
                    if let Event::Key(key) = event::read().map_err(|e| AppError::TuiError(e.to_string()))? {
                        if key.kind == KeyEventKind::Press {
                            break;
                        }
                    }
                }
                break;
            }
            
            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        Ok(())
    }
    
    /// Update app state from encoding summary
    fn update_from_encoding_summary(&mut self, summary: EncodingSummary) {
        self.app_state.batch_progress.completed_files = summary.completed_jobs;
        self.app_state.batch_progress.failed_files = summary.failed_jobs;
        self.app_state.batch_progress.active_files = summary.active_jobs;
        self.app_state.batch_progress.overall_progress = summary.overall_progress;
    }
    
    /// Update job statuses from encoding manager
    pub fn update_job_statuses(&mut self, job_statuses: &HashMap<PathBuf, JobStatus>) {
        for (file_path, job_status) in job_statuses {
            self.app_state.update_job_status(file_path, job_status);
        }
    }
    
    /// Get a reference to the app state for external updates
    pub fn get_app_state_mut(&mut self) -> &mut AppState {
        &mut self.app_state
    }
    
    /// Initialize encoding mode for progress tracking
    pub fn initialize_encoding_mode(&mut self, selected_files: &[VideoFile]) -> Result<()> {
        self.app_state.initialize_progress_tracking(selected_files);
        Ok(())
    }
    
    /// Update progress for a specific file with batching
    pub async fn update_file_progress(&mut self, file_path: &std::path::PathBuf, progress: &encoder::ProgressUpdate) -> Result<()> {
        // Add to batch instead of immediate update
        let progress_batcher = self.performance_monitor.progress_batcher();
        let mut batcher = progress_batcher.write().await;
        batcher.add_update(
            file_path.clone(),
            progress.progress_percent,
            progress.current_fps,
        );
        
        // Check if we should send a batch
        if batcher.should_send_batch() {
            let batch = batcher.get_batch();
            drop(batcher); // Release the lock
            
            // Apply batched updates
            for (path, batched_update) in batch {
                let progress_update = ProgressUpdate {
                    file_path: path.clone(),
                    progress_percent: batched_update.progress_percent,
                    current_fps: batched_update.fps,
                    estimated_time_remaining: None,
                };
                self.app_state.update_file_progress(&path, progress_update);
            }
        }
        
        Ok(())
    }
    
    /// Update batch progress from encoding summary
    pub fn update_batch_progress(&mut self, summary: &encoder::EncodingSummary) -> Result<()> {
        self.app_state.batch_progress.completed_files = summary.completed_jobs;
        self.app_state.batch_progress.failed_files = summary.failed_jobs;
        self.app_state.batch_progress.active_files = summary.active_jobs;
        self.app_state.batch_progress.overall_progress = summary.overall_progress;
        Ok(())
    }
    
    /// Clean up terminal state
    pub fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode().map_err(|e| AppError::TuiError(e.to_string()))?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen
        ).map_err(|e| AppError::TuiError(e.to_string()))?;
        Ok(())
    }
    
    /// Check if TUI should render based on performance constraints
    pub fn should_render(&mut self) -> bool {
        let elapsed = self.last_render_time.elapsed();
        if elapsed >= self.render_interval {
            self.last_render_time = Instant::now();
            true
        } else {
            false
        }
    }
    
    /// Set render interval for performance tuning
    pub fn set_render_interval(&mut self, interval: Duration) {
        self.render_interval = interval;
    }
    
    /// Get performance statistics from TUI
    pub async fn get_performance_stats(&self) -> crate::performance::PerformanceStats {
        self.performance_monitor.get_performance_stats().await
    }
    
    /// Optimize TUI memory usage
    pub async fn optimize_memory_usage(&mut self) {
        // Clear old error notifications
        if self.app_state.error_notifications.len() > 5 {
            self.app_state.error_notifications.truncate(5);
        }
        
        // Clear old progress data for completed files
        let completed_files: Vec<PathBuf> = self.app_state.file_progress
            .iter()
            .filter(|(_, info)| matches!(info.status, JobState::Completed))
            .map(|(path, _)| path.clone())
            .collect();
        
        if completed_files.len() > 10 {
            // Keep only the 10 most recent completed files
            for path in &completed_files[..completed_files.len() - 10] {
                self.app_state.file_progress.remove(path);
            }
        }
        
        // Update memory tracking
        let progress_data_size = self.app_state.file_progress.len() * 
                                std::mem::size_of::<FileProgressInfo>();
        self.performance_monitor.memory_tracker()
            .track_progress_data(progress_data_size as u64);
    }
}

fn render_header(f: &mut Frame, area: Rect, app_state: &AppState) {
    let title = format!(
        "Video Encoder - File Selection ({} files found, {} selected)",
        app_state.files.len(),
        app_state.selected_files.len()
    );
    
    let header = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("AMD Hardware Acceleration"));
    
    f.render_widget(header, area);
}

fn render_file_list(f: &mut Frame, area: Rect, app_state: &mut AppState) {
    let items: Vec<ListItem> = app_state
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let is_selected = app_state.selected_files.contains(&i);
            let checkbox = if is_selected { "✓" } else { " " };
            
            let codec_str = match file.codec {
                VideoCodec::H264 => "H.264",
                VideoCodec::H265 => "H.265",
                VideoCodec::Unknown => "Unknown",
            };
            
            let size_str = format_file_size(file.size);
            
            let content = format!("[{}] {} ({}, {})", checkbox, file.filename, codec_str, size_str);
            
            let style = if is_selected {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            
            ListItem::new(Line::from(Span::styled(content, style)))
        })
        .collect();
    
    let files_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files to Encode"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        )
        .highlight_symbol("► ");
    
    f.render_stateful_widget(files_list, area, &mut app_state.list_state);
}

fn render_instructions(f: &mut Frame, area: Rect) {
    let instructions = vec![
        Line::from(vec![
            Span::styled("Navigation: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("↑/↓ or j/k to move, "),
            Span::styled("Space", Style::default().fg(Color::Green)),
            Span::raw(" to select/deselect"),
        ]),
        Line::from(vec![
            Span::styled("Actions: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("a", Style::default().fg(Color::Green)),
            Span::raw(" select all, "),
            Span::styled("d", Style::default().fg(Color::Green)),
            Span::raw(" deselect all, "),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(" confirm, "),
            Span::styled("q/Esc", Style::default().fg(Color::Red)),
            Span::raw(" quit"),
        ]),
    ];
    
    let instructions_widget = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Instructions"))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(instructions_widget, area);
}

fn render_encoding_header(f: &mut Frame, area: Rect, app_state: &AppState) {
    let stats = app_state.get_encoding_stats();
    let title = if app_state.is_encoding_complete() {
        format!(
            "Encoding Complete - {} files processed ({} completed, {} failed)",
            stats.total_files, stats.completed_files, stats.failed_files
        )
    } else {
        format!(
            "Encoding in Progress - {} of {} files ({} active)",
            stats.completed_files + stats.failed_files,
            stats.total_files,
            stats.active_files
        )
    };
    
    let header = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("AMD Hardware Acceleration"));
    
    f.render_widget(header, area);
}

fn render_overall_progress(f: &mut Frame, area: Rect, app_state: &AppState) {
    let stats = app_state.get_encoding_stats();
    let progress_ratio = (stats.overall_progress / 100.0).min(1.0);
    
    let progress_color = if app_state.is_encoding_complete() {
        if stats.failed_files > 0 {
            Color::Yellow
        } else {
            Color::Green
        }
    } else {
        Color::Blue
    };
    
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Overall Progress"))
        .gauge_style(Style::default().fg(progress_color))
        .percent((progress_ratio * 100.0) as u16)
        .label(format!("{:.1}%", stats.overall_progress));
    
    f.render_widget(gauge, area);
}

fn render_file_progress_list(f: &mut Frame, area: Rect, app_state: &AppState) {
    let items: Vec<ListItem> = app_state.file_progress
        .iter()
        .map(|(path, info)| {
            let filename = path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Unknown");
            
            let status_symbol = match info.status {
                JobState::Queued => "⏳",
                JobState::Running => "🔄",
                JobState::Completed => "✅",
                JobState::Failed(_) => "❌",
            };
            
            let progress_bar = create_progress_bar(info.progress_percent, 20);
            
            let fps_text = if let Some(fps) = info.current_fps {
                format!(" {:.1} fps", fps)
            } else {
                String::new()
            };
            
            let eta_text = if let Some(eta) = info.estimated_time_remaining {
                format!(" ETA: {}s", eta.as_secs())
            } else {
                String::new()
            };
            
            let content = format!(
                "{} {} {} {:.1}%{}{}",
                status_symbol, filename, progress_bar, info.progress_percent, fps_text, eta_text
            );
            
            let style = match info.status {
                JobState::Completed => Style::default().fg(Color::Green),
                JobState::Failed(_) => Style::default().fg(Color::Red),
                JobState::Running => Style::default().fg(Color::Yellow),
                JobState::Queued => Style::default().fg(Color::Gray),
            };
            
            ListItem::new(Line::from(Span::styled(content, style)))
        })
        .collect();
    
    let files_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("File Progress"));
    
    f.render_widget(files_list, area);
}

fn render_encoding_statistics(f: &mut Frame, area: Rect, app_state: &AppState) {
    let stats = app_state.get_encoding_stats();
    
    let elapsed_text = if let Some(elapsed) = stats.elapsed_time {
        format!("Elapsed: {}m {}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
    } else {
        "Elapsed: --".to_string()
    };
    
    let eta_text = if let Some(eta) = stats.estimated_time_remaining {
        format!("ETA: {}m {}s", eta.as_secs() / 60, eta.as_secs() % 60)
    } else {
        "ETA: --".to_string()
    };
    
    let fps_text = if let Some(fps) = stats.average_fps {
        format!("Avg FPS: {:.1}", fps)
    } else {
        "Avg FPS: --".to_string()
    };
    
    let statistics = vec![
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} completed, {} failed, {} active", 
                stats.completed_files, stats.failed_files, stats.active_files)),
        ]),
        Line::from(vec![
            Span::styled("Timing: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{} | {}", elapsed_text, eta_text)),
        ]),
        Line::from(vec![
            Span::styled("Performance: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(fps_text),
        ]),
    ];
    
    let statistics_widget = Paragraph::new(statistics)
        .block(Block::default().borders(Borders::ALL).title("Statistics"))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(statistics_widget, area);
}

fn render_encoding_instructions(f: &mut Frame, area: Rect, app_state: &AppState) {
    let instructions = if app_state.is_encoding_complete() {
        vec![
            Line::from(vec![
                Span::styled("Encoding Complete! ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("Press any key to continue..."),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Controls: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("q/Esc", Style::default().fg(Color::Red)),
                Span::raw(" cancel, "),
                Span::styled("e", Style::default().fg(Color::Blue)),
                Span::raw(" toggle errors, "),
                Span::styled("c", Style::default().fg(Color::Blue)),
                Span::raw(" clear errors"),
            ]),
        ]
    };
    
    let instructions_widget = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Instructions"))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(instructions_widget, area);
}

fn render_error_notifications(f: &mut Frame, area: Rect, app_state: &AppState) {
    let notifications = app_state.get_recent_errors();
    
    if notifications.is_empty() {
        let no_errors = Paragraph::new("No errors or warnings")
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::ALL).title("Error Status"));
        f.render_widget(no_errors, area);
        return;
    }
    
    let items: Vec<ListItem> = notifications
        .iter()
        .rev() // Show most recent first
        .take(5) // Limit to 5 most recent
        .map(|notification| {
            let (symbol, color, text) = match notification {
                ErrorNotification::HardwareFallback { reason, fallback_method } => {
                    ("⚠️", Color::Yellow, format!("Hardware fallback: {} → {}", reason, fallback_method))
                }
                ErrorNotification::FileSkipped { file, reason } => {
                    ("❌", Color::Red, format!("Skipped {}: {}", 
                        file.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"), reason))
                }
                ErrorNotification::RecoveryAttempt { file, strategy, attempt } => {
                    ("🔄", Color::Blue, format!("Recovery attempt #{} for {}: {:?}", 
                        attempt, 
                        file.as_ref().and_then(|f| f.file_name()).and_then(|n| n.to_str()).unwrap_or("unknown"),
                        strategy))
                }
                ErrorNotification::BatchContinuation { completed, failed, remaining } => {
                    ("ℹ️", Color::Cyan, format!("Batch status: {} completed, {} failed, {} remaining", 
                        completed, failed, remaining))
                }
                ErrorNotification::CriticalError { error, .. } => {
                    ("🚨", Color::Red, format!("Critical: {}", error))
                }
            };
            
            let content = if app_state.show_error_details {
                format!("{} {}", symbol, text)
            } else {
                format!("{} {}", symbol, text.chars().take(50).collect::<String>())
            };
            
            ListItem::new(Line::from(Span::styled(content, Style::default().fg(color))))
        })
        .collect();
    
    let title = format!("Errors & Warnings ({}) - Press 'e' for details", notifications.len());
    let error_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    
    f.render_widget(error_list, area);
}

fn render_error_statistics(f: &mut Frame, area: Rect, app_state: &AppState) {
    let stats = &app_state.error_statistics;
    
    let statistics = vec![
        Line::from(vec![
            Span::styled("Total Errors: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(stats.total_errors.to_string(), 
                if stats.total_errors > 0 { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) }),
        ]),
        Line::from(vec![
            Span::styled("Recovery Rate: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(if stats.recovery_attempts > 0 {
                format!("{:.1}% ({}/{})", 
                    (stats.successful_recoveries as f32 / stats.recovery_attempts as f32) * 100.0,
                    stats.successful_recoveries,
                    stats.recovery_attempts)
            } else {
                "N/A".to_string()
            }),
        ]),
        Line::from(vec![
            Span::styled("Critical: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(stats.critical_errors.to_string()),
            Span::raw(" | "),
            Span::styled("File-level: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(stats.file_level_errors.to_string()),
            Span::raw(" | "),
            Span::styled("Recoverable: ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::raw(stats.recoverable_errors.to_string()),
        ]),
    ];
    
    let statistics_widget = Paragraph::new(statistics)
        .block(Block::default().borders(Borders::ALL).title("Error Statistics"))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(statistics_widget, area);
}

/// Render error confirmation dialog for critical errors
fn render_error_confirmation_dialog(f: &mut Frame, error: &AppError) {
    let area = centered_rect(60, 40, f.size());
    
    // Clear the background
    let clear = Block::default()
        .style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);
    
    let error_severity = error.severity();
    let (title_color, border_color) = match error_severity {
        ErrorSeverity::Critical => (Color::Red, Color::Red),
        ErrorSeverity::FileLevel => (Color::Yellow, Color::Yellow),
        ErrorSeverity::Recoverable => (Color::Blue, Color::Blue),
        ErrorSeverity::Warning => (Color::Cyan, Color::Cyan),
    };
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Error message
            Constraint::Length(3), // Instructions
        ])
        .split(area);
    
    // Title
    let title = Paragraph::new(format!("{:?} Error", error_severity))
        .style(Style::default().fg(title_color).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(border_color)));
    f.render_widget(title, chunks[0]);
    
    // Error message
    let error_text = format!("{}\n\nRecovery Strategy: {:?}", error, error.recovery_strategy());
    let error_msg = Paragraph::new(error_text)
        .wrap(ratatui::widgets::Wrap { trim: true })
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Error Details"));
    f.render_widget(error_msg, chunks[1]);
    
    // Instructions
    let instructions = if error.allows_batch_continuation() {
        "Continue processing other files? [Y]es / [N]o / [Q]uit"
    } else {
        "This error requires stopping. Press any key to exit."
    };
    
    let instructions_widget = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Action Required"))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(instructions_widget, chunks[2]);
}

/// Render hardware fallback confirmation dialog
fn render_hardware_fallback_dialog(f: &mut Frame, reason: &str, fallback_method: &str) {
    let area = centered_rect(70, 30, f.size());
    
    // Clear the background
    let clear = Block::default()
        .style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Message
            Constraint::Length(3), // Instructions
        ])
        .split(area);
    
    // Title
    let title = Paragraph::new("Hardware Acceleration Fallback")
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)));
    f.render_widget(title, chunks[0]);
    
    // Message
    let message = format!(
        "Hardware acceleration failed: {}\n\nFallback to: {}\n\nNote: Software encoding will be slower but should work on all systems.",
        reason, fallback_method
    );
    let msg_widget = Paragraph::new(message)
        .wrap(ratatui::widgets::Wrap { trim: true })
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Fallback Information"));
    f.render_widget(msg_widget, chunks[1]);
    
    // Instructions
    let instructions = "Continue with software encoding? [Y]es / [N]o";
    let instructions_widget = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Confirmation"))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(instructions_widget, chunks[2]);
}

/// Render batch processing error confirmation dialog
fn render_batch_error_dialog(f: &mut Frame, completed: usize, failed: usize, remaining: usize) {
    let area = centered_rect(60, 35, f.size());
    
    // Clear the background
    let clear = Block::default()
        .style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Status
            Constraint::Length(3), // Instructions
        ])
        .split(area);
    
    // Title
    let title = Paragraph::new("Batch Processing Status")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
    f.render_widget(title, chunks[0]);
    
    // Status
    let status_text = format!(
        "Batch Processing Summary:\n\n✅ Completed: {} files\n❌ Failed: {} files\n⏳ Remaining: {} files\n\nSome files failed to encode, but others completed successfully.",
        completed, failed, remaining
    );
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Processing Results"));
    f.render_widget(status_widget, chunks[1]);
    
    // Instructions
    let instructions = "Continue with remaining files? [Y]es / [N]o / [S]how details";
    let instructions_widget = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Next Action"))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(instructions_widget, chunks[2]);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the encoding progress interface (standalone function to avoid borrow issues)
fn render_encoding_progress_ui(f: &mut Frame, app_state: &AppState) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([
            Constraint::Percentage(70), // Main content
            Constraint::Percentage(30), // Error panel
        ])
        .split(f.size());
    
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Overall progress
            Constraint::Min(0),    // File progress list
            Constraint::Length(5), // Statistics
            Constraint::Length(3), // Instructions
        ])
        .split(main_chunks[0]);
    
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60), // Error notifications
            Constraint::Percentage(40), // Error statistics
        ])
        .split(main_chunks[1]);
    
    // Render main content
    render_encoding_header(f, left_chunks[0], app_state);
    render_overall_progress(f, left_chunks[1], app_state);
    render_file_progress_list(f, left_chunks[2], app_state);
    render_encoding_statistics(f, left_chunks[3], app_state);
    render_encoding_instructions(f, left_chunks[4], app_state);
    
    // Render error panel
    render_error_notifications(f, right_chunks[0], app_state);
    render_error_statistics(f, right_chunks[1], app_state);
}

/// Create a text-based progress bar
fn create_progress_bar(progress: f32, width: usize) -> String {
    let filled = ((progress / 100.0) * width as f32) as usize;
    let empty = width.saturating_sub(filled);
    
    format!("{}{}",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}

impl Drop for TuiManager {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen
        );
    }
}

/// Format file size in human-readable format
fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{:.0}{}", size, UNITS[unit_index])
    } else {
        format!("{:.1}{}", size, UNITS[unit_index])
    }
}
#[
cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::scanner::{VideoFile, VideoCodec};
    
    fn create_test_video_file(filename: &str, path: &str) -> VideoFile {
        VideoFile {
            path: PathBuf::from(path),
            filename: filename.to_string(),
            size: 1024 * 1024 * 100, // 100MB
            codec: VideoCodec::H264,
            resolution: Some((1920, 1080)),
            duration: Some(Duration::from_secs(3600)), // 1 hour
            bitrate: Some(5000),
        }
    }
    
    #[test]
    fn test_app_state_creation() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
            create_test_video_file("test2.mp4", "/test/test2.mp4"),
        ];
        
        let app_state = AppState::new(files.clone());
        
        assert_eq!(app_state.files.len(), 2);
        assert!(app_state.selected_files.is_empty());
        assert!(app_state.file_progress.is_empty());
        assert_eq!(app_state.batch_progress.total_files, 0);
        assert!(!app_state.encoding_active);
        assert!(!app_state.confirmed);
        assert!(!app_state.should_quit);
    }
    
    #[test]
    fn test_progress_tracking_initialization() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
            create_test_video_file("test2.mp4", "/test/test2.mp4"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        assert_eq!(app_state.file_progress.len(), 2);
        assert_eq!(app_state.batch_progress.total_files, 2);
        assert_eq!(app_state.batch_progress.completed_files, 0);
        assert_eq!(app_state.batch_progress.failed_files, 0);
        assert_eq!(app_state.batch_progress.active_files, 0);
        assert!(app_state.encoding_active);
        assert!(app_state.batch_progress.batch_start_time.is_some());
        
        // Check that all files have default progress info
        for file in &files {
            let progress_info = app_state.file_progress.get(&file.path).unwrap();
            assert_eq!(progress_info.progress_percent, 0.0);
            assert!(progress_info.current_fps.is_none());
            assert!(progress_info.estimated_time_remaining.is_none());
            assert_eq!(progress_info.status, JobState::Queued);
            assert!(progress_info.start_time.is_none());
        }
    }
    
    #[test]
    fn test_file_progress_update() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        let progress_update = ProgressUpdate {
            file_path: files[0].path.clone(),
            progress_percent: 50.0,
            current_fps: Some(30.0),
            estimated_time_remaining: Some(Duration::from_secs(120)),
        };
        
        app_state.update_file_progress(&files[0].path, progress_update);
        
        let progress_info = app_state.file_progress.get(&files[0].path).unwrap();
        assert_eq!(progress_info.progress_percent, 50.0);
        assert_eq!(progress_info.current_fps, Some(30.0));
        assert_eq!(progress_info.estimated_time_remaining, Some(Duration::from_secs(120)));
        assert!(progress_info.start_time.is_some());
    }
    
    #[test]
    fn test_job_status_update() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        // Test transition from Queued to Running
        let running_status = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[0].path.clone(),
                output_path: PathBuf::from("/test/output1.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Running,
            progress: 25.0,
        };
        
        app_state.update_job_status(&files[0].path, &running_status);
        
        assert_eq!(app_state.batch_progress.active_files, 1);
        let progress_info = app_state.file_progress.get(&files[0].path).unwrap();
        assert_eq!(progress_info.status, JobState::Running);
        assert_eq!(progress_info.progress_percent, 25.0);
        assert!(progress_info.start_time.is_some());
        
        // Test transition from Running to Completed
        let completed_status = JobStatus {
            job: running_status.job.clone(),
            status: JobState::Completed,
            progress: 100.0,
        };
        
        app_state.update_job_status(&files[0].path, &completed_status);
        
        assert_eq!(app_state.batch_progress.active_files, 0);
        assert_eq!(app_state.batch_progress.completed_files, 1);
        let progress_info = app_state.file_progress.get(&files[0].path).unwrap();
        assert_eq!(progress_info.status, JobState::Completed);
        assert_eq!(progress_info.progress_percent, 100.0);
    }
    
    #[test]
    fn test_batch_progress_calculation() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
            create_test_video_file("test2.mp4", "/test/test2.mp4"),
            create_test_video_file("test3.mkv", "/test/test3.mkv"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        // Update progress for different files
        let progress1 = ProgressUpdate {
            file_path: files[0].path.clone(),
            progress_percent: 100.0, // Completed
            current_fps: None,
            estimated_time_remaining: None,
        };
        
        let progress2 = ProgressUpdate {
            file_path: files[1].path.clone(),
            progress_percent: 50.0, // Half done
            current_fps: Some(25.0),
            estimated_time_remaining: Some(Duration::from_secs(60)),
        };
        
        let progress3 = ProgressUpdate {
            file_path: files[2].path.clone(),
            progress_percent: 0.0, // Not started
            current_fps: None,
            estimated_time_remaining: None,
        };
        
        app_state.update_file_progress(&files[0].path, progress1);
        app_state.update_file_progress(&files[1].path, progress2);
        app_state.update_file_progress(&files[2].path, progress3);
        
        // Mark first file as completed
        let completed_status = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[0].path.clone(),
                output_path: PathBuf::from("/test/output1.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Completed,
            progress: 100.0,
        };
        app_state.update_job_status(&files[0].path, &completed_status);
        
        // Overall progress should be (100 + 50 + 0) / 3 = 50.0
        assert_eq!(app_state.batch_progress.overall_progress, 50.0);
    }
    
    #[test]
    fn test_encoding_completion_detection() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
            create_test_video_file("test2.mp4", "/test/test2.mp4"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        // Initially not complete
        assert!(!app_state.is_encoding_complete());
        
        // Complete one file
        let completed_status1 = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[0].path.clone(),
                output_path: PathBuf::from("/test/output1.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Completed,
            progress: 100.0,
        };
        app_state.update_job_status(&files[0].path, &completed_status1);
        
        // Still not complete (1 of 2 done)
        assert!(!app_state.is_encoding_complete());
        
        // Complete second file
        let completed_status2 = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[1].path.clone(),
                output_path: PathBuf::from("/test/output2.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Completed,
            progress: 100.0,
        };
        app_state.update_job_status(&files[1].path, &completed_status2);
        
        // Now should be complete
        assert!(app_state.is_encoding_complete());
    }
    
    #[test]
    fn test_encoding_stats_calculation() {
        let files = vec![
            create_test_video_file("test1.mkv", "/test/test1.mkv"),
            create_test_video_file("test2.mp4", "/test/test2.mp4"),
            create_test_video_file("test3.mkv", "/test/test3.mkv"),
        ];
        
        let mut app_state = AppState::new(files.clone());
        app_state.initialize_progress_tracking(&files);
        
        // Set up different job states
        let running_status = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[0].path.clone(),
                output_path: PathBuf::from("/test/output1.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Running,
            progress: 75.0,
        };
        
        let completed_status = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[1].path.clone(),
                output_path: PathBuf::from("/test/output2.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Completed,
            progress: 100.0,
        };
        
        let failed_status = JobStatus {
            job: crate::encoder::EncodingJob {
                input_path: files[2].path.clone(),
                output_path: PathBuf::from("/test/output3.mkv"),
                quality_profile: crate::cli::QualityProfile::Medium,
            },
            status: JobState::Failed("Test error".to_string()),
            progress: 0.0,
        };
        
        app_state.update_job_status(&files[0].path, &running_status);
        app_state.update_job_status(&files[1].path, &completed_status);
        app_state.update_job_status(&files[2].path, &failed_status);
        
        // Add FPS data for running job
        let progress_update = ProgressUpdate {
            file_path: files[0].path.clone(),
            progress_percent: 75.0,
            current_fps: Some(30.0),
            estimated_time_remaining: Some(Duration::from_secs(60)),
        };
        app_state.update_file_progress(&files[0].path, progress_update);
        
        let stats = app_state.get_encoding_stats();
        
        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.completed_files, 1);
        assert_eq!(stats.failed_files, 1);
        assert_eq!(stats.active_files, 1);
        assert_eq!(stats.average_fps, Some(30.0));
        assert!(stats.elapsed_time.is_some());
    }
    
    #[test]
    fn test_progress_bar_creation() {
        assert_eq!(create_progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(create_progress_bar(50.0, 10), "█████░░░░░");
        assert_eq!(create_progress_bar(100.0, 10), "██████████");
        assert_eq!(create_progress_bar(25.0, 8), "██░░░░░░");
        assert_eq!(create_progress_bar(75.0, 4), "███░");
    }
    
    #[test]
    fn test_file_progress_info_default() {
        let info = FileProgressInfo::default();
        
        assert_eq!(info.progress_percent, 0.0);
        assert!(info.current_fps.is_none());
        assert!(info.estimated_time_remaining.is_none());
        assert_eq!(info.status, JobState::Queued);
        assert!(info.start_time.is_none());
    }
    
    #[test]
    fn test_batch_progress_info_default() {
        let info = BatchProgressInfo::default();
        
        assert_eq!(info.total_files, 0);
        assert_eq!(info.completed_files, 0);
        assert_eq!(info.failed_files, 0);
        assert_eq!(info.active_files, 0);
        assert_eq!(info.overall_progress, 0.0);
        assert!(info.batch_start_time.is_none());
        assert!(info.estimated_completion_time.is_none());
    }
}