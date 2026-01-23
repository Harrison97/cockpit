//! Persistence module for Cockpit state and output logging
//!
//! Handles saving/loading application state and writing output logs to disk.

#![allow(dead_code)] // Functions will be used as more features are implemented

use chrono::{DateTime, Local};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
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

/// Appends an output line to an agent's log file
///
/// Writes to ~/.cockpit/logs/{agent_name}.log in append mode.
/// Each line is prefixed with an ISO 8601 timestamp.
/// Creates the log directory and file if they don't exist.
pub fn append_to_log(agent_name: &str, line: &str) -> io::Result<()> {
    let log_path = get_agent_log_path(agent_name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not determine log path"))?;

    // Ensure the log directory exists
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open file in append mode, creating if necessary
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Get current timestamp in ISO 8601 format
    let timestamp: DateTime<Local> = Local::now();

    // Write the timestamped line
    writeln!(
        file,
        "[{}] {}",
        timestamp.format("%Y-%m-%dT%H:%M:%S%.3f"),
        line
    )?;

    Ok(())
}

/// Loads recent output history from an agent's log file
///
/// Reads the last `max_lines` from ~/.cockpit/logs/{agent_name}.log.
/// Returns the lines without timestamps (for display in the output buffer).
/// Returns an empty vector if the log file doesn't exist.
pub fn load_recent_log(agent_name: &str, max_lines: usize) -> io::Result<Vec<String>> {
    let log_path = get_agent_log_path(agent_name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not determine log path"))?;

    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&log_path)?;
    let reader = BufReader::new(file);

    // Collect all lines, stripping timestamps
    let lines: Vec<String> = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| {
            // Lines are formatted as "[timestamp] content"
            // Find the first "] " and take everything after
            line.find("] ").map(|idx| line[idx + 2..].to_string())
        })
        .collect();

    // Return the last max_lines
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].to_vec())
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

    #[test]
    fn test_load_recent_log_nonexistent() {
        // Loading from a nonexistent agent should return empty vec
        let result = load_recent_log("nonexistent_test_agent_xyz", 100);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
