# Loop Manager Specification

## Overview

The loop manager handles spawning, monitoring, and controlling ralph loop subprocesses.

## RalphLoop Struct

```rust
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
```

## Methods

### `new(project_path: PathBuf) -> Self`

Create a new RalphLoop for a project directory. Does not start the loop.

### `start(&mut self, tx: mpsc::Sender<OutputLine>) -> Result<()>`

Start the ralph loop subprocess:

1. Spawn bash with command:
   ```bash
   cd {project_path} && while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions 2>&1; sleep 1; done
   ```

2. Configure:
   - stdout: piped for capture
   - stderr: merged with stdout (2>&1)
   - process_group(0): new process group for signal handling

3. Store Child handle and extract PID/PGID

4. Spawn async task to read stdout and send to channel

### `stop(&mut self) -> Result<()>`

Stop the ralph loop:

1. Send SIGTERM to process group (kills bash and all children including claude)
2. Wait up to 5 seconds for graceful shutdown
3. If still running, send SIGKILL
4. Clear child handle

### `pause(&mut self) -> Result<()>`

Suspend the ralph loop:

1. Send SIGSTOP to process group
2. All processes freeze (including claude)

### `resume(&mut self) -> Result<()>`

Resume a paused ralph loop:

1. Send SIGCONT to process group
2. Processes continue from where they stopped

### `is_running(&self) -> bool`

Check if the subprocess is still alive.

## OutputLine Struct

```rust
pub struct OutputLine {
    /// Which agent this output belongs to
    pub agent_name: String,

    /// The actual output text
    pub line: String,

    /// When this line was received
    pub timestamp: Instant,
}
```

## Error Handling

- `spawn` failure: Return error, caller should show message to user
- `kill` failure: Log warning, force kill with SIGKILL
- Process already dead: Return Ok, update state
- Read failure: Close channel, signal termination

## Signal Handling Notes

On macOS, use `nix` crate:

```rust
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

// Send to process group (negative PID)
kill(Pid::from_raw(-pgid), Signal::SIGSTOP)?;
```

Process groups ensure that when we pause/stop the bash loop, we also pause/stop the claude process running inside it.
