# Loop Manager Specification

## Overview

The loop manager handles spawning, monitoring, and controlling subprocesses. It supports two modes:

1. **Ralph Loop** - Has PROMPT.md, runs in a loop, auto-restarts on idle, can be paused
2. **Claude Instance** - No prompt, runs once, no auto-restart, cannot be paused

## RalphLoop Struct

```rust
pub struct RalphLoop {
    /// Path to the ralph project directory
    pub project_path: PathBuf,

    /// The target repo root where commands are executed
    pub working_dir: PathBuf,

    /// Whether this is a ralph loop (true) or claude instance (false)
    is_ralph_loop: bool,

    /// The running bash process (if any)
    child: Option<tokio::process::Child>,

    /// Process ID for signal handling
    pid: Option<u32>,

    /// Process group ID for killing child processes
    pgid: Option<i32>,
}
```

## Methods

### `new(project_path: PathBuf, working_dir: PathBuf, is_ralph_loop: bool) -> Self`

Create a new RalphLoop for a project directory. Does not start the loop.

- `is_ralph_loop`: true if PROMPT.md exists, false for claude instance

### `start(&mut self, tx: mpsc::Sender<OutputLine>) -> Result<()>`

Start the subprocess:

**For Ralph Loops (is_ralph_loop = true):**
1. Read PROMPT.md content
2. Spawn bash with command:
   ```bash
   cat '{prompt_path}' | claude --dangerously-skip-permissions
   ```
3. Run in a loop: when process exits or becomes idle, auto-restart
4. On idle timeout (2s no output), kill and restart

**For Claude Instances (is_ralph_loop = false):**
1. Spawn bash with command:
   ```bash
   claude --dangerously-skip-permissions
   ```
2. Run once - NO auto-restart
3. When process exits, set running to false
4. Agent status should be updated to Stopped

Common configuration:
- stdout: piped for capture via PTY
- stderr: merged with stdout
- process_group(0): new process group for signal handling

Store Child handle and extract PID/PGID. Spawn async task to read stdout and send to channel.

### `stop(&mut self) -> Result<()>`

Stop the ralph loop:

1. Send SIGTERM to process group (kills bash and all children including claude)
2. Wait up to 5 seconds for graceful shutdown
3. If still running, send SIGKILL
4. Clear child handle

### `pause(&mut self) -> Result<()>`

Suspend the ralph loop:

1. **Only for Ralph Loops** - returns error for Claude instances
2. Send SIGSTOP to process group
3. All processes freeze (including claude)
4. Set paused flag to prevent auto-restart

### `resume(&mut self) -> Result<()>`

Resume a paused ralph loop:

1. **Only for Ralph Loops** - returns error for Claude instances
2. Send SIGCONT to process group
3. Processes continue from where they stopped
4. Reset activity timer to prevent immediate idle timeout

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

## Claude Instance Mode

When an agent is created without a PROMPT.md (empty prompt during creation):

1. **Label**: Display as "Claude Instance" in the UI (not a ralph loop)
2. **No pause**: Pause is not allowed - return error if attempted
3. **No auto-restart**: When claude exits or becomes idle, do NOT restart
4. **Process exit detection**: When the subprocess exits:
   - Detect via the output channel or process status check
   - Automatically set agent status to `Stopped`
   - Display "[Exited]" message in terminal
   - Do NOT show "[Restarting...]" message

### User-Initiated Kill Detection

When the user kills claude from within the terminal (e.g., Ctrl+C, `/exit`):
- Detect process exit
- Set agent status to `Stopped`
- This is treated the same as if the user pressed `s` to stop

### Implementation Notes

Track agent type in the Agent struct:
```rust
pub enum AgentType {
    RalphLoop,      // Has PROMPT.md, loops, can pause
    ClaudeInstance, // No prompt, one-shot, no pause
}

pub struct Agent {
    // ...
    pub agent_type: AgentType,
}
```

When creating an agent:
- If prompt is empty/None -> `AgentType::ClaudeInstance`
- If prompt has content -> `AgentType::RalphLoop`
