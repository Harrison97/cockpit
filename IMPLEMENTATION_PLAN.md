# Implementation Plan

## Phase 1: Async Foundation

- [x] **1.1 Add tokio runtime to main**
  - Wrap main function in `#[tokio::main]`
  - Keep existing synchronous crossterm event polling
  - Verify app still runs correctly

- [x] **1.2 Create loop_manager module**
  - Create `src/loop_manager.rs`
  - Add module declaration in main.rs
  - Define empty `RalphLoop` struct placeholder

- [x] **1.3 Create project module**
  - Create `src/project.rs`
  - Add module declaration in main.rs
  - Define empty `RalphProject` struct placeholder

- [x] **1.4 Add new dependencies**
  - Add to Cargo.toml: `nix = { version = "0.29", features = ["signal", "process"] }`
  - Add: `serde = { version = "1", features = ["derive"] }`
  - Add: `serde_json = "1"`
  - Add: `directories = "5"`
  - Run `cargo build` to verify

- [x] **1.5 Implement output channel**
  - Create `tokio::sync::mpsc` channel in App
  - Store sender in App for subprocess use
  - Modify tick() to drain receiver into agent output buffers

## Phase 2: Subprocess Management

- [x] **2.1 Implement RalphLoop struct**
  - Fields: project_path, child (Option<Child>), pid (Option<u32>)
  - Method: `new(project_path: PathBuf) -> Self`
  - Method: `is_running(&self) -> bool`

- [x] **2.2 Implement subprocess spawning**
  - Method: `start(&mut self, tx: mpsc::Sender<OutputLine>) -> Result<()>`
  - Spawn bash with: `cd {path} && while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions 2>&1; sleep 1; done`
  - Configure stdout pipe for capture
  - Store Child handle and PID

- [x] **2.3 Implement async output reader**
  - Spawn tokio task to read stdout lines
  - Send lines to mpsc channel with timestamp
  - Handle process exit gracefully

- [x] **2.4 Implement stop**
  - Method: `stop(&mut self) -> Result<()>`
  - Send SIGTERM to process
  - Wait for cleanup with timeout
  - Clear child handle

- [x] **2.5 Connect RalphLoop to Agent**
  - Add `ralph_loop: Option<RalphLoop>` field to Agent
  - Modify Agent::start() to spawn real subprocess
  - Modify Agent::stop() to kill subprocess
  - Remove mock output generation

## Phase 3: Ralph Project Structure

- [x] **3.1 Implement RalphProject struct**
  - Fields: root, prompt_path, plan_path, specs_dir
  - Method: `from_path(path: PathBuf) -> Result<Self>`
  - Validate required files exist (PROMPT.md)

- [x] **3.2 Implement project detection**
  - Method: `is_ralph_project(path: &Path) -> bool`
  - Check for PROMPT.md existence
  - Optionally check for IMPLEMENTATION_PLAN.md

- [x] **3.3 Implement project creation**
  - Method: `create(path: PathBuf, prompt_content: &str) -> Result<Self>`
  - Create directory structure
  - Write PROMPT.md with user content
  - Create empty IMPLEMENTATION_PLAN.md
  - Create specs/ directory

- [x] **3.4 Add create loop UI**
  - Add keybinding `n` for new loop
  - Prompt for project path
  - Prompt for prompt content (or use default)
  - Create project and add to agent list

## Phase 4: Pause/Resume with Signals

- [x] **4.1 Spawn with process groups**
  - Use `Command::process_group(0)` or pre_exec with setsid
  - Store process group ID
  - Signals will target entire group

- [x] **4.2 Implement pause**
  - Method: `pause(&mut self) -> Result<()>`
  - Send SIGSTOP to process group
  - Update status to Paused

- [x] **4.3 Implement resume**
  - Method: `resume(&mut self) -> Result<()>`
  - Send SIGCONT to process group
  - Update status to Running

- [x] **4.4 Wire up keybindings**
  - `p` calls ralph_loop.pause() then updates agent status
  - `r` calls ralph_loop.resume() then updates agent status
  - Handle errors gracefully

## Phase 5: Instructions/Intervention

- [x] **5.1 Implement instruction appending**
  - Method: `RalphProject::append_instruction(text: &str) -> Result<()>`
  - Create/append to PRIORITY_INSTRUCTIONS.md
  - File will be read by next Claude iteration

- [x] **5.2 Add instruction UI**
  - Add keybinding `i` for instruct
  - Simple line input at bottom of screen
  - Write to project's instruction file
  - Show confirmation message

- [x] **5.3 Implement plan modification**
  - Method: `RalphProject::prepend_task(task: &str) -> Result<()>`
  - Add task to top of IMPLEMENTATION_PLAN.md
  - Useful for priority overrides

## Phase 6: Persistence

