//! Persistence module for Cockpit state
//!
//! Handles saving/loading application state to disk.

#![allow(dead_code)]

use crate::agent::AgentType;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

const APP_NAME: &str = "cockpit";
const STATE_FILE: &str = "state.json";

pub fn get_data_dir() -> Option<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("", "", APP_NAME) {
        Some(proj_dirs.data_dir().to_path_buf())
    } else {
        dirs_fallback()
    }
}

fn dirs_fallback() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(format!(".{}", APP_NAME)))
}

pub fn get_state_file_path() -> Option<PathBuf> {
    get_data_dir().map(|dir| dir.join(STATE_FILE))
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
    let state_path = get_state_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine state file path",
        )
    })?;

    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize state: {}", e),
        )
    })?;

    fs::write(&state_path, json)?;
    Ok(())
}

pub fn load_state() -> io::Result<PersistedState> {
    let state_path = get_state_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine state file path",
        )
    })?;

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
    fn test_get_data_dir_is_some() {
        let dir = get_data_dir();
        assert!(dir.is_some() || std::env::var("HOME").is_err());
    }

    #[test]
    fn test_state_file_path_ends_with_state_json() {
        if let Some(path) = get_state_file_path() {
            assert!(path.ends_with("state.json"));
        }
    }
}
