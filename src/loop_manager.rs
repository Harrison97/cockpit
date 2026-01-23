#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use pty_process::{open as open_pty, Command as PtyCommand, Size};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

/// Errors that can occur when managing ralph loops
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum LoopError {
    #[error("Loop is already running")]
    AlreadyRunning,

    #[error("Loop is not running")]
    NotRunning,

    #[error("Project directory not found: {0}")]
    ProjectNotFound(PathBuf),

    #[error("PROMPT.md not found in project directory")]
    PromptNotFound,

    #[error("Failed to start subprocess: {0}")]
    SpawnFailed(String),

    #[error("Failed to capture subprocess output")]
    OutputCaptureFailed,

    #[error("Failed to pause: {0}")]
    PauseFailed(String),

    #[error("Failed to resume: {0}")]
    ResumeFailed(String),

    #[error("Cannot pause Claude instances (only ralph loops can be paused)")]
    PauseNotSupported,

    #[error("Claude CLI not found. Install it with: npm install -g @anthropic-ai/claude-code")]
    ClaudeNotFound,

    #[error("PTY error: {0}")]
    PtyError(String),

    #[error("Failed to write to stdin: {0}")]
    StdinWriteFailed(String),
}

/// Raw terminal data from a ralph loop subprocess.
#[derive(Debug, Clone)]
pub struct TerminalData {
    pub agent_name: String,
    pub data: Vec<u8>,
}

impl TerminalData {
    pub fn new(agent_name: String, data: Vec<u8>) -> Self {
        Self { agent_name, data }
    }
}

/// For backwards compatibility - line-based output
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OutputLine {
    pub agent_name: String,
    pub line: String,
    pub timestamp: std::time::Instant,
}

#[allow(dead_code)]
impl OutputLine {
    pub fn new(agent_name: String, line: String) -> Self {
        Self {
            agent_name,
            line,
            timestamp: std::time::Instant::now(),
        }
    }
}

/// Result of checking a line for iteration boundary markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IterationBoundary {
    Completed,
    Done,
    None,
}

/// Detects iteration boundaries in ralph loop output.
pub struct IterationDetector {
    commit_pattern: &'static Regex,
    done_pattern: &'static Regex,
}

static COMMIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(feat|fix|refactor|chore|docs|test|style|perf|ci|build|revert)(\(.+\))?:\s*.+")
        .expect("commit regex should be valid")
});

static DONE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)I'm done with").expect("done regex should be valid"));

impl Default for IterationDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl IterationDetector {
    pub fn new() -> Self {
        Self {
            commit_pattern: &COMMIT_REGEX,
            done_pattern: &DONE_REGEX,
        }
    }

    pub fn check_line(&self, line: &str) -> IterationBoundary {
        if self.done_pattern.is_match(line) {
            return IterationBoundary::Done;
        }
        if self.commit_pattern.is_match(line) {
            return IterationBoundary::Completed;
        }
        IterationBoundary::None
    }
}

