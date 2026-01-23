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
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        match self.start_time {
            Some(start) => start.elapsed().as_secs(),
            None => 0,
        }
    }

    /// Process raw terminal data from the PTY
    pub fn process_terminal_data(&mut self, data: &[u8]) {
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

    /// Send keyboard input to the agent's PTY
    pub fn send_input(&self, data: &[u8]) -> Result<(), AgentError> {
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
                        return Ok(());
                    }
                    Err(e) => {
                        if !Self::is_transient_error(&e) {
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

            return Err(last_error
                .map(AgentError::from)
                .unwrap_or_else(|| AgentError::InvalidState("Spawn failed".into())));
        }

        self.status = AgentStatus::Running;
        self.start_time = Some(Instant::now());
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
        if let Some(ref mut ralph_loop) = self.ralph_loop {
            let _ = ralph_loop.stop().await;
        }
        self.ralph_loop = None;

        self.status = AgentStatus::Stopped;
        self.start_time = None;
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

    pub fn pid(&self) -> Option<u32> {
        self.ralph_loop.as_ref().and_then(|rl| rl.pid())
    }
}
