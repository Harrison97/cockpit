# System Inventory

Last updated: 2026-01-23

## Modules

| File             | Lines | Primary Responsibility                              |
| ---------------- | ----- | --------------------------------------------------- |
| main.rs          | 164   | Entry point, event loop, terminal setup, panic hook |
| app.rs           | 1188  | Application state, input handling, search mode      |
| agent.rs         | 847   | Agent struct, terminal buffer, lifecycle management |
| loop_manager.rs  | 699   | PTY spawning, process control, output capture       |
| ui.rs            | 972   | Ratatui rendering, search highlighting              |
| persistence.rs   | 141   | JSON state serialization/deserialization            |
| project.rs       | 185   | Ralph project file operations                       |

**Total: 4196 lines**

## External Dependencies

| Crate        | Version | Purpose                                          |
| ------------ | ------- | ------------------------------------------------ |
| tokio        | 1.49    | Async runtime for subprocess I/O                 |
| ratatui      | 0.29    | TUI framework for terminal UI rendering          |
| crossterm    | 0.28    | Terminal backend for input/output                |
| portable-pty | 0.9     | Cross-platform PTY management                    |
| vt100        | 0.15    | Terminal emulation for embedded terminal display |
| tui-term     | 0.2     | Terminal widget for ratatui                      |
| nix          | 0.29    | Unix signal handling (SIGSTOP/SIGCONT/SIGTERM)   |
| serde        | 1.0     | Serialization framework                          |
| serde_json   | 1.0     | JSON serialization for state persistence         |
| chrono       | 0.4     | Date/time formatting for logs and UI             |
| directories  | 5.0     | Cross-platform data directory resolution         |
| regex        | 1.12    | Pattern matching for iteration detection         |
| thiserror    | 1.0     | Error type derivation                            |
| rand         | 0.8     | Random number generation                         |

## Resource Limits

| Resource          | Limit         | Location                | Notes                            |
| ----------------- | ------------- | ----------------------- | -------------------------------- |
| Scrollback buffer | 100,000 lines | agent.rs:105            | SCROLLBACK_SIZE constant         |
| Search matches    | 10,000        | agent.rs:107            | MAX_SEARCH_MATCHES constant      |
| History file load | 1 MB          | agent.rs:296            | MAX_HISTORY_BYTES constant       |
| Output channel    | 1,000 msgs    | app.rs:36               | OUTPUT_CHANNEL_SIZE constant     |
| PTY read buffer   | 4 KB          | loop_manager.rs:488     | Per-read chunk size              |
| Idle timeout      | 2 seconds     | loop_manager.rs:144     | Before auto-restart for loops    |
| Process kill wait | 2 seconds     | loop_manager.rs:513-516 | Grace period before SIGKILL      |
| Thread join wait  | 2-3 seconds   | loop_manager.rs:593-604 | Before abandoning reader thread  |

## Key Data Flows

### Terminal Output Flow
1. PTY reader thread reads 4KB chunks (loop_manager.rs:488-499)
2. Data sent via std::sync::mpsc channel to main thread
3. Main thread forwards to agent via tokio::mpsc (app.rs:367-375)
4. Agent processes through vt100 parser (agent.rs:365-391)
5. History appended to disk file (agent.rs:356-359)

### Input Flow
1. crossterm captures key events (main.rs:118-124)
2. App.handle_key() routes based on focus state (app.rs:706-863)
3. If terminal focused, input forwarded to PTY (app.rs:776-784)
4. RalphLoop.send_input() writes to PTY writer (loop_manager.rs:191-212)

### State Persistence Flow
1. State changes trigger App.save_state() (app.rs:1059-1077)
2. PersistedState serialized to JSON (persistence.rs:74-100)
3. Atomic write: temp file then rename (persistence.rs:95-97)
4. State loaded on startup (app.rs:1101-1132)

## Thread Model

| Thread         | Created By                  | Purpose                        | Shutdown Mechanism              |
| -------------- | --------------------------- | ------------------------------ | ------------------------------- |
| Main           | tokio::main                 | Event loop, rendering          | App.running flag                |
| Reader         | loop_manager.rs:487-500     | Read PTY output                | reader_running AtomicBool       |
| Run loop       | loop_manager.rs:284-299     | Manage Claude iterations       | running AtomicBool              |
| Signal handler | main.rs:51-64               | Catch SIGINT/SIGTERM           | SHUTDOWN_REQUESTED AtomicBool   |

## Mutex Inventory

| Mutex                        | Location              | Purpose                           |
| ---------------------------- | --------------------- | --------------------------------- |
| terminal (Arc<Mutex>)        | agent.rs:171          | VT100 parser state                |
| pty_writer (Arc<Mutex>)      | loop_manager.rs:160   | PTY writer for input              |
| pty_master (Arc<Mutex>)      | loop_manager.rs:162   | PTY master for resize             |
| last_activity (Arc<Mutex>)   | loop_manager.rs:158   | Idle timeout tracking             |

All mutex accesses handle poisoning gracefully using `into_inner()` pattern.

## File Locations

| Purpose          | Path                              | Format     |
| ---------------- | --------------------------------- | ---------- |
| State file       | ~/.local/share/cockpit/state.json | JSON       |
| History logs     | {project}/.agents/{name}/history.log | Raw bytes |
| Prompt file      | {project}/.agents/{name}/PROMPT.md | Markdown  |
| Plan file        | {project}/.agents/{name}/IMPLEMENTATION_PLAN.md | Markdown |
| Instructions     | {project}/.agents/{name}/PRIORITY_INSTRUCTIONS.md | Markdown |

## Known Limitations

1. **Reader thread abandonment**: If PTY reader doesn't exit within 2s on shutdown, thread is abandoned (will be cleaned up eventually when read returns)

2. **No process group signals**: portable_pty handles process lifecycle, not raw process groups with signal propagation

3. **History not rotated**: History files grow unbounded on disk (not loaded, just appended)

4. **Single-terminal rendering**: vt100 can only render up to terminal_height lines of scrollback at a time, requiring chunk-based search
