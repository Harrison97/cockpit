//! Agent data model for the God Agent Console
//!
//! This module defines the Agent struct and AgentStatus enum used throughout the TUI.

#![allow(dead_code)] // Items will be used as more features are implemented

use ratatui::style::Color;
use std::fmt;
use std::time::Instant;

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

    /// Starts the agent, setting status to Running and recording start time
    pub fn start(&mut self) {
        self.status = AgentStatus::Running;
        self.start_time = Some(Instant::now());
    }

    /// Stops the agent, setting status to Stopped and clearing start time
    pub fn stop(&mut self) {
        self.status = AgentStatus::Stopped;
        self.start_time = None;
    }

    /// Pauses the agent, setting status to Paused (keeps start time for uptime tracking)
    pub fn pause(&mut self) {
        self.status = AgentStatus::Paused;
    }

    /// Resumes a paused agent, setting status back to Running
    pub fn resume(&mut self) {
        if self.status == AgentStatus::Paused {
            self.status = AgentStatus::Running;
        }
    }
}
