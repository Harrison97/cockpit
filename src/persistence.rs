//! Persistence module for Cockpit state and output logging
//!
//! Handles saving/loading application state and writing output logs to disk.

#![allow(dead_code)] // Functions will be used as more features are implemented

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Application name for directory paths
const APP_NAME: &str = "cockpit";

/// State file name
const STATE_FILE: &str = "state.json";

/// Log subdirectory name
const LOG_DIR: &str = "logs";

/// Gets the cockpit data directory path (~/.cockpit or platform equivalent)
///
/// Returns None if the home directory cannot be determined.
pub fn get_data_dir() -> Option<PathBuf> {
    // Use directories crate for cross-platform support
    // Falls back to ~/.cockpit on Unix
    if let Some(proj_dirs) = ProjectDirs::from("", "", APP_NAME) {
        Some(proj_dirs.data_dir().to_path_buf())
    } else {
        // Fallback: try to use home directory directly
        dirs_fallback()
    }
}

/// Fallback method to get data directory using home dir
fn dirs_fallback() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(format!(".{}", APP_NAME)))
}

/// Gets the state file path (~/.cockpit/state.json or platform equivalent)
///
/// Returns None if the data directory cannot be determined.
pub fn get_state_file_path() -> Option<PathBuf> {
    get_data_dir().map(|dir| dir.join(STATE_FILE))
}

/// Gets the log directory path (~/.cockpit/logs/ or platform equivalent)
///
/// Returns None if the data directory cannot be determined.
pub fn get_log_dir() -> Option<PathBuf> {
    get_data_dir().map(|dir| dir.join(LOG_DIR))
}

/// Gets the log file path for a specific agent
///
/// Returns the path to ~/.cockpit/logs/{agent_name}.log
pub fn get_agent_log_path(agent_name: &str) -> Option<PathBuf> {
    get_log_dir().map(|dir| dir.join(format!("{}.log", agent_name)))
}

/// State of a single loop persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    /// Name of the agent/loop
    pub name: String,
    /// Path to the ralph project directory
    pub project_path: PathBuf,
    /// Last recorded iteration count
    pub last_iteration: u32,
}

/// Persisted application state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    /// List of all known loops
    pub loops: Vec<LoopState>,
}

impl PersistedState {
    /// Creates a new empty persisted state
    pub fn new() -> Self {
        Self { loops: Vec::new() }
    }

    /// Adds or updates a loop in the state
    ///
    /// If a loop with the same name exists, it is updated. Otherwise, a new entry is added.
    pub fn upsert_loop(&mut self, loop_state: LoopState) {
        if let Some(existing) = self.loops.iter_mut().find(|l| l.name == loop_state.name) {
            *existing = loop_state;
        } else {
            self.loops.push(loop_state);
        }
    }

    /// Removes a loop from the state by name
    pub fn remove_loop(&mut self, name: &str) {
        self.loops.retain(|l| l.name != name);
    }
}

/// Saves the persisted state to disk
///
/// Creates the data directory if it doesn't exist.
/// Returns an error if the state file cannot be written.
pub fn save_state(state: &PersistedState) -> io::Result<()> {
    let state_path = get_state_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine state file path",
        )
    })?;

    // Ensure the parent directory exists
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Serialize and write the state
    let json = serde_json::to_string_pretty(state).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to serialize state: {}", e),
        )
    })?;

    fs::write(&state_path, json)?;
    Ok(())
}

/// Loads the persisted state from disk
///
/// Returns a default empty state if the file doesn't exist.
/// Returns an error if the file exists but cannot be read or parsed.
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
        // Should return Some on most systems with HOME set
        let dir = get_data_dir();
        // Just verify it doesn't panic and returns a path
        assert!(dir.is_some() || std::env::var("HOME").is_err());
    }

    #[test]
    fn test_state_file_path_ends_with_state_json() {
        if let Some(path) = get_state_file_path() {
            assert!(path.ends_with("state.json"));
        }
    }

    #[test]
    fn test_log_dir_ends_with_logs() {
        if let Some(path) = get_log_dir() {
            assert!(path.ends_with("logs"));
        }
    }

    #[test]
    fn test_agent_log_path() {
        if let Some(path) = get_agent_log_path("test_agent") {
            assert!(path.ends_with("test_agent.log"));
        }
    }
}
