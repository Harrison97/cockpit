//! Application state for the God Agent Console
//!
//! This module contains the App struct which holds all application state.

#![allow(dead_code)] // Methods will be used as more features are implemented

use crate::agent::{create_demo_agents, Agent, AgentStatus};
use crate::loop_manager::OutputLine;
use crate::project::RalphProject;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

/// Input mode determines what kind of input the user is currently providing
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode - regular keybindings work
    #[default]
    Normal,
    /// Entering project path for new loop
    EnteringPath,
    /// Entering prompt content for new loop
    EnteringPrompt,
}

/// Channel buffer size for subprocess output
const OUTPUT_CHANNEL_SIZE: usize = 1000;

/// Main application state
pub struct App {
    /// List of agents being monitored
    pub agents: Vec<Agent>,
    /// Index of the currently selected agent
    pub selected_index: usize,
    /// Scroll offset in the output pane
    pub scroll_offset: usize,
    /// Whether the output pane is focused (vs agent list)
    pub output_focused: bool,
    /// Whether the application is still running
    pub running: bool,
    /// Whether to auto-scroll to bottom when new output arrives
    /// Set to false when user manually scrolls up, true when scrolling to bottom
    pinned_to_bottom: bool,
    /// Last time we updated agent outputs
    last_tick: Instant,
    /// Frame counter for timing updates
    frame_count: u64,
    /// Sender for subprocess output - clone this and pass to RalphLoop::start()
    pub output_tx: mpsc::Sender<OutputLine>,
    /// Receiver for subprocess output - drained in tick()
    output_rx: mpsc::Receiver<OutputLine>,
    /// Current input mode
    pub input_mode: InputMode,
    /// Current input buffer for text entry
    pub input_buffer: String,
    /// Pending project path (set during EnteringPath, used when moving to EnteringPrompt)
    pending_project_path: Option<PathBuf>,
    /// Status message to display (cleared after showing)
    pub status_message: Option<String>,
}

impl App {
    /// Creates a new App with demo agents
    pub fn new() -> Self {
        let agents = create_demo_agents();
        let (output_tx, output_rx) = mpsc::channel(OUTPUT_CHANNEL_SIZE);
        Self {
            agents,
            selected_index: 0,
            scroll_offset: 0,
            output_focused: false,
            running: true,
            pinned_to_bottom: true,
            last_tick: Instant::now(),
            frame_count: 0,
            output_tx,
            output_rx,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            pending_project_path: None,
            status_message: None,
        }
    }

    /// Selects the next agent in the list
    ///
    /// Wraps around to the first agent if at the end.
    pub fn select_next(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.agents.len();
        // Reset scroll and enable auto-scroll when changing selection
        self.scroll_offset = 0;
        self.pinned_to_bottom = true;
    }

    /// Selects the previous agent in the list
    ///
    /// Wraps around to the last agent if at the beginning.
    pub fn select_prev(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.agents.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        // Reset scroll and enable auto-scroll when changing selection
        self.scroll_offset = 0;
        self.pinned_to_bottom = true;
    }

    /// Returns a reference to the currently selected agent
    ///
    /// Returns None if there are no agents.
    pub fn selected_agent(&self) -> Option<&Agent> {
        self.agents.get(self.selected_index)
    }

    /// Returns a mutable reference to the currently selected agent
    ///
    /// Returns None if there are no agents.
    pub fn selected_agent_mut(&mut self) -> Option<&mut Agent> {
        self.agents.get_mut(self.selected_index)
    }

    /// Selects the first agent in the list
    pub fn select_first(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_index = 0;
        // Reset scroll and enable auto-scroll when changing selection
        self.scroll_offset = 0;
        self.pinned_to_bottom = true;
    }

    /// Selects the last agent in the list
    pub fn select_last(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_index = self.agents.len() - 1;
        // Reset scroll and enable auto-scroll when changing selection
        self.scroll_offset = 0;
        self.pinned_to_bottom = true;
    }

