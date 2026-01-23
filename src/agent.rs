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
            ralph_loop: None,
            agent_type: AgentType::default(),
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
        if let Ok(mut term) = self.terminal.lock() {
            term.process(data);
        }
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

        if let Some(ref project_path) = self.project_path {
            let working_dir = self
                .working_dir
                .clone()
                .unwrap_or_else(|| project_path.clone());
            let is_ralph_loop = self.agent_type == AgentType::RalphLoop;
            let mut ralph_loop = RalphLoop::new(project_path.clone(), working_dir, is_ralph_loop);

            let mut last_error = None;
            for attempt in 0..Self::MAX_SPAWN_RETRIES {
                match ralph_loop.start(self.name.clone(), tx.clone()) {
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

    pub fn pause(&mut self) -> Result<(), AgentError> {
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
