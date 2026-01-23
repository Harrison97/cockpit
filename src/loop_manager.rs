use std::path::PathBuf;
use std::time::Instant;

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
}
