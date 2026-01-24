//! Agent data model for the God Agent Console

#![allow(dead_code)]

use crate::loop_manager::{LoopError, RalphLoop, TerminalData};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::warn;

/// Type of agent: RalphLoop (has PROMPT.md, loops continuously) vs ClaudeInstance (no prompt, single run)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum AgentType {
    /// A ralph loop with PROMPT.md that runs continuously
    #[default]
    RalphLoop,
    /// A single Claude instance without a prompt file (no auto-restart)
    ClaudeInstance,
}

/// Process readiness state for blocking input during transitions
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ProcessState {
    /// Process is starting up, not yet ready for input
    Starting,
    /// Process is ready to receive input
    Ready,
    /// Process is shutting down (initiated stop)
    Stopping,
    /// Sent Ctrl+C, waiting for graceful exit (cleanup sequences)
    Exiting,
    /// Process is not running
    #[default]
    Stopped,
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentType::RalphLoop => write!(f, "Ralph Loop"),
            AgentType::ClaudeInstance => write!(f, "Claude Instance"),
        }
    }
}

/// Errors that can occur when managing agents
#[derive(Debug)]
pub enum AgentError {
    LoopError(LoopError),
    InvalidState(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::LoopError(e) => write!(f, "{}", e),
            AgentError::InvalidState(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<LoopError> for AgentError {
    fn from(err: LoopError) -> Self {
        AgentError::LoopError(err)
    }
}

/// Status of an AI agent
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Stopped,
    Paused,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Running => write!(f, "RUNNING"),
            AgentStatus::Stopped => write!(f, "STOPPED"),
            AgentStatus::Paused => write!(f, "PAUSED"),
        }
    }
}

impl AgentStatus {
    pub fn color(&self) -> Color {
        match self {
            AgentStatus::Running => Color::Green,
            AgentStatus::Stopped => Color::Red,
            AgentStatus::Paused => Color::Yellow,
        }
    }
}

/// Terminal size for the embedded terminal
pub const TERM_COLS: u16 = 180;
pub const TERM_ROWS: u16 = 40;
/// Scrollback buffer size in lines (~100K lines for unlimited history)
pub const SCROLLBACK_SIZE: usize = 100_000;
/// Maximum number of search matches to collect (prevents unbounded memory growth)
pub const MAX_SEARCH_MATCHES: usize = 10_000;
/// Maximum raw history size in memory (2MB) - older data is trimmed
const MAX_RAW_HISTORY_BYTES: usize = 2 * 1024 * 1024;

/// Filter out mouse escape sequences that may leak from PTY
/// Handles multiple mouse protocols:
/// - SGR: `\x1b[<Pb;Px;PyM` or `\x1b[<Pb;Px;Pym`
/// - X10: `\x1b[M` + 3 raw bytes
/// - urxvt: `\x1b[Pb;Px;PyM`
fn filter_mouse_sequences(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    'outer: while i < data.len() {
        // Check for escape sequence start
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
            // SGR mouse: ESC [ < ... M or ESC [ < ... m
            if i + 2 < data.len() && data[i + 2] == b'<' {
                let mut j = i + 3;
                while j < data.len() && data[j] != b'M' && data[j] != b'm' {
                    j += 1;
                }
                if j < data.len() {
                    i = j + 1;
                    continue 'outer;
                }
            }
            // X10 mouse: ESC [ M followed by exactly 3 bytes
            if i + 2 < data.len() && data[i + 2] == b'M' {
                // Skip ESC [ M + 3 coordinate bytes
                i = (i + 6).min(data.len());
                continue 'outer;
            }
            // urxvt mouse: ESC [ digits ; digits ; digits M (uppercase M only!)
            // Lowercase 'm' is used for ANSI color codes, so we must not filter those
            if i + 2 < data.len() && data[i + 2].is_ascii_digit() {
                let mut j = i + 2;
                let mut semicolon_count = 0;
                while j < data.len() {
                    if data[j] == b';' {
                        semicolon_count += 1;
                    } else if data[j] == b'M' {
                        // urxvt mouse has exactly 2 semicolons (button;x;y)
                        if semicolon_count == 2 {
                            i = j + 1;
                            continue 'outer;
                        }
                        break;
                    } else if !data[j].is_ascii_digit() {
                        break;
                    }
                    j += 1;
                }
            }
        }
        result.push(data[i]);
        i += 1;
    }
    result
}

