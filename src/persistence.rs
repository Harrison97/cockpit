//! Persistence module for Cockpit state and output logging
//!
//! Handles saving/loading application state and writing output logs to disk.

#![allow(dead_code)] // Functions will be used as more features are implemented

use directories::ProjectDirs;
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
