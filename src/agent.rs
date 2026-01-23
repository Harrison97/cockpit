//! Agent data model for the God Agent Console

#![allow(dead_code)]

use crate::loop_manager::{LoopError, RalphLoop, TerminalData};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

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
    /// Process is shutting down
    Stopping,
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
    /// The agent's internal directory (.agents/<name>) where PROMPT.md lives
    pub project_path: Option<PathBuf>,
    /// The target repo root where the agent executes commands
    pub working_dir: Option<PathBuf>,
    pub ralph_loop: Option<RalphLoop>,
    /// Type of agent: RalphLoop or ClaudeInstance
    pub agent_type: AgentType,
    /// Scroll offset for terminal pane (0 = bottom, positive = scrolled up)
    pub scroll_offset: u16,
    /// Last known terminal size (rows, cols) for resize optimization
    last_size: (u16, u16),
    /// Process readiness state for blocking input during transitions
    pub process_state: ProcessState,
}

impl Agent {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            terminal: Arc::new(Mutex::new(vt100::Parser::new(TERM_ROWS, TERM_COLS, 1000))),
            iteration: 0,
            project_path: None,
            working_dir: None,
            last_size: (TERM_ROWS, TERM_COLS),
            ralph_loop: None,
            agent_type: AgentType::default(),
            scroll_offset: 0,
            process_state: ProcessState::Stopped,
        }
    }

    pub fn with_project(
        name: &str,
        project_path: PathBuf,
        working_dir: PathBuf,
        agent_type: AgentType,
    ) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            terminal: Arc::new(Mutex::new(vt100::Parser::new(TERM_ROWS, TERM_COLS, 1000))),
            iteration: 0,
            project_path: Some(project_path),
            working_dir: Some(working_dir),
            ralph_loop: None,
            agent_type,
            scroll_offset: 0,
            last_size: (TERM_ROWS, TERM_COLS),
            process_state: ProcessState::Stopped,
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

    /// Process raw terminal data from the PTY
    pub fn process_terminal_data(&mut self, data: &[u8]) {
        // Check for restart transition marker from ralph loop
        // When detected, set Starting state to block input until new Claude instance is ready
        let contains_starting_marker = data
            .windows(Self::STARTING_MARKER.len())
            .any(|w| w == Self::STARTING_MARKER);

        if contains_starting_marker {
            self.process_state = ProcessState::Starting;
            // Don't transition to Ready yet - the starting marker itself doesn't count
            // as real Claude output. Wait for actual output from the new instance.
        } else {
            // Transition from Starting to Ready when first real output is received
            if self.process_state == ProcessState::Starting && !data.is_empty() {
                self.process_state = ProcessState::Ready;
            }
        }

        // Filter out mouse escape sequences that may leak from PTY
        let filtered = filter_mouse_sequences(data);

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
                // vt100 bug workaround: must clamp to terminal height
                let terminal_height = self.last_size.0 as usize;

                term.set_scrollback(usize::MAX);
                let new_max = term.screen().scrollback();
                term.set_scrollback(0);

                let lines_added = new_max.saturating_sub(old_max);
                let new_offset = self.scroll_offset.saturating_add(lines_added as u16);

                // Clamp to safe max (min of scrollback size and terminal height - 1)
                let safe_max = new_max.min(terminal_height.saturating_sub(1));
                self.scroll_offset = new_offset.min(safe_max as u16);
            }
        }
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: u16) {
        if let Ok(mut term) = self.terminal.lock() {
            // vt100 bug workaround: must clamp to both scrollback size AND terminal height
            let terminal_height = self.last_size.0 as usize;

            // Get max scrollback by setting to max and reading clamped value
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            // Clamp to both (vt100 visible_rows panics if offset > rows.len())
            let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));
            let new_offset = self
                .scroll_offset
                .saturating_add(lines)
                .min(safe_max as u16);
            term.set_scrollback(new_offset as usize);
            self.scroll_offset = new_offset;
        }
    }

    /// Scroll down by the given number of lines (towards bottom)
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Reset scroll to bottom (follow output)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to top of scrollback history
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut term) = self.terminal.lock() {
            let terminal_height = self.last_size.0 as usize;

            // Get max scrollback
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();

            // Clamp to safe max (same as scroll_up)
            let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));
            self.scroll_offset = safe_max as u16;
            term.set_scrollback(safe_max);
        }
    }

    /// Get the terminal height in rows
    pub fn terminal_height(&self) -> u16 {
        self.last_size.0
    }

    /// Resize the terminal to the given dimensions (only if size changed)
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if self.last_size == (rows, cols) {
            return;
        }
        self.last_size = (rows, cols);

        // Reset scroll before resize to avoid stale offset issues
        self.scroll_offset = 0;

        // Resize the vt100 parser
        if let Ok(mut term) = self.terminal.lock() {
            term.set_scrollback(0);
            term.set_size(rows, cols);
        }

        // Resize the PTY
        if let Some(ref ralph_loop) = self.ralph_loop {
            let _ = ralph_loop.resize(rows, cols);
        }
    }

    /// Reset the terminal parser to a fresh state with the current size
    pub fn reset_terminal(&mut self) {
        let (rows, cols) = self.last_size;
        self.terminal = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
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

        // Reset terminal to fresh state with current size
        self.reset_terminal();

        // Set process state to Starting before spawning
        self.process_state = ProcessState::Starting;

        if let Some(ref project_path) = self.project_path {
            let working_dir = self
                .working_dir
                .clone()
                .unwrap_or_else(|| project_path.clone());
            let is_ralph_loop = self.agent_type == AgentType::RalphLoop;
            let mut ralph_loop = RalphLoop::new(project_path.clone(), working_dir, is_ralph_loop);

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

    pub async fn stop(&mut self) -> Result<(), AgentError> {
        // Set process state to Stopping during shutdown
        self.process_state = ProcessState::Stopping;

        if let Some(ref mut ralph_loop) = self.ralph_loop {
            let _ = ralph_loop.stop().await;
        }
        self.ralph_loop = None;

        self.status = AgentStatus::Stopped;
        self.start_time = None;
        self.process_state = ProcessState::Stopped;
        Ok(())
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
        self.project_path.is_some()
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
    /// Also returns the safe_max value for scroll calculations.
    pub fn find_all_matches(&self, query: &str) -> (Vec<(usize, usize, usize)>, usize) {
        if query.is_empty() {
            return (Vec::new(), 0);
        }

        let mut matches = Vec::new();
        let query_lower = query.to_lowercase();
        let query_chars: Vec<char> = query_lower.chars().collect();

        if let Ok(mut term) = self.terminal.lock() {
            let terminal_height = self.last_size.0 as usize;

            // Get max scrollback
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();
            let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));

            let rows = term.screen().size().0 as usize;
            let cols = term.screen().size().1 as usize;

            // Helper to search a single row and add matches
            let search_row =
                |term: &vt100::Parser,
                 row: usize,
                 absolute_row: usize,
                 matches: &mut Vec<(usize, usize, usize)>| {
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
                        }
                    }
                };

            // Search at max scroll (oldest content): absolute rows 0 to rows-1
            term.set_scrollback(safe_max);
            for row in 0..rows {
                let absolute_row = row;
                search_row(&term, row, absolute_row, &mut matches);
            }

            // Search at scroll 0 (newest content): absolute rows safe_max to safe_max+rows-1
            // Skip row 0 at this position since it overlaps with row (rows-1) at max scroll
            // when safe_max == rows-1 (which is always true due to clamping)
            term.set_scrollback(0);
            for row in 1..rows {
                let absolute_row = safe_max + row;
                search_row(&term, row, absolute_row, &mut matches);
            }

            // Sort by absolute row, then column
            matches.sort_by_key(|&(r, c, _)| (r, c));

            // Restore current scroll position
            term.set_scrollback(self.scroll_offset as usize);

            return (matches, safe_max);
        }

        (matches, 0)
    }

    /// Find matches in the currently visible terminal content only.
    /// Returns Vec of (row, column_start, match_length) where row is relative to visible area.
    /// Uses the current scroll_offset that was set by scroll_up/scroll_down.
    pub fn find_visible_matches(&self, query: &str) -> Vec<(usize, usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        let query_lower = query.to_lowercase();
        let query_chars: Vec<char> = query_lower.chars().collect();

        if let Ok(mut term) = self.terminal.lock() {
            // Use the current scroll_offset - don't recalculate
            // This ensures we search the same content that will be rendered
            term.set_scrollback(self.scroll_offset as usize);

            let screen = term.screen();
            let rows = screen.size().0 as usize;
            let cols = screen.size().1 as usize;

            for row in 0..rows {
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
                        matches.push((row, col, query.len()));
                    }
                }
            }
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
            let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));

            // Calculate scroll_offset to center the absolute_row
            // visible_row = absolute_row - (safe_max - scroll_offset)
            // We want visible_row ≈ terminal_height/2
            // So: scroll_offset = safe_max - absolute_row + terminal_height/2
            let center_offset = terminal_height / 2;
            let ideal_offset = safe_max as isize - absolute_row as isize + center_offset as isize;
            let new_offset = ideal_offset.max(0).min(safe_max as isize) as usize;

            self.scroll_offset = new_offset as u16;
            term.set_scrollback(new_offset);
        }
    }

    /// Get the current scroll state for match position calculations
    /// Returns (current_scroll_offset, safe_max)
    pub fn get_scroll_state(&self) -> (usize, usize) {
        if let Ok(mut term) = self.terminal.lock() {
            let terminal_height = self.last_size.0 as usize;
            term.set_scrollback(usize::MAX);
            let scrollback_max = term.screen().scrollback();
            let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));
            // Restore the current scroll offset
            term.set_scrollback(self.scroll_offset as usize);
            return (self.scroll_offset as usize, safe_max);
        }
        (0, 0)
    }
}
