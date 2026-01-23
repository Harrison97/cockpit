//! Persistence module for Cockpit state
//!
//! Handles saving/loading application state to disk.
//!
//! Folder structure:
//!   .cockpit/           - project-local cockpit directory
//!   .cockpit/agents/    - agent data and prompts
//!   .cockpit/logs/      - timestamped log files
//!   .cockpit/state.json - persisted application state

#![allow(dead_code)]

use crate::agent::AgentType;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

const COCKPIT_DIR: &str = ".cockpit";
const AGENTS_DIR: &str = "agents";
const LOGS_DIR: &str = "logs";
const STATE_FILE: &str = "state.json";

/// Get the base .cockpit directory in the current working directory
pub fn get_cockpit_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(COCKPIT_DIR)
}

/// Get the agents directory (.cockpit/agents/)
pub fn get_agents_dir() -> PathBuf {
    get_cockpit_dir().join(AGENTS_DIR)
}

/// Get the logs directory (.cockpit/logs/)
pub fn get_logs_dir() -> PathBuf {
    get_cockpit_dir().join(LOGS_DIR)
}

/// Get the state file path (.cockpit/state.json)
pub fn get_state_file_path() -> PathBuf {
    get_cockpit_dir().join(STATE_FILE)
}

/// State of a single loop persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub name: String,
    /// The agent's internal directory (.agents/<name>) where PROMPT.md lives
    pub project_path: PathBuf,
    /// The target repo root where the agent executes commands
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    pub last_iteration: u32,
    /// Type of agent: RalphLoop or ClaudeInstance
    #[serde(default)]
    pub agent_type: AgentType,
}

/// Persisted application state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub loops: Vec<LoopState>,
}

impl PersistedState {
    pub fn new() -> Self {
        Self { loops: Vec::new() }
    }

    pub fn upsert_loop(&mut self, loop_state: LoopState) {
        if let Some(existing) = self.loops.iter_mut().find(|l| l.name == loop_state.name) {
            *existing = loop_state;
        } else {
            self.loops.push(loop_state);
        }
    }

    pub fn remove_loop(&mut self, name: &str) {
        self.loops.retain(|l| l.name != name);
    }
}

pub fn save_state(state: &PersistedState) -> io::Result<()> {
    let state_path = get_state_file_path();

    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize state: {}", e),
        )
    })?;

    // Atomic write: write to temp file, then rename
    // This prevents corruption if the process is interrupted during write
    let temp_path = state_path.with_extension("json.tmp");
    fs::write(&temp_path, &json)?;
    fs::rename(&temp_path, &state_path)?;

    Ok(())
}

pub fn load_state() -> io::Result<PersistedState> {
    let state_path = get_state_file_path();

    if !state_path.exists() {
        return Ok(PersistedState::new());
    }

    let json = fs::read_to_string(&state_path)?;
    let state: PersistedState = serde_json::from_str(&json).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse state: {}", e),
        )
    })?;

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cockpit_dir_ends_with_cockpit() {
        let dir = get_cockpit_dir();
        assert!(dir.ends_with(".cockpit"));
    }

    #[test]
    fn test_agents_dir_structure() {
        let dir = get_agents_dir();
        assert!(dir.ends_with(".cockpit/agents"));
    }

    #[test]
    fn test_state_file_path_ends_with_state_json() {
        let path = get_state_file_path();
        assert!(path.ends_with(".cockpit/state.json"));
    }
}
