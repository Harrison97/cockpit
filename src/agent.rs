//! Agent data model for the God Agent Console
//!
//! This module defines the Agent struct and AgentStatus enum used throughout the TUI.

#![allow(dead_code)] // Items will be used as more features are implemented

use crate::loop_manager::{LoopError, OutputLine, RalphLoop};
use ratatui::style::Color;
use std::fmt;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

/// Errors that can occur when managing agents
#[derive(Debug)]
pub enum AgentError {
    /// Error from the underlying ralph loop
    LoopError(LoopError),
    /// Agent is not in a valid state for the requested operation
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
    /// Returns the color to use when displaying this status in the UI
    pub fn color(&self) -> Color {
        match self {
            AgentStatus::Running => Color::Green,
            AgentStatus::Stopped => Color::Red,
            AgentStatus::Paused => Color::Yellow,
        }
    }
}

/// Represents an AI agent being monitored by the console
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub start_time: Option<Instant>,
    pub output: Vec<String>,
    pub iteration: u32,
    /// Path to the ralph project directory (if this agent runs a real ralph loop)
    pub project_path: Option<PathBuf>,
    /// The ralph loop subprocess manager (if running a real loop)
    pub ralph_loop: Option<RalphLoop>,
}

impl Agent {
    /// Creates a new agent with the given name
    ///
    /// The agent starts in Stopped status with no output.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            output: Vec::new(),
            iteration: 0,
            project_path: None,
            ralph_loop: None,
        }
    }

    /// Creates a new agent with a ralph project path
    ///
    /// This agent will run a real ralph loop subprocess when started.
    pub fn with_project(name: &str, project_path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            output: Vec::new(),
            iteration: 0,
            project_path: Some(project_path),
            ralph_loop: None,
        }
    }

    /// Returns the uptime in seconds, or 0 if the agent has no start time
    pub fn uptime_secs(&self) -> u64 {
        match self.start_time {
            Some(start) => start.elapsed().as_secs(),
            None => 0,
        }
    }

    /// Adds an output line to the agent's output buffer
    pub fn add_output(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    /// Maximum number of retries for transient spawn failures
    const MAX_SPAWN_RETRIES: u32 = 3;

    /// Starts the agent, spawning a real ralph loop subprocess if a project path is configured.
    ///
    /// If no project_path is set, only updates the status (legacy behavior).
    /// Includes retry logic for transient spawn failures (up to 3 attempts).
    ///
    /// # Errors
    /// Returns `AgentError::LoopError` if the subprocess fails to start after all retries.
    pub fn start(&mut self, tx: mpsc::Sender<OutputLine>) -> Result<(), AgentError> {
        if self.status == AgentStatus::Running {
            return Ok(()); // Already running
        }

        // If we have a project path, spawn the real subprocess
        if let Some(ref project_path) = self.project_path {
            let mut ralph_loop = RalphLoop::new(project_path.clone());

            // Retry logic for transient spawn failures
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
                        // Don't retry for non-transient errors (config/validation issues)
                        if !Self::is_transient_error(&e) {
                            return Err(e.into());
                        }
                        last_error = Some(e);

                        // Brief delay before retry (exponential backoff: 10ms, 20ms, 40ms)
                        if attempt < Self::MAX_SPAWN_RETRIES - 1 {
                            std::thread::sleep(std::time::Duration::from_millis(
                                10 * (1 << attempt),
                            ));
                        }
                    }
                }
            }

            // All retries exhausted
            return Err(last_error
                .map(AgentError::from)
                .unwrap_or_else(|| AgentError::InvalidState("Spawn failed".into())));
        }

        self.status = AgentStatus::Running;
        self.start_time = Some(Instant::now());
        Ok(())
    }

    /// Determines if an error is transient and worth retrying
    fn is_transient_error(err: &LoopError) -> bool {
        match err {
            // Only retry spawn failures (could be temporary resource constraints)
            LoopError::SpawnFailed(_) => true,
            LoopError::OutputCaptureFailed => true,
            // Don't retry configuration/validation errors
            LoopError::AlreadyRunning
            | LoopError::NotRunning
            | LoopError::ProjectNotFound(_)
            | LoopError::PromptNotFound
            | LoopError::ClaudeNotFound
            | LoopError::PauseFailed(_)
            | LoopError::ResumeFailed(_) => false,
        }
    }

    /// Stops the agent, killing the subprocess if running.
    ///
    /// This is an async method because stopping the subprocess requires waiting.
    pub async fn stop(&mut self) -> Result<(), AgentError> {
        // Stop the ralph loop subprocess if we have one
        if let Some(ref mut ralph_loop) = self.ralph_loop {
            let _ = ralph_loop.stop().await;
        }
        self.ralph_loop = None;

        self.status = AgentStatus::Stopped;
        self.start_time = None;
        Ok(())
    }

    /// Pauses the agent by sending SIGSTOP to the subprocess.
    /// Keeps start time for uptime tracking.
    ///
    /// # Errors
    /// Returns `AgentError::InvalidState` if the agent is not running.
    /// Returns `AgentError::LoopError` if the signal fails to send.
    pub fn pause(&mut self) -> Result<(), AgentError> {
        if self.status != AgentStatus::Running {
            return Err(AgentError::InvalidState("Agent is not running".into()));
        }

        // Send SIGSTOP to the ralph loop subprocess if we have one
        if let Some(ref mut ralph_loop) = self.ralph_loop {
            ralph_loop.pause()?;
        }

        self.status = AgentStatus::Paused;
        Ok(())
    }

    /// Resumes a paused agent by sending SIGCONT to the subprocess.
    ///
    /// # Errors
    /// Returns `AgentError::InvalidState` if the agent is not paused.
    /// Returns `AgentError::LoopError` if the signal fails to send.
    pub fn resume(&mut self) -> Result<(), AgentError> {
        if self.status != AgentStatus::Paused {
            return Err(AgentError::InvalidState("Agent is not paused".into()));
        }

        // Send SIGCONT to the ralph loop subprocess if we have one
        if let Some(ref mut ralph_loop) = self.ralph_loop {
            ralph_loop.resume()?;
        }

        self.status = AgentStatus::Running;
        Ok(())
    }

    /// Returns true if this agent has a real ralph loop configured
    pub fn has_project(&self) -> bool {
        self.project_path.is_some()
    }

    /// Returns true if the ralph loop subprocess is running
    pub fn is_subprocess_running(&self) -> bool {
        self.ralph_loop.as_ref().is_some_and(|rl| rl.is_running())
    }
}

/// Creates demo agents for demonstration purposes
///
/// Returns 3 stopped agents (no project paths configured yet).
/// To use real subprocesses, create agents with Agent::with_project().
pub fn create_demo_agents() -> Vec<Agent> {
    vec![Agent::new("alpha"), Agent::new("beta"), Agent::new("gamma")]
}