fn is_claude_installed() -> bool {
    std::process::Command::new("which")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// How long to wait with no output before considering Claude "idle"
const IDLE_TIMEOUT_SECS: u64 = 2;

/// Commands sent to the PTY management task
#[derive(Debug)]
enum PtyCmd {
    /// Send input bytes to the PTY
    Input(Vec<u8>),
    /// Resize the PTY
    Resize { rows: u16, cols: u16 },
}

/// Manages a ralph loop using PTY for full terminal emulation.
/// Auto-restarts when Claude becomes idle (unless paused).
///
/// This implementation uses tokio throughout for proper async cancellation.
pub struct RalphLoop {
    /// The agent's internal directory where PROMPT.md lives
    pub agent_dir: PathBuf,
    /// The target repo root where commands are executed
    pub working_dir: PathBuf,
    /// True for ralph loops (has PROMPT.md, auto-restarts), false for Claude instances
    is_ralph_loop: bool,
    /// Cancellation token for clean shutdown
    cancel_token: CancellationToken,
    /// Handle to the main management task
    task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Channel to send commands (input, resize) to the task
    cmd_tx: Option<mpsc::Sender<PtyCmd>>,
    /// Shared running state (for is_running check)
    running: Arc<AtomicBool>,
    /// Shared paused state
    paused: Arc<AtomicBool>,
}

impl RalphLoop {
    pub fn new(agent_dir: PathBuf, working_dir: PathBuf, is_ralph_loop: bool) -> Self {
        Self {
            agent_dir,
            working_dir,
            is_ralph_loop,
            cancel_token: CancellationToken::new(),
            task_handle: None,
            cmd_tx: None,
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> Option<u32> {
        // PID tracking would require additional state; return None for now
        // The child process is managed internally by the task
        None
    }

    /// Send raw input to the Claude PTY (keyboard input forwarding).
    pub fn send_input(&self, data: &[u8]) -> Result<(), LoopError> {
        let Some(ref tx) = self.cmd_tx else {
            return Err(LoopError::NotRunning);
        };

        // Use try_send for non-blocking send from sync context
        tx.try_send(PtyCmd::Input(data.to_vec()))
            .map_err(|e| LoopError::StdinWriteFailed(e.to_string()))
    }

    /// Resize the PTY to the given dimensions.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), LoopError> {
        let Some(ref tx) = self.cmd_tx else {
            return Ok(()); // Not running, ignore resize
        };

        // Use try_send - if channel is full, resize will be picked up later
        let _ = tx.try_send(PtyCmd::Resize { rows, cols });
        Ok(())
    }

    /// Start the ralph loop with PTY and auto-restart on idle.
    /// Sends raw terminal data through the channel for full TUI rendering.
    /// For Claude instances (is_ralph_loop=false), runs a single session without auto-restart.
    pub fn start(
        &mut self,
        agent_name: String,
        tx: mpsc::Sender<TerminalData>,
        initial_size: (u16, u16),
    ) -> Result<(), LoopError> {
        if self.is_running() {
            return Err(LoopError::AlreadyRunning);
        }

        let path = self.agent_dir.clone();

        if !path.exists() {
            return Err(LoopError::ProjectNotFound(path));
        }

        // Only require PROMPT.md for ralph loops, not Claude instances
        let prompt_content =
            if self.is_ralph_loop {
                let prompt_path = path.join("PROMPT.md");
                if !prompt_path.exists() {
                    return Err(LoopError::PromptNotFound);
                }
                Some(std::fs::read_to_string(&prompt_path).map_err(|e| {
                    LoopError::SpawnFailed(format!("Failed to read PROMPT.md: {}", e))
                })?)
            } else {
                None
            };

        if !is_claude_installed() {
            return Err(LoopError::ClaudeNotFound);
        }

        // Reset state
        self.cancel_token = CancellationToken::new();
        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        // Create command channel (bounded to prevent unbounded memory growth)
        let (cmd_tx, cmd_rx) = mpsc::channel::<PtyCmd>(256);
        self.cmd_tx = Some(cmd_tx);

        // Clone state for the task
        let cancel_token = self.cancel_token.clone();
        let running = self.running.clone();
        let paused = self.paused.clone();
        let agent_dir = self.agent_dir.clone();
        let working_dir = self.working_dir.clone();
        let is_ralph_loop = self.is_ralph_loop;

        // Spawn the main management task
        let handle = tokio::spawn(async move {
            Self::run_loop(
                cancel_token,
                running,
                paused,
                cmd_rx,
                tx,
                agent_dir,
                working_dir,
                prompt_content,
                agent_name,
                is_ralph_loop,
                initial_size,
            )
            .await;
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// Main loop that manages Claude iterations.
    /// For ralph loops, restarts on idle. For Claude instances, runs once.
    async fn run_loop(
        cancel_token: CancellationToken,
        running: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        mut cmd_rx: mpsc::Receiver<PtyCmd>,
        tx: mpsc::Sender<TerminalData>,
        agent_dir: PathBuf,
        working_dir: PathBuf,
        prompt_content: Option<String>,
        agent_name: String,
        is_ralph_loop: bool,
        initial_size: (u16, u16),
    ) {
        while !cancel_token.is_cancelled() {
            // Wait until not paused before starting a new iteration
            while paused.load(Ordering::SeqCst) && !cancel_token.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            if cancel_token.is_cancelled() {
                break;
            }

            let result = Self::spawn_claude_iteration(
                &cancel_token,
                &paused,
                &mut cmd_rx,
                &tx,
                &agent_dir,
                &working_dir,
                prompt_content.as_deref(),
                &agent_name,
                is_ralph_loop,
                initial_size,
            )
            .await;

            if let Err(e) = result {
                let error_msg = format!("[ERROR] {}\r\n", e);
                let _ = tx
                    .send(TerminalData::new(
                        agent_name.clone(),
                        error_msg.into_bytes(),
                    ))
                    .await;

                // For Claude instances, don't retry on error - just stop
                if !is_ralph_loop {
                    break;
                }

                // Wait before retry
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }

            // For Claude instances, don't auto-restart - exit after single session
            if !is_ralph_loop {
                let _ = tx
                    .send(TerminalData::new(
                        agent_name.clone(),
                        b"\r\n[Exited]\r\n".to_vec(),
                    ))
                    .await;
                break;
            }

            // Ralph loops: announce restart and continue
            if !cancel_token.is_cancelled() && !paused.load(Ordering::SeqCst) {
                let _ = tx
                    .send(TerminalData::new(
                        agent_name.clone(),
                        b"\r\n[Restarting iteration...]\r\n".to_vec(),
                    ))
                    .await;

                tokio::time::sleep(Duration::from_secs(1)).await;

                // Send starting marker to trigger process state transition
                let _ = tx
                    .send(TerminalData::new(
                        agent_name.clone(),
                        b"[Starting...]\r\n".to_vec(),
                    ))
                    .await;
            }
        }

        // Mark as no longer running
        running.store(false, Ordering::SeqCst);
    }

    /// Spawn and manage a single Claude iteration.
    /// Returns when the iteration completes (child exits, idle timeout, or cancellation).
    async fn spawn_claude_iteration(
        cancel_token: &CancellationToken,
        paused: &Arc<AtomicBool>,
        cmd_rx: &mut mpsc::Receiver<PtyCmd>,
        tx: &mpsc::Sender<TerminalData>,
        agent_dir: &Path,
        working_dir: &Path,
        _prompt_content: Option<&str>,
        agent_name: &str,
        is_ralph_loop: bool,
        initial_size: (u16, u16),
    ) -> Result<(), LoopError> {
        // Validate initial size - use defaults if invalid
        let (rows, cols) = if initial_size.0 == 0 || initial_size.1 == 0 {
            (24, 80) // Safe defaults
        } else {
            initial_size
        };

        // Open PTY - returns (pty, pts) tuple in 0.5 API
        let (pty, pts) =
            open_pty().map_err(|e| LoopError::PtyError(format!("Failed to open PTY: {}", e)))?;

        // Resize to initial size
        pty.resize(Size::new(rows, cols)).map_err(|e| {
            LoopError::PtyError(format!("Failed to resize PTY to {}x{}: {}", rows, cols, e))
        })?;

        // Build the command based on agent type
        let cmd_str = if is_ralph_loop {
            let prompt_path = agent_dir.join("PROMPT.md");
            let prompt_path_str = prompt_path.to_string_lossy();
            format!(
                "cat '{}' | claude --dangerously-skip-permissions",
                prompt_path_str
            )
        } else {
            "claude --dangerously-skip-permissions".to_string()
        };

        // Spawn the child process using pty_process::Command
        // Note: spawn() consumes both the Command and Pts in 0.5 API
        let mut child = PtyCommand::new("bash")
            .args(["-c", &cmd_str])
            .current_dir(working_dir)
            .spawn(pts)
            .map_err(|e| LoopError::SpawnFailed(format!("Failed to spawn process: {}", e)))?;

        debug!(name = %agent_name, "PTY process spawned");

        // Split PTY into read and write halves for concurrent access
        let (mut pty_read, mut pty_write) = pty.into_split();

        // Track last activity for idle timeout
        let mut last_activity = std::time::Instant::now();

        // Buffer for reading
        let mut buf = [0u8; 4096];

        loop {
            tokio::select! {
                // Priority 1: Check for cancellation
                _ = cancel_token.cancelled() => {
                    // Clean shutdown: kill child and exit
                    let _ = child.kill().await;
                    break;
                }

                // Priority 2: Handle commands from main thread (input, resize)
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        PtyCmd::Input(data) => {
                            if let Err(e) = pty_write.write_all(&data).await {
                                // Write failed, likely PTY closed
                                let _ = tx.send(TerminalData::new(
                                    agent_name.to_string(),
                                    format!("\r\n[Write error: {}]\r\n", e).into_bytes(),
                                )).await;
                            } else {
                                let _ = pty_write.flush().await;
                                // Update activity on input
                                last_activity = std::time::Instant::now();
                            }
                        }
                        PtyCmd::Resize { rows, cols } => {
                            // OwnedWritePty has a resize method
                            let _ = pty_write.resize(Size::new(rows, cols));
                        }
                    }
                }

                // Priority 3: Read output from PTY
                result = pty_read.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            // EOF - child closed the PTY
                            let _ = tx.send(TerminalData::new(
                                agent_name.to_string(),
                                b"\r\n[Process exited]\r\n".to_vec(),
                            )).await;
                            break;
                        }
                        Ok(n) => {
                            // Update activity timestamp
                            last_activity = std::time::Instant::now();

                            // Send data to UI
                            let _ = tx.send(TerminalData::new(
                                agent_name.to_string(),
                                buf[..n].to_vec(),
                            )).await;
                        }
                        Err(e) => {
                            // Read error - PTY likely closed
                            let _ = tx.send(TerminalData::new(
                                agent_name.to_string(),
                                format!("\r\n[Read error: {}]\r\n", e).into_bytes(),
                            )).await;
                            break;
                        }
                    }
                }

                // Priority 4: Periodic checks (child status, idle timeout)
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Check if child has exited
                    if let Ok(Some(status)) = child.try_wait() {
                        let _ = tx.send(TerminalData::new(
                            agent_name.to_string(),
                            format!("\r\n[Process exited with status: {}]\r\n", status).into_bytes(),
                        )).await;
                        break;
                    }

                    // Only apply idle timeout for ralph loops, not Claude instances
                    if is_ralph_loop {
                        let idle_duration = last_activity.elapsed();
                        if !paused.load(Ordering::SeqCst)
                            && idle_duration > Duration::from_secs(IDLE_TIMEOUT_SECS)
                        {
                            let _ = tx.send(TerminalData::new(
                                agent_name.to_string(),
                                format!("\r\n[Idle for {}s, restarting...]\r\n", IDLE_TIMEOUT_SECS)
                                    .into_bytes(),
                            )).await;
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
            }
        }

        // Ensure child is cleaned up
        let exit_status = child.wait().await;
        debug!(name = %agent_name, status = ?exit_status, "PTY process exited");

        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), LoopError> {
        if !self.is_running() {
            return Err(LoopError::NotRunning);
        }
        self.paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), LoopError> {
        if !self.is_running() {
            return Err(LoopError::NotRunning);
        }

        self.paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Signal cancellation - this immediately unblocks the select! in the task
        self.cancel_token.cancel();
        self.paused.store(false, Ordering::SeqCst);

        // Drop the command channel to signal the task
        self.cmd_tx = None;

        // Wait for the task to complete (with timeout for safety)
        if let Some(handle) = self.task_handle.take() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(result) => {
                    if let Err(e) = result {
                        // Task panicked - log but don't propagate
                        error!(error = ?e, "loop task panicked");
                    }
                }
                Err(_) => {
                    // Timeout - task didn't exit in time
                    // This shouldn't happen with proper cancellation, but handle it gracefully
                    warn!("loop task did not exit within timeout");
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
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
    }

    #[test]
    fn test_detect_done_message() {
        let detector = IterationDetector::new();
        assert_eq!(
            detector.check_line("I'm done with task 7.1"),
            IterationBoundary::Done
        );
    }
}