- [x] **6.1 Create persistence module**
  - Create `src/persistence.rs`
  - Define state file path: `~/.cockpit/state.json`
  - Define log directory: `~/.cockpit/logs/`

- [x] **6.2 Implement state saving**
  - Struct: `PersistedState { loops: Vec<LoopState> }`
  - Struct: `LoopState { name, project_path, last_iteration }`
  - Save on loop creation and status changes

- [x] **6.3 Implement state loading**
  - Load state file on startup
  - Recreate agents from persisted state
  - All start as Stopped (user must restart)

- [x] **6.4 Implement output logging**
  - Write output to `~/.cockpit/logs/{agent_name}.log`
  - Append mode, include timestamps
  - Load recent history on restart

## Phase 7: Iteration Detection

- [x] **7.1 Parse output for boundaries**
  - Detect patterns like "feat:" commit messages
  - Detect "I'm done with" exit messages
  - Track iteration boundaries

- [x] **7.2 Update iteration counter**
  - Increment agent.iteration on detected boundary
  - Update UI to show real count

- [x] **7.3 Add visual separators**
  - Insert separator line between iterations in output
  - Style: dim line with iteration number

## Phase 8: Polish

- [x] **8.1 Error handling**
  - Show user-friendly error messages
  - Handle spawn failures gracefully
  - Retry logic for transient failures

- [x] **8.2 Resource cleanup**
  - Kill all subprocesses on cockpit exit
  - Clean up zombie processes
  - Save state before exit

- [x] **8.3 UI enhancements**
  - Show PID in agent info
  - Add help screen with `?`
  - Show loop project path

## Phase 9: Claude Instance Mode & UX Improvements

- [x] **9.1 Add AgentType enum**
  - Add `AgentType` enum: `RalphLoop` (has PROMPT.md) vs `ClaudeInstance` (no prompt)
  - Add `agent_type` field to `Agent` struct
  - Update `Agent::with_project()` to accept agent_type parameter
  - Update persistence to save/load agent_type

- [x] **9.2 Support prompt-less agent creation**
  - Modify `RalphProject::create()` to skip PROMPT.md when prompt is empty
  - When prompt is empty, set agent_type to `ClaudeInstance`
  - Update UI labels: show "Claude Instance" for prompt-less agents

- [x] **9.3 Modify RalphLoop for Claude instances**
  - Add `is_ralph_loop: bool` field to `RalphLoop`
  - For Claude instances: spawn `claude --dangerously-skip-permissions` directly (no cat pipe)
  - For Claude instances: do NOT auto-restart on idle timeout
  - When process exits naturally, set `running` to false

- [x] **9.4 Disable pause for Claude instances**
  - `Agent::pause()` returns error for ClaudeInstance type
  - Add `Agent::can_pause()` method
  - Update UI to gray out or hide pause option for Claude instances

- [x] **9.5 Detect process exit and update status**
  - When subprocess exits (not killed by user), detect in tick() or via channel
  - Automatically set agent status to Stopped
  - Show "[Exited]" message in terminal, no restart message

- [x] **9.6 Simplify keybindings**
  - `s` = stop (unchanged)
  - `r` = resume if paused, OR start if stopped (merge S and r)
  - `p` = pause (unchanged)
  - Remove capital `S` keybinding
  - Update footer hints and help screen

## Phase 10: Terminal Interaction Improvements

- [x] **10.1 Add mouse wheel scrolling for terminal pane**
  - Enable mouse capture: `crossterm::event::EnableMouseCapture`
  - When output_focused, handle `MouseEvent::ScrollUp/ScrollDown`
  - Implement scroll offset in terminal rendering
  - Track `scroll_offset` per agent
  - Scroll through vt100 scrollback buffer

- [x] **10.2 Add left pane scrolling for agent list**
  - Track `list_scroll_offset` in App
  - When agent count exceeds visible area, enable scrolling
  - Arrow keys (j/k) already move selection; auto-scroll to keep selected visible
  - Ensure selected agent is always visible in the viewport
  - Show scroll indicators (▲/▼) when more items above/below

## Phase 11: Full Terminal Takeover

- [ ] **11.1 Verify alternate screen usage**
  - Confirm `EnterAlternateScreen` is used in main.rs (already done)
  - Ensure no stdout writes outside of ratatui rendering
  - Test that scrollback is isolated from cockpit UI

- [ ] **11.2 Disable terminal scrollback during run**
  - The alternate screen should already isolate scrollback
  - If needed, add explicit terminal mode settings
  - Test with various terminal emulators (iTerm, Terminal.app, kitty)

## Completion Criteria

All items checked. Application:
- Builds without warnings (`cargo clippy`)
- Can create new ralph projects
- Can start/stop/pause/resume real loops
- Shows real Claude output
- Can send instructions to loops
- Persists state across restarts
- Supports Claude instances (no prompt, no loop)
- Scrollable agent list and terminal pane
