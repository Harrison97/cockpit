#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use regex::Regex;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;

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
    pub timestamp: Instant,
}

#[allow(dead_code)]
impl OutputLine {
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

/// Manages a ralph loop using PTY for full terminal emulation.
/// Auto-restarts when Claude becomes idle (unless paused).
pub struct RalphLoop {
    /// The agent's internal directory where PROMPT.md lives
    pub project_path: PathBuf,
    /// The target repo root where commands are executed
    pub working_dir: PathBuf,
    /// True for ralph loops (has PROMPT.md, auto-restarts), false for Claude instances
    is_ralph_loop: bool,
    pid: Option<u32>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    last_activity: Arc<Mutex<Instant>>,
    /// Writer to send input to Claude's PTY
    pty_writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

impl RalphLoop {
    pub fn new(project_path: PathBuf, working_dir: PathBuf, is_ralph_loop: bool) -> Self {
        Self {
            project_path,
            working_dir,
            is_ralph_loop,
            pid: None,
            paused: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            pty_writer: Arc::new(Mutex::new(None)),
            reader_handle: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Send raw input to the Claude PTY (keyboard input forwarding).
    pub fn send_input(&self, data: &[u8]) -> Result<(), LoopError> {
        let mut writer_guard = self
            .pty_writer
            .lock()
            .map_err(|e| LoopError::StdinWriteFailed(e.to_string()))?;

        let writer = writer_guard.as_mut().ok_or(LoopError::NotRunning)?;

        writer
            .write_all(data)
            .map_err(|e| LoopError::StdinWriteFailed(e.to_string()))?;
        writer
            .flush()
            .map_err(|e| LoopError::StdinWriteFailed(e.to_string()))?;

        // Update activity on input too
        *self.last_activity.lock().unwrap() = Instant::now();

        Ok(())
    }

    /// Start the ralph loop with PTY and auto-restart on idle.
    /// Sends raw terminal data through the channel for full TUI rendering.
    /// For Claude instances (is_ralph_loop=false), runs a single session without auto-restart.
    pub fn start(
        &mut self,
        agent_name: String,
        tx: mpsc::Sender<TerminalData>,
    ) -> Result<(), LoopError> {
        if self.is_running() {
            return Err(LoopError::AlreadyRunning);
        }

        let path = self.project_path.clone();

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

        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        *self.last_activity.lock().unwrap() = Instant::now();

        let running = self.running.clone();
        let paused = self.paused.clone();
        let last_activity = self.last_activity.clone();
        let project_path = self.project_path.clone();
        let working_dir = self.working_dir.clone();
        let pty_writer = self.pty_writer.clone();
        let is_ralph_loop = self.is_ralph_loop;

        let handle = std::thread::spawn(move || {
            Self::run_loop(
                running,
                paused,
                last_activity,
                project_path,
                working_dir,
                prompt_content,
                agent_name,
                tx,
                pty_writer,
                is_ralph_loop,
            );
        });

        self.reader_handle = Some(handle);
        Ok(())
    }

    fn run_loop(
        running: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        last_activity: Arc<Mutex<Instant>>,
        project_path: PathBuf,
        working_dir: PathBuf,
        prompt_content: Option<String>,
        agent_name: String,
        tx: mpsc::Sender<TerminalData>,
        pty_writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
        is_ralph_loop: bool,
    ) {
        while running.load(Ordering::SeqCst) {
            // Wait until not paused before starting a new iteration
            while paused.load(Ordering::SeqCst) && running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }

            if !running.load(Ordering::SeqCst) {
                break;
            }

            let result = Self::spawn_claude_iteration(
                &project_path,
                &working_dir,
                prompt_content.as_deref(),
                &agent_name,
                &tx,
                &running,
                &paused,
                &last_activity,
                &pty_writer,
                is_ralph_loop,
            );

            if let Err(e) = result {
                let error_msg = format!("[ERROR] {}\r\n", e);
                let _ = tx.blocking_send(TerminalData::new(
                    agent_name.clone(),
                    error_msg.into_bytes(),
                ));
                // For Claude instances, don't retry on error - just stop
                if !is_ralph_loop {
                    running.store(false, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }

            // For Claude instances, don't auto-restart - set running to false and exit
            if !is_ralph_loop {
                running.store(false, Ordering::SeqCst);
                let _ = tx.blocking_send(TerminalData::new(
                    agent_name.clone(),
                    b"\r\n[Exited]\r\n".to_vec(),
                ));
                break;
            }

            if running.load(Ordering::SeqCst) && !paused.load(Ordering::SeqCst) {
                let _ = tx.blocking_send(TerminalData::new(
                    agent_name.clone(),
                    b"\r\n[Restarting iteration...]\r\n".to_vec(),
                ));
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }

    fn spawn_claude_iteration(
        project_path: &Path,
        working_dir: &Path,
        prompt_content: Option<&str>,
        agent_name: &str,
        tx: &mpsc::Sender<TerminalData>,
        running: &Arc<AtomicBool>,
        paused: &Arc<AtomicBool>,
        last_activity: &Arc<Mutex<Instant>>,
        pty_writer: &Arc<Mutex<Option<Box<dyn Write + Send>>>>,
        is_ralph_loop: bool,
    ) -> Result<(), LoopError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 180,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| LoopError::PtyError(e.to_string()))?;

        // Build the command based on agent type
        let cmd_str = if is_ralph_loop {
            // Ralph loop: pipe PROMPT.md to claude
            let prompt_path = project_path.join("PROMPT.md");
            let prompt_path_str = prompt_path.to_string_lossy();
            format!(
                "cat '{}' | claude --dangerously-skip-permissions",
                prompt_path_str
            )
        } else {
            // Claude instance: run claude directly without piping
            "claude --dangerously-skip-permissions".to_string()
        };

        let mut cmd = CommandBuilder::new("bash");
        cmd.arg("-c");
        cmd.arg(&cmd_str);
        cmd.cwd(working_dir);

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| LoopError::SpawnFailed(e.to_string()))?;

        // Drop the slave to avoid blocking
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| LoopError::PtyError(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| LoopError::PtyError(e.to_string()))?;

        // Store writer for input forwarding
        *pty_writer.lock().unwrap() = Some(writer);

        // prompt_content was already validated but we pipe from file directly for ralph loops
        let _ = prompt_content;

        *last_activity.lock().unwrap() = Instant::now();

        // Read in a separate thread so we can check child status without blocking
        let (read_tx, read_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let reader_running = Arc::new(AtomicBool::new(true));
        let reader_running_clone = reader_running.clone();

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while reader_running_clone.load(Ordering::SeqCst) {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if read_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Main loop: check for data, child exit, idle timeout
        loop {
            if !running.load(Ordering::SeqCst) {
                let _ = child.kill();
                break;
            }

            // Check if child has exited
            if let Ok(Some(_status)) = child.try_wait() {
                // Drain any remaining output
                while let Ok(data) = read_rx.try_recv() {
                    let _ = tx.blocking_send(TerminalData::new(agent_name.to_string(), data));
                }
                let _ = tx.blocking_send(TerminalData::new(
                    agent_name.to_string(),
                    b"\r\n[Process exited]\r\n".to_vec(),
                ));
                break;
            }

            // Only apply idle timeout for ralph loops, not Claude instances
            if is_ralph_loop {
                let idle_duration = last_activity.lock().unwrap().elapsed();
                if !paused.load(Ordering::SeqCst)
                    && idle_duration > Duration::from_secs(IDLE_TIMEOUT_SECS)
                {
                    let _ = tx.blocking_send(TerminalData::new(
                        agent_name.to_string(),
                        format!("\r\n[Idle for {}s, restarting...]\r\n", IDLE_TIMEOUT_SECS)
                            .into_bytes(),
                    ));
                    let _ = child.kill();
                    break;
                }
            }

            // Check for data with timeout
            match read_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(data) => {
                    *last_activity.lock().unwrap() = Instant::now();
                    let _ = tx.blocking_send(TerminalData::new(agent_name.to_string(), data));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No data, continue checking child status
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Reader thread exited
                    break;
                }
            }
        }

        // Stop reader thread
        reader_running.store(false, Ordering::SeqCst);
        let _ = reader_thread.join();

        // Clear writer
        *pty_writer.lock().unwrap() = None;

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
        // Reset activity timer so we don't immediately trigger idle timeout
        *self.last_activity.lock().unwrap() = Instant::now();

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.running.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        // Clear writer to unblock any writes
        *self.pty_writer.lock().unwrap() = None;

        if let Some(pid) = self.pid.take() {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        }

        if let Some(handle) = self.reader_handle.take() {
            // Don't block forever waiting for thread
            let _ = handle.join();
        }

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