    /// Pauses the currently selected agent (if running)
    pub fn pause_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status == AgentStatus::Running {
                agent.pause();
            }
        }
    }

    /// Resumes the currently selected agent (if paused)
    pub fn resume_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.resume();
        }
    }

    /// Starts the currently selected agent (if stopped)
    pub fn start_selected(&mut self) {
        let tx = self.output_tx.clone();
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status == AgentStatus::Stopped {
                // Ignore errors for now - will be properly handled later
                let _ = agent.start(tx);
            }
        }
    }

    /// Stops the currently selected agent
    ///
    /// Uses block_in_place to run the async stop operation from synchronous context.
    pub fn stop_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status != AgentStatus::Stopped {
                // Run the async stop in a blocking context
                // This is safe because we're in a tokio runtime
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    let _ = rt.block_on(agent.stop());
                });
            }
        }
    }

    /// Toggles focus between agent list and output pane
    pub fn toggle_focus(&mut self) {
        self.output_focused = !self.output_focused;
        // Reset scroll and enable auto-scroll when focusing output pane
        if self.output_focused {
            // Scroll to bottom and enable auto-scroll when entering focus
            self.pinned_to_bottom = true;
            self.scroll_to_bottom();
        }
    }

    /// Unfocuses the output pane, returning to agent list
    pub fn unfocus_output(&mut self) {
        self.output_focused = false;
    }

    /// Scrolls the output pane up by one line
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            // User is manually scrolling up, disable auto-scroll
            self.pinned_to_bottom = false;
        }
    }

    /// Scrolls the output pane down by one line
    pub fn scroll_down(&mut self) {
        // Get max scroll based on selected agent's output length
        if let Some(agent) = self.selected_agent() {
            let output_len = agent.output.len();
            // Allow scrolling down but will be clamped in UI
            if self.scroll_offset < output_len {
                self.scroll_offset += 1;
            }
            // If we've scrolled to the bottom (or beyond), re-enable auto-scroll
            if self.scroll_offset >= output_len {
                self.pinned_to_bottom = true;
            }
        }
    }

    /// Scrolls the output pane up by half a page (approximately)
    pub fn page_up(&mut self) {
        // Scroll up by 10 lines (approximate half page)
        let old_offset = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.saturating_sub(10);
        // If we actually scrolled up, disable auto-scroll
        if self.scroll_offset < old_offset {
            self.pinned_to_bottom = false;
        }
    }

    /// Scrolls the output pane down by half a page (approximately)
    pub fn page_down(&mut self) {
        // Get max scroll based on selected agent's output length
        if let Some(agent) = self.selected_agent() {
            let output_len = agent.output.len();
            self.scroll_offset = (self.scroll_offset + 10).min(output_len);
            // If we've scrolled to the bottom (or beyond), re-enable auto-scroll
            if self.scroll_offset >= output_len {
                self.pinned_to_bottom = true;
            }
        }
    }

    /// Scrolls to the bottom of the output
    fn scroll_to_bottom(&mut self) {
        if let Some(agent) = self.selected_agent() {
            // Set scroll offset to show last lines
            // The actual clamping happens in the UI based on visible height
            self.scroll_offset = agent.output.len();
        }
    }

    /// Called each frame to update application state
    ///
    /// Drains the output channel and routes lines to the appropriate agents.
    /// Auto-scrolls to bottom when new output is added if pinned_to_bottom is true.
    pub fn tick(&mut self) {
        self.frame_count += 1;
        self.last_tick = Instant::now();

        // Track if the selected agent's output length changes
        let selected_output_len_before = self
            .agents
            .get(self.selected_index)
            .map(|a| a.output.len())
            .unwrap_or(0);

        // Drain the output channel and route to appropriate agents
        while let Ok(output_line) = self.output_rx.try_recv() {
            // Find the agent with matching name and add the output
            if let Some(agent) = self
                .agents
                .iter_mut()
                .find(|a| a.name == output_line.agent_name)
            {
                agent.add_output(&output_line.line);
            }
        }

        // Check if selected agent got new output
        let selected_output_len_after = self
            .agents
            .get(self.selected_index)
            .map(|a| a.output.len())
            .unwrap_or(0);

        // Auto-scroll to bottom if pinned and new output was added to selected agent
        if self.pinned_to_bottom && selected_output_len_after > selected_output_len_before {
            self.scroll_to_bottom();
        }
    }

    /// Starts the "new loop" flow by entering path input mode
    pub fn start_new_loop(&mut self) {
        self.input_mode = InputMode::EnteringPath;
        self.input_buffer.clear();
        self.pending_project_path = None;
    }

    /// Cancels input mode and returns to normal
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.pending_project_path = None;
    }

    /// Submits the current input based on input mode
    fn submit_input(&mut self) {
        match self.input_mode {
            InputMode::Normal => {}
            InputMode::EnteringPath => {
                // Validate and store the path, move to prompt entry
                let path = PathBuf::from(self.input_buffer.trim());
                if path.as_os_str().is_empty() {
                    self.status_message = Some("Path cannot be empty".to_string());
                    return;
                }
                self.pending_project_path = Some(path);
                self.input_buffer.clear();
                self.input_mode = InputMode::EnteringPrompt;
            }
            InputMode::EnteringPrompt => {
                // Create the project and add agent
                self.create_loop_from_input();
            }
        }
    }

    /// Creates a new loop project and adds it as an agent
    fn create_loop_from_input(&mut self) {
        let Some(project_path) = self.pending_project_path.take() else {
            self.status_message = Some("No project path set".to_string());
            self.cancel_input();
            return;
        };

        // Use default prompt if empty
        let prompt_content = if self.input_buffer.trim().is_empty() {
            default_prompt_content()
        } else {
            self.input_buffer.clone()
        };

        // Create the project
        match RalphProject::create(project_path.clone(), &prompt_content) {
            Ok(_project) => {
                // Derive agent name from directory name
                let name = project_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
                    .to_string();

                // Create agent with project path
                let agent = Agent::with_project(&name, project_path);
                self.agents.push(agent);

                // Select the new agent
                self.selected_index = self.agents.len() - 1;
                self.scroll_offset = 0;
                self.pinned_to_bottom = true;

                self.status_message = Some(format!("Created loop: {}", name));
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to create project: {}", e));
            }
        }

        // Return to normal mode
        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
    }

    /// Returns the prompt text to show for the current input mode
    pub fn input_prompt(&self) -> &str {
        match self.input_mode {
            InputMode::Normal => "",
            InputMode::EnteringPath => "Project path: ",
            InputMode::EnteringPrompt => "Prompt (Enter for default): ",
        }
    }

    /// Handles a key press event
    ///
    /// Dispatches to the appropriate handler based on key code and input mode.
    /// Returns true if the key was handled.
    pub fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Clear status message on any key press
        self.status_message = None;

        // Handle input modes first
        if self.input_mode != InputMode::Normal {
            return self.handle_input_key(code, modifiers);
        }

        // Global keybindings (work regardless of focus state)
        match code {
            KeyCode::Char('q') => {
                self.running = false;
                return true;
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
                return true;
            }
            KeyCode::Enter => {
                self.toggle_focus();
                return true;
            }
            _ => {}
        }

        // Focus-dependent keybindings
        if self.output_focused {
            // Output pane is focused: j/k scroll, Esc unfocuses, Ctrl+d/u page scroll
            match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll_down();
                    true
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_up();
                    true
                }
                KeyCode::Esc => {
                    self.unfocus_output();
                    true
                }
                KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.page_down();
                    true
                }
                KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.page_up();
                    true
                }
                _ => false,
            }
        } else {
            // Agent list is focused: navigation and control keybindings
            match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.select_next();
                    true
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.select_prev();
                    true
                }
                KeyCode::Char('g') => {
                    self.select_first();
                    true
                }
                KeyCode::Char('G') => {
                    self.select_last();
                    true
                }
                // Agent control keybindings
                KeyCode::Char('S') => {
                    self.start_selected();
                    true
                }
                KeyCode::Char('p') => {
                    self.pause_selected();
                    true
                }
                KeyCode::Char('r') => {
                    self.resume_selected();
                    true
                }
                KeyCode::Char('s') => {
                    self.stop_selected();
                    true
                }
                // New loop creation
                KeyCode::Char('n') => {
                    self.start_new_loop();
                    true
                }
                _ => false,
            }
        }
    }

    /// Handles key presses during input mode (text entry)
    fn handle_input_key(
        &mut self,
        code: crossterm::event::KeyCode,
        _modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Enter => {
                self.submit_input();
                true
            }
            KeyCode::Esc => {
                self.cancel_input();
                true
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                true
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                true
            }
            _ => false,
        }
    }
}

/// Returns the default prompt content for new projects
fn default_prompt_content() -> String {
    r#"# Ralph Loop Prompt

You are an autonomous agent running in a loop. Each iteration:

1. Read IMPLEMENTATION_PLAN.md to find the next uncompleted task
2. Implement the task completely
3. Run tests and quality checks
4. Mark the task complete and commit
5. Exit

## Rules

- Complete ONE task per iteration
- Always run `cargo build` and `cargo clippy` before committing
- Keep changes focused and minimal
"#
    .to_string()
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