/// Represents an AI agent being monitored by the console
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub start_time: Option<Instant>,
    /// Terminal parser for full TUI rendering
    pub terminal: Arc<Mutex<vt100::Parser>>,
    pub iteration: u32,
    /// The agent's config directory (.cockpit/agents/<name>) where PROMPT.md and history.log live
    pub agent_dir: Option<PathBuf>,
    /// The working directory where Claude Code runs (the repo root)
    pub working_dir: Option<PathBuf>,
    pub ralph_loop: Option<RalphLoop>,
    /// Type of agent: RalphLoop or ClaudeInstance
    pub agent_type: AgentType,
    /// Scroll offset for terminal pane (0 = bottom, positive = scrolled up)
    /// Uses u32 to support scrollback buffers larger than 65535 lines
    pub scroll_offset: u32,
    /// Last known terminal size (rows, cols) for resize optimization
    last_size: (u16, u16),
    /// Process readiness state for blocking input during transitions
    pub process_state: ProcessState,
    /// File handle for writing terminal history to disk
    history_file: Option<File>,
    /// Whether history has been loaded (deferred until first resize for correct sizing)
    history_loaded: bool,
    /// Raw PTY data kept in memory for instant re-wrapping on resize
    raw_history: Vec<u8>,
}

impl Agent {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            terminal: Arc::new(Mutex::new(vt100::Parser::new(
                TERM_ROWS,
                TERM_COLS,
                SCROLLBACK_SIZE,
            ))),
            iteration: 0,
            agent_dir: None,
            working_dir: None,
            last_size: (TERM_ROWS, TERM_COLS),
            ralph_loop: None,
            agent_type: AgentType::default(),
            scroll_offset: 0,
            process_state: ProcessState::Stopped,
            history_file: None,
            history_loaded: true, // No project = no history to load
            raw_history: Vec::new(),
        }
    }

    pub fn with_project(
        name: &str,
        agent_dir: PathBuf,
        working_dir: PathBuf,
        agent_type: AgentType,
    ) -> Self {
        let terminal = Arc::new(Mutex::new(vt100::Parser::new(
            TERM_ROWS,
            TERM_COLS,
            SCROLLBACK_SIZE,
        )));

        // History will be loaded lazily on first resize to use correct terminal size
        // Open history file for appending (creates if needed)
        let history_file = Self::open_history_file(&agent_dir);

        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            terminal,
            iteration: 0,
            agent_dir: Some(agent_dir),
            working_dir: Some(working_dir),
            ralph_loop: None,
            agent_type,
            scroll_offset: 0,
            last_size: (TERM_ROWS, TERM_COLS),
            process_state: ProcessState::Stopped,
            history_file,
            history_loaded: false, // Will load on first resize
            raw_history: Vec::new(),
        }
    }

    /// Get the history file path for this agent (stored in project folder)
    fn get_history_path(agent_dir: &Path) -> PathBuf {
        agent_dir.join("history.log")
    }

    /// Open the history file for appending, creating the directory structure if needed
    fn open_history_file(agent_dir: &Path) -> Option<File> {
        let path = Self::get_history_path(agent_dir);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!(error = %e, path = ?parent, "failed to create history directory");
                return None;
            }
        }

        // Open file in append mode, create if doesn't exist
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => Some(file),
            Err(e) => {
                warn!(error = %e, path = ?path, "failed to open history file");
                None
            }
        }
    }

    /// Load recent history from disk into raw_history buffer and render at current width.
    /// Reads the last ~2MB of history to avoid loading huge files.
    /// Called lazily on first resize to ensure correct terminal size.
    fn load_history(&mut self) {
        if self.history_loaded {
            return;
        }
        self.history_loaded = true;

        let Some(ref agent_dir) = self.agent_dir else {
            return;
        };

        let path = Self::get_history_path(agent_dir);
        if !path.exists() {
            return;
        }

        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return,
        };

        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut reader = BufReader::new(file);

        // If file is larger than limit, seek to the last MAX_RAW_HISTORY_BYTES
        let data = if file_size > MAX_RAW_HISTORY_BYTES as u64 {
            use std::io::Seek;
            let mut reader = reader.into_inner();
            let _ = reader.seek(std::io::SeekFrom::End(-(MAX_RAW_HISTORY_BYTES as i64)));
            let mut buf = Vec::with_capacity(MAX_RAW_HISTORY_BYTES);
            let _ = reader.read_to_end(&mut buf);
            buf
        } else {
            let mut buf = Vec::with_capacity(file_size as usize);
            let _ = reader.read_to_end(&mut buf);
            buf
        };

        // Filter mouse sequences and store in raw_history
        let filtered = filter_mouse_sequences(&data);
        self.raw_history = filtered;

        // Render at current terminal size
        self.rerender_from_raw();
    }

    /// Re-render the terminal from raw_history at the current size.
    /// Called on width changes to properly wrap/unwrap content.
    fn rerender_from_raw(&mut self) {
        let (rows, cols) = self.last_size;

        // Build new terminal OUTSIDE the lock so old content stays visible during processing
        let mut new_term = vt100::Parser::new(rows, cols, SCROLLBACK_SIZE);
        new_term.process(&self.raw_history);

        // Quick swap - only hold lock briefly
        if let Ok(mut term) = self.terminal.lock() {
            *term = new_term;
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        match self.start_time {
            Some(start) => start.elapsed().as_secs(),
            None => 0,
        }
    }

    /// Marker sent by loop_manager when a new Claude iteration is about to start
    const STARTING_MARKER: &'static [u8] = b"[Starting...]";
    /// Marker sent by loop_manager when graceful shutdown begins
    const EXITING_MARKER: &'static [u8] = b"[Exiting...]";
    /// Characters that indicate Claude Code is ready for input
    /// The ❯ prompt character or the banner text
    const READY_INDICATORS: &'static [&'static [u8]] = &[
        "❯".as_bytes(),      // Claude's prompt character
        b"Claude Code",      // Banner text
        b"claude-code",      // Alternative banner
    ];

    /// Process raw terminal data from the PTY
    pub fn process_terminal_data(&mut self, data: &[u8]) {
        // Check for exiting marker (graceful shutdown in progress)
        let contains_exiting_marker = data
            .windows(Self::EXITING_MARKER.len())
            .any(|w| w == Self::EXITING_MARKER);

        if contains_exiting_marker {
            self.process_state = ProcessState::Exiting;
            // Don't write the marker to history or terminal - it's internal
            return;
        }

        // Check for restart transition marker from ralph loop
        // When detected, set Starting state to block input until new Claude instance is ready
        let contains_starting_marker = data
            .windows(Self::STARTING_MARKER.len())
            .any(|w| w == Self::STARTING_MARKER);

        if contains_starting_marker {
            self.process_state = ProcessState::Starting;
            // Don't transition to Ready yet - the starting marker itself doesn't count
            // as real Claude output. Wait for actual output from the new instance.
        } else if self.process_state == ProcessState::Starting {
            // Only transition to Ready when we see actual Claude Code output,
            // not just bash startup noise or terminal control sequences.
            // Look for Claude's prompt character or banner text.
            let contains_ready_indicator = Self::READY_INDICATORS
                .iter()
                .any(|indicator| data.windows(indicator.len()).any(|w| w == *indicator));
            if contains_ready_indicator {
                self.process_state = ProcessState::Ready;
            }
        }

        // Append raw data to history file for persistence
        if let Some(ref mut file) = self.history_file {
            // Write the raw bytes (before filtering) to preserve full history
            let _ = file.write_all(data);
            // Flush periodically isn't needed - OS buffers and we don't need real-time sync
        }

        // Filter out mouse escape sequences that may leak from PTY
        let filtered = filter_mouse_sequences(data);

        // Store filtered data in raw_history for instant re-wrapping on resize
        self.raw_history.extend_from_slice(&filtered);
        // Trim if exceeding max size (keep the most recent data)
        if self.raw_history.len() > MAX_RAW_HISTORY_BYTES {
            let trim_at = self.raw_history.len() - MAX_RAW_HISTORY_BYTES;
            self.raw_history.drain(0..trim_at);
        }

        if let Ok(mut term) = self.terminal.lock() {
            // If scrolled up, track how many new lines are added to maintain position
            let old_scrollback_max = if self.scroll_offset > 0 {
                // Get max scrollback by setting to max and reading clamped value
                term.set_scrollback(usize::MAX);
                let max = term.screen().scrollback();
                term.set_scrollback(0);
                Some(max)
            } else {
                None
            };

            term.process(&filtered);

            // Adjust scroll_offset to maintain absolute position
            if let Some(old_max) = old_scrollback_max {
                term.set_scrollback(usize::MAX);
                let new_max = term.screen().scrollback();
                term.set_scrollback(0);

                let lines_added = new_max.saturating_sub(old_max);
                let new_offset = self.scroll_offset.saturating_add(lines_added as u32);

                // Allow scrolling up to the full scrollback size
                self.scroll_offset = new_offset.min(new_max as u32);
            }
        }
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: u16) {
        if let Ok(mut term) = self.terminal.lock() {
            // Get max scrollback by setting to max and reading clamped value
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            // Allow scrolling up to the full scrollback size
            let new_offset = self
                .scroll_offset
                .saturating_add(lines as u32)
                .min(scrollback_max as u32);
            self.scroll_offset = new_offset;
            // Restore scrollback to 0 (rendering will set it appropriately)
            term.set_scrollback(0);
        }
    }

    /// Scroll down by the given number of lines (towards bottom)
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines as u32);
    }

    /// Reset scroll to bottom (follow output)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to top of scrollback history
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut term) = self.terminal.lock() {
            // Get max scrollback
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            // Allow scrolling to the full scrollback size
            self.scroll_offset = scrollback_max as u32;
            // Restore scrollback to 0 (rendering will set it appropriately)
            term.set_scrollback(0);
        }
    }

    /// Get the terminal height in rows
    pub fn terminal_height(&self) -> u16 {
        self.last_size.0
    }

    /// Resize the terminal to the given dimensions (only if size changed)
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (old_rows, old_cols) = self.last_size;
        let size_changed = (old_rows, old_cols) != (rows, cols);
        let width_changed = cols != old_cols;

        if size_changed {
            self.last_size = (rows, cols);

            // Only reset scroll on width change (vertical resize should maintain position)
            if width_changed {
                self.scroll_offset = 0;
            }

            // Width change: re-render content for proper wrapping
            // Height-only change: just resize vt100 parser (no content re-render)
            if width_changed && self.history_loaded && !self.raw_history.is_empty() {
                const RECENT_BYTES: usize = 10 * 1024;
                let start = self.raw_history.len().saturating_sub(RECENT_BYTES);
                let recent = &self.raw_history[start..];

                let mut new_term = vt100::Parser::new(rows, cols, SCROLLBACK_SIZE);
                new_term.process(recent);

                if let Ok(mut term) = self.terminal.lock() {
                    *term = new_term;
                }
            } else {
                // Height-only or no history: just resize the parser
                if let Ok(mut term) = self.terminal.lock() {
                    term.set_size(rows, cols);
                }
            }

            // Resize the PTY
            if let Some(ref ralph_loop) = self.ralph_loop {
                let _ = ralph_loop.resize(rows, cols);
            }
        }

        // Load history after terminal is at correct size (lazy load on first render)
        if !self.history_loaded {
            self.load_history();
        }
    }

    /// Reset the terminal parser to a fresh state with the current size
    pub fn reset_terminal(&mut self) {
        let (rows, cols) = self.last_size;
        self.terminal = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_SIZE)));
        self.scroll_offset = 0;
    }

    /// Send keyboard input to the agent's PTY.
    /// Input is silently dropped if process is not Ready (e.g., during Starting or Stopping).
    pub fn send_input(&self, data: &[u8]) -> Result<(), AgentError> {
        // Only forward input when process is Ready
        if self.process_state != ProcessState::Ready {
            // Silently drop input during transitions
            return Ok(());
        }

        if let Some(ref ralph_loop) = self.ralph_loop {
            ralph_loop.send_input(data)?;
        }
        Ok(())
    }

    const MAX_SPAWN_RETRIES: u32 = 3;

    pub fn start(&mut self, tx: mpsc::Sender<TerminalData>) -> Result<(), AgentError> {
        if self.status == AgentStatus::Running {
            return Ok(());
        }

        // Don't reset terminal - preserve history from previous sessions
        // Just reset scroll to follow new output
        self.scroll_offset = 0;

        // Set process state to Starting before spawning
        self.process_state = ProcessState::Starting;

        if let Some(ref agent_dir) = self.agent_dir {
            let working_dir = self
                .working_dir
                .clone()
                .unwrap_or_else(|| agent_dir.clone());
            let is_ralph_loop = self.agent_type == AgentType::RalphLoop;
            let mut ralph_loop = RalphLoop::new(agent_dir.clone(), working_dir, is_ralph_loop);

            let mut last_error = None;
            for attempt in 0..Self::MAX_SPAWN_RETRIES {
                match ralph_loop.start(self.name.clone(), tx.clone(), self.last_size) {
                    Ok(()) => {
                        self.ralph_loop = Some(ralph_loop);
                        self.status = AgentStatus::Running;
                        self.start_time = Some(Instant::now());
                        // process_state remains Starting until first output is received
                        return Ok(());
                    }
                    Err(e) => {
                        if !Self::is_transient_error(&e) {
                            self.process_state = ProcessState::Stopped;
                            return Err(e.into());
                        }
                        last_error = Some(e);

                        if attempt < Self::MAX_SPAWN_RETRIES - 1 {
                            std::thread::sleep(std::time::Duration::from_millis(
                                10 * (1 << attempt),
                            ));
                        }
                    }
                }
            }

            self.process_state = ProcessState::Stopped;
            return Err(last_error
                .map(AgentError::from)
                .unwrap_or_else(|| AgentError::InvalidState("Spawn failed".into())));
        }

        self.status = AgentStatus::Running;
        self.start_time = Some(Instant::now());
        // No subprocess, so mark as Ready immediately
        self.process_state = ProcessState::Ready;
        Ok(())
    }

    fn is_transient_error(err: &LoopError) -> bool {
        match err {
            LoopError::SpawnFailed(_) => true,
            LoopError::OutputCaptureFailed => true,
            LoopError::PtyError(_) => true,
            LoopError::StdinWriteFailed(_) => true,
            LoopError::AlreadyRunning
            | LoopError::NotRunning
            | LoopError::ProjectNotFound(_)
            | LoopError::PromptNotFound
            | LoopError::PauseNotSupported
            | LoopError::ClaudeNotFound
            | LoopError::PauseFailed(_)
            | LoopError::ResumeFailed(_) => false,
        }
    }

    /// Stop the agent. Returns immediately; cleanup happens in the background.
    pub fn stop(&mut self) {
        // Set process state to Stopping during shutdown
        self.process_state = ProcessState::Stopping;

        if let Some(ref mut ralph_loop) = self.ralph_loop {
            ralph_loop.stop();
        }
        self.ralph_loop = None;

        self.status = AgentStatus::Stopped;
        self.start_time = None;
        self.process_state = ProcessState::Stopped;
    }

    /// Returns true if this agent can be paused (only RalphLoop agents support pause)
    pub fn can_pause(&self) -> bool {
        self.agent_type == AgentType::RalphLoop
    }

    pub fn pause(&mut self) -> Result<(), AgentError> {
        if !self.can_pause() {
            return Err(AgentError::LoopError(LoopError::PauseNotSupported));
        }

        if self.status != AgentStatus::Running {
            return Err(AgentError::InvalidState("Agent is not running".into()));
        }

        if let Some(ref mut ralph_loop) = self.ralph_loop {
            ralph_loop.pause()?;
        }

        self.status = AgentStatus::Paused;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), AgentError> {
        if self.status != AgentStatus::Paused {
            return Err(AgentError::InvalidState("Agent is not paused".into()));
        }

        if let Some(ref mut ralph_loop) = self.ralph_loop {
            ralph_loop.resume()?;
        }

        self.status = AgentStatus::Running;
        Ok(())
    }

    pub fn has_project(&self) -> bool {
        self.agent_dir.is_some()
    }

    pub fn is_subprocess_running(&self) -> bool {
        self.ralph_loop.as_ref().is_some_and(|rl| rl.is_running())
    }

    /// Returns true if the agent is ready to receive input (process is Ready state)
    pub fn is_ready_for_input(&self) -> bool {
        self.process_state == ProcessState::Ready
    }

    pub fn pid(&self) -> Option<u32> {
        self.ralph_loop.as_ref().and_then(|rl| rl.pid())
    }

    /// Find all matches of a search query in the ENTIRE terminal scrollback.
    /// Returns Vec of (absolute_row, column_start, match_length) where absolute_row
    /// is the row from the top of the entire history (0 = oldest line).
    /// Also returns the scrollback_max value for scroll calculations.
    /// Limited to MAX_SEARCH_MATCHES to prevent unbounded memory growth.
    pub fn find_all_matches(&self, query: &str) -> (Vec<(usize, usize, usize)>, usize) {
        if query.is_empty() {
            return (Vec::new(), 0);
        }

        let mut matches = Vec::new();
        let query_lower = query.to_lowercase();
        let query_chars: Vec<char> = query_lower.chars().collect();

        if let Ok(mut term) = self.terminal.lock() {
            // Get max scrollback
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            let rows = term.screen().size().0 as usize;
            let cols = term.screen().size().1 as usize;

            // For full buffer search, use the full scrollback max
            let render_max = scrollback_max;

            // Helper to search a single row and add matches (returns true if limit reached)
            let search_row = |term: &vt100::Parser,
                              row: usize,
                              absolute_row: usize,
                              matches: &mut Vec<(usize, usize, usize)>|
             -> bool {
                let screen = term.screen();
                let mut row_chars: Vec<(usize, char)> = Vec::new();
                for col in 0..cols {
                    if let Some(cell) = screen.cell(row as u16, col as u16) {
                        let contents = cell.contents();
                        for c in contents.chars() {
                            row_chars.push((col, c));
                        }
                    }
                }

                let row_chars_lower: Vec<char> = row_chars
                    .iter()
                    .map(|(_, c)| c.to_lowercase().next().unwrap_or(*c))
                    .collect();

                for start_idx in 0..row_chars_lower.len() {
                    if start_idx + query_chars.len() > row_chars_lower.len() {
                        break;
                    }

                    let mut found = true;
                    for (i, qc) in query_chars.iter().enumerate() {
                        if row_chars_lower[start_idx + i] != *qc {
                            found = false;
                            break;
                        }
                    }

                    if found {
                        let col = row_chars[start_idx].0;
                        matches.push((absolute_row, col, query.len()));
                        // Check limit to prevent unbounded growth
                        if matches.len() >= MAX_SEARCH_MATCHES {
                            return true; // Limit reached
                        }
                    }
                }
                false // Limit not reached
            };

            // Search at max render scroll (oldest visible content): absolute rows 0 to rows-1
            term.set_scrollback(render_max);
            let mut limit_reached = false;
            for row in 0..rows {
                let absolute_row = row;
                if search_row(&term, row, absolute_row, &mut matches) {
                    limit_reached = true;
                    break;
                }
            }

            // Search at scroll 0 (newest content): absolute rows render_max to render_max+rows-1
            // Skip row 0 at this position since it overlaps with row (rows-1) at max scroll
            // when render_max < rows (the overlap row)
            if !limit_reached {
                term.set_scrollback(0);
                let start_row = if render_max < rows { 1 } else { 0 };
                for row in start_row..rows {
                    let absolute_row = render_max + row;
                    if search_row(&term, row, absolute_row, &mut matches) {
                        break;
                    }
                }
            }

            // Sort by absolute row, then column
            matches.sort_by_key(|&(r, c, _)| (r, c));

            // Restore scrollback to 0
            term.set_scrollback(0);

            return (matches, scrollback_max);
        }

        (matches, 0)
    }

    /// Find matches in the currently visible terminal content only.
    /// Returns Vec of (row, column_start, match_length) where row is relative to visible area.
    ///
    /// NOTE: Due to a vt100 crate limitation, this only works correctly when
    /// scroll_offset <= terminal_height. For larger offsets, we search at the
    /// maximum safe offset.
    pub fn find_visible_matches(&self, query: &str) -> Vec<(usize, usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        let query_lower = query.to_lowercase();
        let query_chars: Vec<char> = query_lower.chars().collect();

        if let Ok(mut term) = self.terminal.lock() {
            let screen = term.screen();
            let rows = screen.size().0 as usize;
            let cols = screen.size().1 as usize;

            if rows == 0 || cols == 0 {
                return Vec::new();
            }

            // Set scrollback offset for search
            term.set_scrollback(self.scroll_offset as usize);

            for row in 0..rows {
                let mut row_chars: Vec<(usize, char)> = Vec::new();
                for col in 0..cols {
                    if let Some(cell) = term.screen().cell(row as u16, col as u16) {
                        let contents = cell.contents();
                        for c in contents.chars() {
                            row_chars.push((col, c));
                        }
                    }
                }

                let row_chars_lower: Vec<char> = row_chars
                    .iter()
                    .map(|(_, c)| c.to_lowercase().next().unwrap_or(*c))
                    .collect();

                for start_idx in 0..row_chars_lower.len() {
                    if start_idx + query_chars.len() > row_chars_lower.len() {
                        break;
                    }

                    let mut found = true;
                    for (i, qc) in query_chars.iter().enumerate() {
                        if row_chars_lower[start_idx + i] != *qc {
                            found = false;
                            break;
                        }
                    }

                    if found {
                        let col = row_chars[start_idx].0;
                        matches.push((row, col, query.len()));
                    }
                }
            }

            // Reset scrollback to 0 to avoid affecting other operations
            term.set_scrollback(0);
        }

        matches
    }

    /// Scroll to show a specific absolute row (centered if possible)
    pub fn scroll_to_absolute_row(&mut self, absolute_row: usize) {
        if let Ok(mut term) = self.terminal.lock() {
            let terminal_height = self.last_size.0 as usize;

            // Get max scrollback
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            // Calculate scroll_offset to center the absolute_row
            // visible_row = absolute_row - (scrollback_max - scroll_offset)
            // We want visible_row ≈ terminal_height/2
            // So: scroll_offset = scrollback_max - absolute_row + terminal_height/2
            let center_offset = terminal_height / 2;
            let ideal_offset =
                scrollback_max as isize - absolute_row as isize + center_offset as isize;
            let new_offset = ideal_offset.max(0).min(scrollback_max as isize) as usize;

            self.scroll_offset = new_offset as u32;
            // Restore scrollback to 0 (rendering will set it appropriately)
            term.set_scrollback(0);
        }
    }

    /// Get the current scroll state for match position calculations
    /// Returns (current_scroll_offset, scrollback_max)
    pub fn get_scroll_state(&self) -> (usize, usize) {
        if let Ok(mut term) = self.terminal.lock() {
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();
            // Restore scrollback to 0
            term.set_scrollback(0);
            return (self.scroll_offset as usize, scrollback_max);
        }
        (0, 0)
    }

    /// Convert a visible row index to absolute row in history
    /// visible_row is 0-indexed from top of visible area
    pub fn visible_row_to_absolute(&self, visible_row: usize) -> usize {
        if let Ok(mut term) = self.terminal.lock() {
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();
            term.set_scrollback(0);

            let scroll_offset = self.scroll_offset as usize;

            // When scrolled: visible row 0 is at (scrollback_max - scroll_offset)
            // When at bottom (scroll_offset=0): visible row 0 is at scrollback_max
            let base_row = scrollback_max.saturating_sub(scroll_offset);
            return base_row + visible_row;
        }
        visible_row
    }

    /// Convert absolute row to visible row (if visible)
    /// Returns None if the row is not currently visible
    pub fn absolute_row_to_visible(&self, absolute_row: usize) -> Option<usize> {
        if let Ok(mut term) = self.terminal.lock() {
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();
            term.set_scrollback(0);

            let terminal_height = self.last_size.0 as usize;
            let scroll_offset = self.scroll_offset as usize;

            let base_row = scrollback_max.saturating_sub(scroll_offset);
            let end_row = base_row + terminal_height;

            if absolute_row >= base_row && absolute_row < end_row {
                return Some(absolute_row - base_row);
            }
        }
        None
    }

    /// Extract text from terminal between two absolute positions
    /// Returns the text content including newlines for multi-line selections
    pub fn extract_text_range(
        &self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) -> String {
        let mut result = String::new();

        if let Ok(mut term) = self.terminal.lock() {
            let screen = term.screen();
            let cols = screen.size().1 as usize;

            // We need to iterate through the terminal content
            // For each row in range, extract text from cells
            for row in start_row..=end_row {
                // Determine column range for this row
                let col_start = if row == start_row { start_col } else { 0 };
                let col_end = if row == end_row {
                    end_col + 1
                } else {
                    cols
                };

                // We need to access the row at the correct scroll position
                // Set scrollback to position this row as visible
                term.set_scrollback(usize::MAX);
                let scrollback_max = term.screen().scrollback();

                // Calculate scroll offset to make this row visible
                // row is absolute, we need it to be in the visible window
                let target_scroll = scrollback_max.saturating_sub(row);
                term.set_scrollback(target_scroll.min(scrollback_max));

                // Now calculate which visible row this maps to
                let visible_row = if target_scroll > 0 {
                    0 // Row is at top when we scroll to it
                } else {
                    row.saturating_sub(scrollback_max)
                };

                // Extract text from this row
                let mut row_text = String::new();
                for col in col_start..col_end.min(cols) {
                    if let Some(cell) = term.screen().cell(visible_row as u16, col as u16) {
                        let contents = cell.contents();
                        if contents.is_empty() {
                            row_text.push(' ');
                        } else {
                            row_text.push_str(&contents);
                        }
                    } else {
                        row_text.push(' ');
                    }
                }

                // Trim trailing spaces from the row (but preserve intentional spaces)
                let trimmed = row_text.trim_end();
                result.push_str(trimmed);

                // Add newline between rows (not after last row)
                if row < end_row {
                    result.push('\n');
                }
            }

            // Restore scrollback to 0
            term.set_scrollback(0);
        }

        result
    }
}
