use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::io::{stdout, Stdout};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyEventKind},
};
use crate::{AppError, Result};
use crate::scanner::{VideoFile, VideoCodec};

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
pub struct AppState {
    pub files: Vec<VideoFile>,
    pub selected_files: HashSet<usize>,
    pub encoding_progress: HashMap<PathBuf, f32>,
    pub overall_progress: f32,
    pub current_selection: usize,
    pub list_state: ListState,
    pub should_quit: bool,
    pub confirmed: bool,
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
            encoding_progress: HashMap::new(),
            overall_progress: 0.0,
            current_selection: 0,
            list_state,
            should_quit: false,
            confirmed: false,
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
}

pub struct TuiManager {
    terminal: TerminalType,
    app_state: AppState,
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