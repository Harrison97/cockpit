use nix::libc;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use regex::Regex;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// A line of output from a ralph loop subprocess.
/// Sent from the async reader task to the main app via mpsc channel.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OutputLine {
    /// Which agent this output belongs to
    pub agent_name: String,

    /// The actual output text
    pub line: String,

    /// When this line was received
    pub timestamp: Instant,
}

#[allow(dead_code)]
impl OutputLine {
    /// Create a new OutputLine with the current timestamp
    pub fn new(agent_name: String, line: String) -> Self {
        Self {
            agent_name,
            line,
            timestamp: Instant::now(),
        }
    }
}

/// Result of checking a line for iteration boundary markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IterationBoundary {
    /// Line indicates a new iteration completed (e.g., commit message like "feat:")
    Completed,
    /// Line indicates the agent is done and exiting (e.g., "I'm done with...")
    Done,
    /// Line does not indicate an iteration boundary
    None,
}

/// Detects iteration boundaries in ralph loop output.
///
/// Recognizes two types of boundaries:
/// 1. Commit messages (conventional commit format: feat:, fix:, etc.)
/// 2. Exit messages ("I'm done with" pattern)
pub struct IterationDetector {
    /// Regex for conventional commit message patterns
    commit_pattern: &'static Regex,
    /// Regex for "I'm done with" exit messages
    done_pattern: &'static Regex,
}

/// Static regex for conventional commit patterns (feat:, fix:, refactor:, etc.)
static COMMIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(feat|fix|refactor|chore|docs|test|style|perf|ci|build|revert)(\(.+\))?:\s*.+")
        .expect("commit regex should be valid")
});

/// Static regex for "I'm done with" exit messages
static DONE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)I'm done with").expect("done regex should be valid"));

impl Default for IterationDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl IterationDetector {
    /// Create a new IterationDetector with default patterns.
    pub fn new() -> Self {
        Self {
            commit_pattern: &COMMIT_REGEX,
            done_pattern: &DONE_REGEX,
        }
    }

    /// Check if a line indicates an iteration boundary.
    ///
    /// Returns the type of boundary detected, or None if this is a regular line.
    pub fn check_line(&self, line: &str) -> IterationBoundary {
        // Check for "I'm done with" first (higher priority - indicates exit)
        if self.done_pattern.is_match(line) {
            return IterationBoundary::Done;
        }

        // Check for conventional commit patterns
        if self.commit_pattern.is_match(line) {
            return IterationBoundary::Completed;
        }

        IterationBoundary::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_feat_commit() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("feat: add new feature"),
            IterationBoundary::Completed
        );
        assert_eq!(
            detector.check_line("  feat: indented commit"),
            IterationBoundary::Completed
        );
    }

    #[test]
    fn test_detect_fix_commit() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("fix: resolve bug"),
            IterationBoundary::Completed
        );
    }

    #[test]
    fn test_detect_scoped_commit() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("feat(api): add endpoint"),
            IterationBoundary::Completed
        );
        assert_eq!(
            detector.check_line("fix(ui): button alignment"),
            IterationBoundary::Completed
        );
    }

    #[test]
    fn test_detect_other_commit_types() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("refactor: clean up code"),
            IterationBoundary::Completed
        );
        assert_eq!(
            detector.check_line("chore: update deps"),
            IterationBoundary::Completed
        );
        assert_eq!(
            detector.check_line("docs: update readme"),
            IterationBoundary::Completed
        );
        assert_eq!(
            detector.check_line("test: add unit tests"),
            IterationBoundary::Completed
        );
    }

    #[test]
    fn test_detect_done_message() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("I'm done with task 7.1"),
            IterationBoundary::Done
        );
        assert_eq!(
            detector.check_line("say \"I'm done with implementing the feature.\""),
            IterationBoundary::Done
        );
    }

    #[test]
    fn test_no_boundary() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("Reading file..."),
            IterationBoundary::None
        );
        assert_eq!(
            detector.check_line("  some output"),
            IterationBoundary::None
        );
        assert_eq!(
            detector.check_line("feature complete"),
            IterationBoundary::None
        );
    }

    #[test]
    fn test_done_takes_priority() {
        let detector = IterationDetector::new();
        // If a line somehow contains both patterns, done should take priority
        // This is unlikely in practice but tests priority ordering
        assert_eq!(
            detector.check_line("I'm done with feat: something"),
            IterationBoundary::Done
        );
    }
}

/// Manages a ralph loop subprocess.
/// A ralph loop continuously runs Claude Code in a bash loop,
/// reading PROMPT.md and executing tasks.
#[allow(dead_code)]
pub struct RalphLoop {
    /// Path to the ralph project directory
    pub project_path: PathBuf,

    /// The running bash process (if any)
    child: Option<tokio::process::Child>,

    /// Process ID for signal handling
    pid: Option<u32>,

    /// Process group ID for killing child processes
    pgid: Option<i32>,
}

#[allow(dead_code)]
impl RalphLoop {
    /// Create a new RalphLoop for a project directory. Does not start the loop.
    pub fn new(project_path: PathBuf) -> Self {
        Self {
            project_path,
            child: None,
            pid: None,
            pgid: None,
        }
    }

    /// Check if the subprocess is still alive.
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// Start the ralph loop subprocess.
    ///
    /// Spawns bash with the ralph loop command and configures stdout capture.
    /// The agent_name is used to tag output lines sent through the channel.
    pub fn start(
        &mut self,
        agent_name: String,
        tx: mpsc::Sender<OutputLine>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.is_running() {
            return Err("Loop is already running".into());
        }

        let path = self.project_path.clone();
        let path_str = path.to_string_lossy();

        // Build the bash command that runs the ralph loop
        let cmd = format!(
            "cd {} && while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions 2>&1; sleep 1; done",
            path_str
        );

        // Spawn bash with process group for signal handling
        let mut command = Command::new("bash");
        command
            .args(["-c", &cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::null()); // stderr merged via 2>&1 in bash command

        // Set process group to 0 so the child becomes its own process group leader
        // This allows us to send signals to the entire group (bash + claude)
        unsafe {
            command.pre_exec(|| {
                // Create new process group with this process as leader
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut child = command.spawn()?;

        // Extract PID and PGID
        let pid = child.id();
        self.pid = pid;
        // PGID equals PID when we create a new process group
        self.pgid = pid.map(|p| p as i32);

        // Take stdout for the async reader
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture stdout from subprocess")?;

        // Store the child handle
        self.child = Some(child);

        // Spawn async task to read stdout lines and send to channel
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let output_line = OutputLine::new(agent_name.clone(), line);
                        // If the receiver is dropped, stop reading
                        if tx.send(output_line).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        // EOF - process has closed stdout
                        break;
                    }
                    Err(_) => {
                        // Read error - stop reading
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Pause the ralph loop subprocess.
    ///
    /// Sends SIGSTOP to the process group, freezing all processes (bash and claude).
    pub fn pause(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pgid = match self.pgid {
            Some(pgid) => pgid,
            None => return Err("Loop is not running".into()),
        };

        // Send SIGSTOP to the entire process group (negative PID targets the group)
        kill(Pid::from_raw(-pgid), Signal::SIGSTOP)
            .map_err(|e| format!("Failed to pause process group: {}", e))?;

        Ok(())
    }

    /// Resume a paused ralph loop subprocess.
    ///
    /// Sends SIGCONT to the process group, allowing processes to continue.
    pub fn resume(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pgid = match self.pgid {
            Some(pgid) => pgid,
            None => return Err("Loop is not running".into()),
        };

        // Send SIGCONT to the entire process group (negative PID targets the group)
        kill(Pid::from_raw(-pgid), Signal::SIGCONT)
            .map_err(|e| format!("Failed to resume process group: {}", e))?;

        Ok(())
    }

    /// Stop the ralph loop subprocess.
    ///
    /// Sends SIGTERM to the process group, waits up to 5 seconds for graceful shutdown,
    /// then sends SIGKILL if the process is still running.
    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get the process group ID, or return early if not running
        let pgid = match self.pgid {
            Some(pgid) => pgid,
            None => {
                // Already stopped, clear state and return success
                self.child = None;
                self.pid = None;
                return Ok(());
            }
        };

        // Send SIGTERM to the entire process group (negative PID targets the group)
        let _ = kill(Pid::from_raw(-pgid), Signal::SIGTERM);

        // Wait up to 5 seconds for graceful shutdown
        let deadline = Instant::now() + Duration::from_secs(5);

        if let Some(ref mut child) = self.child {
            loop {
                // Check if process has exited using try_wait
                match child.try_wait() {
                    Ok(Some(_)) => {
                        // Process has exited
                        break;
                    }
                    Ok(None) => {
                        // Still running, check if we've exceeded the timeout
                        if Instant::now() >= deadline {
                            // Timeout reached, send SIGKILL to the process group
                            let _ = kill(Pid::from_raw(-pgid), Signal::SIGKILL);
                            // Wait a bit more for SIGKILL to take effect
                            let _ = child.wait().await;
                            break;
                        }
                        // Sleep briefly before checking again
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(_) => {
                        // Error checking status, assume dead
                        break;
                    }
                }
            }
        }

        // Clear all state
        self.child = None;
        self.pid = None;
        self.pgid = None;

        Ok(())
    }
}
