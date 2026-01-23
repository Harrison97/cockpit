//! Application state for the God Agent Console

#![allow(dead_code)]

use crate::agent::{Agent, AgentStatus, AgentType};
use crate::loop_manager::TerminalData;
use crate::persistence::{get_agents_dir, load_state, save_state, LoopState, PersistedState};
use crate::project::RalphProject;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Input mode determines what kind of input the user is currently providing
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    EnteringPath,
    EnteringName,
    EnteringPrompt,
    EnteringInstruction,
}

/// Search mode for Ctrl+F terminal search
#[derive(Clone, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Search is off
    #[default]
    Off,
    /// User is typing a search query
    Searching(String),
    /// Search complete, user is navigating through matches (stores the query)
    Navigating(String),
}

const OUTPUT_CHANNEL_SIZE: usize = 1000;

/// Lines per agent entry in the list (name, status, info, type, path + separator)
pub const LINES_PER_AGENT: usize = 6;

/// Main application state
pub struct App {
    pub agents: Vec<Agent>,
    pub selected_index: usize,
    pub output_focused: bool,
    pub running: bool,
    last_tick: Instant,
    frame_count: u64,
    /// Sender for terminal data - clone this and pass to RalphLoop::start()
    pub terminal_tx: mpsc::Sender<TerminalData>,
    /// Receiver for terminal data - drained in tick()
    terminal_rx: mpsc::Receiver<TerminalData>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    /// If Some, input contains a paste - stores (char_count, line_count)
    pub paste_info: Option<(usize, usize)>,
    pending_agent_dir: Option<PathBuf>,
    pending_agent_name: Option<String>,
    pub status_message: Option<String>,
    pub show_help: bool,
    pub show_delete_confirm: bool,
    /// Scroll offset for agent list (in number of agents)
    pub list_scroll_offset: usize,
    /// Current search mode for terminal Ctrl+F search
    pub search_mode: SearchMode,
    /// Visible search match positions as (line, column) - for highlighting only
    pub search_matches: Vec<(usize, usize)>,
    /// Absolute search match positions as (absolute_row, column, length) - for navigation
    search_matches_absolute: Vec<(usize, usize, usize)>,
    /// Max scrollback value when absolute matches were calculated
    search_max_scrollback: usize,
    /// Index of the currently highlighted match in search_matches_absolute
    pub search_current: usize,
}

impl App {
    pub fn new() -> Self {
        let agents = load_agents_from_state();
        let (terminal_tx, terminal_rx) = mpsc::channel(OUTPUT_CHANNEL_SIZE);
        Self {
            agents,
            selected_index: 0,
            output_focused: false,
            running: true,
            last_tick: Instant::now(),
            frame_count: 0,
            terminal_tx,
            terminal_rx,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            paste_info: None,
            pending_agent_dir: None,
            pending_agent_name: None,
            status_message: None,
            show_help: false,
            show_delete_confirm: false,
            list_scroll_offset: 0,
            search_mode: SearchMode::Off,
            search_matches: Vec::new(),
            search_matches_absolute: Vec::new(),
            search_max_scrollback: 0,
            search_current: 0,
        }
    }

    pub fn select_next(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.agents.len();
    }

    pub fn select_prev(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.agents.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    /// Ensures the selected agent is visible in the list by adjusting scroll offset.
    /// Call this after changing selected_index with the visible height in lines.
    pub fn ensure_selected_visible(&mut self, visible_lines: usize) {
        if self.agents.is_empty() {
            self.list_scroll_offset = 0;
            return;
        }

        // Calculate how many complete agents fit in the visible area
        // Reserve 1 line for potential scroll indicator at top, 1 at bottom
        let usable_lines = visible_lines.saturating_sub(2);
        let visible_agents = if usable_lines >= LINES_PER_AGENT {
            usable_lines / LINES_PER_AGENT
        } else {
            1 // At minimum show 1 agent
        };

        // If selected is before the visible window, scroll up
        if self.selected_index < self.list_scroll_offset {
            self.list_scroll_offset = self.selected_index;
        }

        // If selected is after the visible window, scroll down
        if self.selected_index >= self.list_scroll_offset + visible_agents {
            self.list_scroll_offset = self.selected_index.saturating_sub(visible_agents - 1);
        }

        // Clamp scroll offset to valid range
        let max_scroll = self.agents.len().saturating_sub(visible_agents);
        self.list_scroll_offset = self.list_scroll_offset.min(max_scroll);
    }

    pub fn selected_agent(&self) -> Option<&Agent> {
        self.agents.get(self.selected_index)
    }

    pub fn selected_agent_mut(&mut self) -> Option<&mut Agent> {
        self.agents.get_mut(self.selected_index)
    }

    pub fn select_first(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = 0;
        }
    }

    pub fn select_last(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.agents.len() - 1;
        }
    }

    pub fn pause_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status == AgentStatus::Running {
                if let Err(e) = agent.pause() {
                    self.status_message = Some(format!("Failed to pause: {}", e));
                } else {
                    self.save_state();
                }
            }
        }
    }

    pub fn resume_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status == AgentStatus::Paused {
                if let Err(e) = agent.resume() {
                    self.status_message = Some(format!("Failed to resume: {}", e));
                } else {
                    self.save_state();
                }
            }
        }
    }

    /// Run or resume the selected agent.
    /// If stopped, starts the agent. If paused, resumes it.
    pub fn run_or_resume_selected(&mut self) {
        if let Some(agent) = self.agents.get(self.selected_index) {
            match agent.status {
                AgentStatus::Stopped => self.start_selected(),
                AgentStatus::Paused => self.resume_selected(),
                AgentStatus::Running => {
                    // Already running, nothing to do
                }
            }
        }
    }

    pub fn start_selected(&mut self) {
        let tx = self.terminal_tx.clone();
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status == AgentStatus::Stopped {
                match agent.start(tx) {
                    Ok(()) => {
                        self.save_state();
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Failed to start: {}", e));
                    }
                }
            }
        }
    }

    pub fn stop_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            if agent.status != AgentStatus::Stopped {
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    let _ = rt.block_on(agent.stop());
                });
            }
        }
        self.save_state();
    }

    pub fn delete_selected(&mut self) {
        if self.agents.is_empty() {
            return;
        }

        if let Some(agent) = self.selected_agent_mut() {
            if agent.status != AgentStatus::Stopped {
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    let _ = rt.block_on(agent.stop());
                });
            }
        }

        let name = self
            .agents
            .get(self.selected_index)
            .map(|a| a.name.clone())
            .unwrap_or_default();

        self.agents.remove(self.selected_index);

        if !self.agents.is_empty() && self.selected_index >= self.agents.len() {
            self.selected_index = self.agents.len() - 1;
        }

        info!(name = %name, "agent deleted");
        self.status_message = Some(format!("Removed: {}", name));
        self.save_state();
    }

    pub fn toggle_focus(&mut self) {
        self.output_focused = !self.output_focused;
    }

    pub fn unfocus_output(&mut self) {
        self.output_focused = false;
    }

    /// Scroll the selected agent's terminal up
    pub fn scroll_terminal_up(&mut self, lines: u16) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.scroll_up(lines);
        }
    }

    /// Scroll the selected agent's terminal down
    pub fn scroll_terminal_down(&mut self, lines: u16) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.scroll_down(lines);
        }
    }

    /// Scroll the selected agent's terminal down by half a page
    pub fn scroll_terminal_half_page_down(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            let half_page = (agent.terminal_height() / 2).max(1);
            agent.scroll_down(half_page);
        }
    }

    /// Scroll the selected agent's terminal up by half a page
    pub fn scroll_terminal_half_page_up(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            let half_page = (agent.terminal_height() / 2).max(1);
            agent.scroll_up(half_page);
        }
    }

    /// Scroll the selected agent's terminal to the top of history
    pub fn scroll_terminal_to_top(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.scroll_to_top();
        }
    }

    /// Scroll the selected agent's terminal to the bottom (live output)
    pub fn scroll_terminal_to_bottom(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.scroll_to_bottom();
        }
    }

    /// Jump to the next search match and scroll to center it
    fn search_next_match(&mut self) {
        if self.search_matches_absolute.is_empty() {
            return;
        }
        // Cycle to next match
        self.search_current = (self.search_current + 1) % self.search_matches_absolute.len();
        self.scroll_to_current_match();
    }

    /// Jump to the previous search match and scroll to center it
    fn search_prev_match(&mut self) {
        if self.search_matches_absolute.is_empty() {
            return;
        }
        // Cycle to previous match
        if self.search_current == 0 {
            self.search_current = self.search_matches_absolute.len() - 1;
        } else {
            self.search_current -= 1;
        }
        self.scroll_to_current_match();
    }

    /// Scroll the terminal to center the current match
    fn scroll_to_current_match(&mut self) {
        if let Some(&(abs_row, _, _)) = self.search_matches_absolute.get(self.search_current) {
            if let Some(agent) = self.selected_agent_mut() {
                agent.scroll_to_absolute_row(abs_row);
            }
            // Update visible matches after scrolling
            self.update_visible_from_absolute();
        }
    }

    /// Called each frame to update application state.
    /// Drains terminal data and routes to appropriate agents.
    /// Also detects when subprocesses have exited and updates agent status.
    pub fn tick(&mut self) {
        self.frame_count += 1;
        self.last_tick = Instant::now();

        // Drain the terminal data channel and route to appropriate agents
        while let Ok(term_data) = self.terminal_rx.try_recv() {
            if let Some(agent) = self
                .agents
                .iter_mut()
                .find(|a| a.name == term_data.agent_name)
            {
                agent.process_terminal_data(&term_data.data);
            }
        }

        // Check for subprocess exits and update agent status accordingly
        let mut status_changed = false;
        for agent in &mut self.agents {
            // If agent status is Running but subprocess is no longer running,
            // the process has exited naturally (not killed by user)
            if agent.status == AgentStatus::Running && !agent.is_subprocess_running() {
                agent.status = AgentStatus::Stopped;
                agent.start_time = None;
                agent.process_state = crate::agent::ProcessState::Stopped;
                status_changed = true;
            }
        }

        // Save state if any agent status changed
        if status_changed {
            self.save_state();
        }

        // Refresh visible matches for highlighting in both search modes
        // This must happen in tick() (right before render) so terminal size is consistent
        match &self.search_mode {
            SearchMode::Searching(query) | SearchMode::Navigating(query) => {
                if !query.is_empty() {
                    let query = query.clone();
                    self.refresh_visible_matches(&query);
                }
            }
            SearchMode::Off => {}
        }
    }

    /// Refresh visible matches for highlighting (used during Searching mode)
    fn refresh_visible_matches(&mut self, query: &str) {
        if let Some(agent) = self.selected_agent() {
            let matches: Vec<(usize, usize)> = agent
                .find_visible_matches(query)
                .into_iter()
                .map(|(line, col, _len)| (line, col))
                .collect();

            self.search_matches = matches;
        }
    }

    /// Get the total count of matches in full history
    pub fn search_matches_absolute_count(&self) -> usize {
        self.search_matches_absolute.len()
    }

    /// Calculate and store all absolute matches for navigation
    fn calculate_absolute_matches(&mut self, query: &str) {
        if let Some(agent) = self.selected_agent() {
            let (matches, max_scroll) = agent.find_all_matches(query);
            self.search_matches_absolute = matches;
            self.search_max_scrollback = max_scroll;

            // Set current to last match (closest to bottom/newest content)
            if !self.search_matches_absolute.is_empty() {
                self.search_current = self.search_matches_absolute.len() - 1;
            } else {
                self.search_current = 0;
            }
        }
    }

    /// Update visible matches by re-searching the current visible content
    /// This is simpler and more reliable than trying to convert absolute positions
    fn update_visible_from_absolute(&mut self) {
        // Get the current search query
        let query = match &self.search_mode {
            SearchMode::Navigating(q) => q.clone(),
            _ => return,
        };

        if let Some(agent) = self.selected_agent() {
            // Just re-search the visible content
            let matches: Vec<(usize, usize)> = agent
                .find_visible_matches(&query)
                .into_iter()
                .map(|(line, col, _len)| (line, col))
                .collect();

            self.search_matches = matches;
        }
    }

    /// Send keyboard input to the focused agent's PTY
    pub fn send_input_to_agent(&mut self, data: &[u8]) {
        if let Some(agent) = self.selected_agent() {
            let _ = agent.send_input(data);
        }
    }

    pub fn start_new_loop(&mut self) {
        self.input_mode = InputMode::EnteringPath;
        self.input_buffer.clear();
        self.paste_info = None;
        self.pending_agent_dir = None;
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.paste_info = None;
        self.pending_agent_dir = None;
        self.pending_agent_name = None;
    }

    pub fn start_instruction(&mut self) {
        if let Some(agent) = self.selected_agent() {
            if agent.agent_dir.is_none() {
                self.status_message = Some("No project path for this agent".to_string());
                return;
            }
        } else {
            self.status_message = Some("No agent selected".to_string());
            return;
        }

        self.input_mode = InputMode::EnteringInstruction;
        self.input_buffer.clear();
        self.paste_info = None;
    }

    fn submit_input(&mut self) {
        match self.input_mode {
            InputMode::Normal => {}
            InputMode::EnteringPath => {
                let path = PathBuf::from(self.input_buffer.trim());
                if path.as_os_str().is_empty() {
                    self.status_message = Some("Path cannot be empty".to_string());
                    return;
                }
                self.pending_agent_dir = Some(path);
                self.input_buffer.clear();
                self.paste_info = None;
                self.input_mode = InputMode::EnteringName;
            }
            InputMode::EnteringName => {
                let name = self.input_buffer.trim().to_string();
                let final_name = if name.is_empty() {
                    if let Some(ref path) = self.pending_agent_dir {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unnamed")
                            .to_string()
                    } else {
                        "unnamed".to_string()
                    }
                } else {
                    name
                };

                // Validate agent name to prevent path traversal attacks
                // Reject names containing path separators or starting with dots
                if final_name.contains('/') || final_name.contains('\\') {
                    self.status_message =
                        Some("Agent name cannot contain path separators (/ or \\)".to_string());
                    return;
                }
                if final_name.starts_with('.') {
                    self.status_message = Some("Agent name cannot start with a dot".to_string());
                    return;
                }
                if final_name == "." || final_name == ".." {
                    self.status_message = Some("Invalid agent name".to_string());
                    return;
                }

                self.pending_agent_name = Some(final_name);
                self.input_buffer.clear();
                self.paste_info = None;
                self.input_mode = InputMode::EnteringPrompt;
            }
            InputMode::EnteringPrompt => {
                self.create_loop_from_input();
            }
            InputMode::EnteringInstruction => {
                self.submit_instruction();
            }
        }
    }

    fn create_loop_from_input(&mut self) {
        let Some(base_path) = self.pending_agent_dir.take() else {
            self.status_message = Some("No project path set".to_string());
            self.cancel_input();
            return;
        };

        let Some(agent_name) = self.pending_agent_name.take() else {
            self.status_message = Some("No agent name set".to_string());
            self.cancel_input();
            return;
        };

        let prompt_content = self.input_buffer.clone();

        // Determine agent type: empty prompt = ClaudeInstance, otherwise RalphLoop
        let agent_type = if prompt_content.trim().is_empty() {
            AgentType::ClaudeInstance
        } else {
            AgentType::RalphLoop
        };

        let agents_dir = get_agents_dir();
        let agent_dir = agents_dir.join(&agent_name);

        // Defense-in-depth: verify the resolved path stays within agents directory
        // This catches any path traversal that might have slipped through name validation
        let resolved_project = agent_dir.canonicalize().unwrap_or_else(|_| {
            // If path doesn't exist yet, check parent exists and construct expected path
            if let Some(parent) = agent_dir.parent() {
                if let Ok(resolved_parent) = parent.canonicalize() {
                    if let Some(name) = agent_dir.file_name() {
                        return resolved_parent.join(name);
                    }
                }
            }
            // Fallback: use the raw path if we can't resolve
            agent_dir.clone()
        });
        if let Ok(resolved_agents) = agents_dir.canonicalize() {
            if !resolved_project.starts_with(&resolved_agents) {
                self.status_message =
                    Some("Invalid project path: would escape agents directory".to_string());
                self.input_buffer.clear();
                self.input_mode = InputMode::Normal;
                return;
            }
        }

        let working_dir = base_path.clone();

        match RalphProject::create(agent_dir.clone(), &prompt_content) {
            Ok(_project) => {
                let agent = Agent::with_project(&agent_name, agent_dir, working_dir, agent_type);
                self.agents.push(agent);
                self.selected_index = self.agents.len() - 1;
                let type_label = match agent_type {
                    AgentType::ClaudeInstance => "Claude instance",
                    AgentType::RalphLoop => "Ralph loop",
                };
                info!(name = %agent_name, agent_type = type_label, "agent created");
                self.status_message = Some(format!("Created {}: {}", type_label, agent_name));
                self.save_state();
            }
            Err(e) => {
                warn!(error = %e, name = %agent_name, "failed to create agent");
                self.status_message = Some(format!("Failed to create project: {}", e));
            }
        }

        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
    }

    fn submit_instruction(&mut self) {
        let instruction_text = self.input_buffer.trim().to_string();

        if instruction_text.is_empty() {
            self.status_message = Some("Instruction cannot be empty".to_string());
            self.input_buffer.clear();
            self.input_mode = InputMode::Normal;
            return;
        }

        // Send directly to the agent's PTY if running
        if let Some(agent) = self.selected_agent() {
            if agent.status == AgentStatus::Running {
                if let Err(e) = agent.send_input(instruction_text.as_bytes()) {
                    self.status_message = Some(format!("Failed to send: {}", e));
                } else {
                    // Also send newline
                    let _ = agent.send_input(b"\n");
                    self.status_message = Some("Instruction sent to Claude".to_string());
                }
            } else {
                // Fall back to writing to file if not running
                if let Some(ref path) = agent.agent_dir {
                    match RalphProject::from_path(path.clone()) {
                        Ok(project) => match project.append_instruction(&instruction_text) {
                            Ok(()) => {
                                self.status_message = Some("Instruction saved to file".to_string());
                            }
                            Err(e) => {
                                self.status_message =
                                    Some(format!("Failed to write instruction: {}", e));
                            }
                        },
                        Err(e) => {
                            self.status_message = Some(format!("Failed to load project: {}", e));
                        }
                    }
                }
            }
        }

        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
    }

    pub fn input_prompt(&self) -> &str {
        match self.input_mode {
            InputMode::Normal => "",
            InputMode::EnteringPath => "Target repo path: ",
            InputMode::EnteringName => "Agent name: ",
            InputMode::EnteringPrompt => "Prompt (optional): ",
            InputMode::EnteringInstruction => "Message to Claude: ",
        }
    }

    /// Handle pasted text (preserves newlines from clipboard)
    pub fn handle_paste(&mut self, text: &str) {
        if self.input_mode != InputMode::Normal {
            // Only normalize line endings, preserve everything else as-is
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");

            // Track paste info for display
            let char_count = normalized.chars().count();
            let line_count = normalized.lines().count().max(1);
            if char_count > 0 {
                self.paste_info = Some((char_count, line_count));
            }

            self.input_buffer.push_str(&normalized);
        } else if self.output_focused {
            // When focused on terminal, send paste to the agent's PTY (raw, no sanitization)
            if let Some(agent) = self.selected_agent() {
                let _ = agent.send_input(text.as_bytes());
            }
        }
    }

    /// Handles a key press event.
    /// When output is focused and agent is running, forwards input to PTY.
    pub fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        self.status_message = None;

        // Handle input modes first
        if self.input_mode != InputMode::Normal {
            return self.handle_input_key(code, modifiers);
        }

        // If help screen is showing, any key dismisses it
        if self.show_help {
            self.show_help = false;
            return true;
        }

        // If delete confirmation is showing, handle y/n
        if self.show_delete_confirm {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.show_delete_confirm = false;
                    self.delete_selected();
                }
                _ => {
                    self.show_delete_confirm = false;
                }
            }
            return true;
        }

        // When output is focused
        if self.output_focused {
            // Handle search mode if active
            if self.search_mode != SearchMode::Off {
                return self.handle_search_key(code, modifiers);
            }

            // TAB unfocuses the terminal
            if code == KeyCode::Tab {
                self.unfocus_output();
                return true;
            }

            // Ctrl+F enters search mode
            if code == KeyCode::Char('f') && modifiers.contains(KeyModifiers::CONTROL) {
                self.search_mode = SearchMode::Searching(String::new());
                self.search_matches.clear();
                self.search_current = 0;
                return true;
            }

            // Shift+Arrow for scrolling (works regardless of agent status)
            if modifiers.contains(KeyModifiers::SHIFT) {
                match code {
                    KeyCode::Up => {
                        self.scroll_terminal_up(5);
                        return true;
                    }
                    KeyCode::Down => {
                        self.scroll_terminal_down(5);
                        return true;
                    }
                    _ => {}
                }
            }

            // Forward keys to PTY only if agent is running or paused
            if let Some(agent) = self.selected_agent() {
                if agent.status == AgentStatus::Running || agent.status == AgentStatus::Paused {
                    // Forward ALL keys to PTY (including Ctrl+C, Esc)
                    let bytes = key_to_bytes(code, modifiers);
                    if !bytes.is_empty() {
                        self.send_input_to_agent(&bytes);
                        return true;
                    }
                }
            }

            // Esc also unfocuses
            if code == KeyCode::Esc {
                self.unfocus_output();
                return true;
            }

            false
        } else {
            // Agent list focused - Ctrl+C quits only when not focused on terminal
            if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                self.running = false;
                return true;
            }

            match code {
                KeyCode::Char('q') => {
                    self.running = false;
                    true
                }
                KeyCode::Char('?') => {
                    self.show_help = true;
                    true
                }
                KeyCode::Char(' ') => {
                    self.toggle_focus();
                    true
                }
                KeyCode::Tab => {
                    self.select_next();
                    true
                }
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
                KeyCode::Char('p') => {
                    self.pause_selected();
                    true
                }
                KeyCode::Char('r') => {
                    self.run_or_resume_selected();
                    true
                }
                KeyCode::Char('s') => {
                    self.stop_selected();
                    true
                }
                KeyCode::Char('n') => {
                    self.start_new_loop();
                    true
                }
                KeyCode::Char('i') => {
                    self.start_instruction();
                    true
                }
                KeyCode::Char('d') => {
                    if !self.agents.is_empty() {
                        self.show_delete_confirm = true;
                    }
                    true
                }
                _ => false,
            }
        }
    }

    fn handle_input_key(
        &mut self,
        code: crossterm::event::KeyCode,
        _modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Enter => {
                // If buffer ends with backslash, replace it with newline (like Claude Code)
                if self.input_buffer.ends_with('\\') {
                    self.input_buffer.pop();
                    self.input_buffer.push('\n');
                } else {
                    // Otherwise, submit the input
                    self.submit_input();
                }
                true
            }
            KeyCode::Esc => {
                self.cancel_input();
                true
            }
            KeyCode::Backspace => {
                // If there's pasted content, delete it all at once
                if self.paste_info.is_some() {
                    self.input_buffer.clear();
                    self.paste_info = None;
                } else {
                    self.input_buffer.pop();
                }
                true
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                true
            }
            _ => false,
        }
    }

    /// Handle key events while in search mode
    fn handle_search_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match &self.search_mode {
            SearchMode::Off => false,
            SearchMode::Searching(query) => {
                match code {
                    KeyCode::Enter => {
                        // Confirm search and switch to navigation mode
                        if !query.is_empty() {
                            let query = query.clone();
                            // Calculate all absolute matches for navigation
                            self.calculate_absolute_matches(&query);
                            // Update visible matches for current scroll position
                            self.update_visible_from_absolute();
                            self.search_mode = SearchMode::Navigating(query);
                        } else {
                            // Empty query - exit search
                            self.exit_search_mode();
                        }
                        true
                    }
                    KeyCode::Esc => {
                        // Cancel search and return to focused mode
                        self.exit_search_mode();
                        true
                    }
                    KeyCode::Backspace => {
                        // Remove last character from query
                        let mut new_query = query.clone();
                        new_query.pop();
                        self.search_mode = SearchMode::Searching(new_query.clone());
                        // Update matches incrementally
                        self.update_search_matches(&new_query);
                        true
                    }
                    KeyCode::Char(c) => {
                        // Add character to query
                        let mut new_query = query.clone();
                        new_query.push(c);
                        self.search_mode = SearchMode::Searching(new_query.clone());
                        // Update matches incrementally
                        self.update_search_matches(&new_query);
                        true
                    }
                    _ => false,
                }
            }
            SearchMode::Navigating(_query) => {
                // Vim-style navigation in search mode
                match code {
                    // Exit search
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.exit_search_mode();
                        true
                    }
                    // Next match
                    KeyCode::Char('n') => {
                        self.search_next_match();
                        true
                    }
                    // Previous match (Shift+N)
                    KeyCode::Char('N') => {
                        self.search_prev_match();
                        true
                    }
                    // Scroll down one line (j or Down arrow)
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.scroll_terminal_down(1);
                        true
                    }
                    // Scroll up one line (k or Up arrow)
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.scroll_terminal_up(1);
                        true
                    }
                    // Half-page down (Ctrl+D)
                    KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll_terminal_half_page_down();
                        true
                    }
                    // Half-page up (Ctrl+U)
                    KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                        self.scroll_terminal_half_page_up();
                        true
                    }
                    // Go to top of history
                    KeyCode::Char('g') => {
                        self.scroll_terminal_to_top();
                        true
                    }
                    // Go to bottom of history (Shift+G)
                    KeyCode::Char('G') => {
                        self.scroll_terminal_to_bottom();
                        true
                    }
                    _ => false,
                }
            }
        }
    }

    /// Update search matches based on current query (used during Searching mode)
    fn update_search_matches(&mut self, query: &str) {
        if query.is_empty() {
            self.search_matches.clear();
            self.search_current = 0;
            return;
        }

        if let Some(agent) = self.selected_agent() {
            // Find visible matches for highlighting during search
            let matches: Vec<(usize, usize)> = agent
                .find_visible_matches(query)
                .into_iter()
                .map(|(line, col, _len)| (line, col))
                .collect();

            self.search_matches = matches;

            // Keep current index in bounds, or set to last match
            if self.search_matches.is_empty() {
                self.search_current = 0;
            } else if self.search_current >= self.search_matches.len() {
                self.search_current = self.search_matches.len() - 1;
            }
        }
    }

    /// Exit search mode and return to normal focused state
    fn exit_search_mode(&mut self) {
        self.search_mode = SearchMode::Off;
        self.search_matches.clear();
        self.search_matches_absolute.clear();
        self.search_max_scrollback = 0;
        self.search_current = 0;
    }

    /// Get the current search query if in search mode
    pub fn search_query(&self) -> Option<&str> {
        match &self.search_mode {
            SearchMode::Searching(query) => Some(query),
            SearchMode::Navigating(query) => Some(query),
            SearchMode::Off => None,
        }
    }

    pub fn save_state(&self) {
        let mut state = PersistedState::new();

        for agent in &self.agents {
            if let Some(ref agent_dir) = agent.agent_dir {
                state.upsert_loop(LoopState {
                    name: agent.name.clone(),
                    agent_dir: agent_dir.clone(),
                    working_dir: agent.working_dir.clone(),
                    last_iteration: agent.iteration,
                    agent_type: agent.agent_type,
                });
            }
        }

        if let Err(e) = save_state(&state) {
            warn!(error = %e, "failed to save state");
        }
    }

    pub fn shutdown(&mut self) {
        self.save_state();

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                for agent in &mut self.agents {
                    if agent.status != AgentStatus::Stopped {
                        let _ = agent.stop().await;
                    }
                }
            });
        });
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn load_agents_from_state() -> Vec<Agent> {
    match load_state() {
        Ok(state) => state
            .loops
            .into_iter()
            .map(|loop_state| {
                // For backwards compatibility, derive working_dir from agent_dir if not set
                let working_dir = loop_state.working_dir.unwrap_or_else(|| {
                    // agent_dir is .agents/<name>, so working_dir is two levels up
                    loop_state
                        .agent_dir
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| loop_state.agent_dir.clone())
                });
                let mut agent = Agent::with_project(
                    &loop_state.name,
                    loop_state.agent_dir,
                    working_dir,
                    loop_state.agent_type,
                );
                agent.iteration = loop_state.last_iteration;
                agent
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to load state");
            Vec::new()
        }
    }
}

/// Convert a key event to bytes to send to the PTY
fn key_to_bytes(
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> Vec<u8> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match code {
        KeyCode::Char(c) => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
                let ctrl_char = (c as u8).wrapping_sub(b'a' - 1);
                if ctrl_char <= 26 {
                    return vec![ctrl_char];
                }
            }
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![27],
        KeyCode::Up => vec![27, b'[', b'A'],
        KeyCode::Down => vec![27, b'[', b'B'],
        KeyCode::Right => vec![27, b'[', b'C'],
        KeyCode::Left => vec![27, b'[', b'D'],
        KeyCode::Home => vec![27, b'[', b'H'],
        KeyCode::End => vec![27, b'[', b'F'],
        KeyCode::PageUp => vec![27, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![27, b'[', b'6', b'~'],
        KeyCode::Delete => vec![27, b'[', b'3', b'~'],
        KeyCode::Insert => vec![27, b'[', b'2', b'~'],
        KeyCode::F(n) => {
            // F1-F4 use different sequences
            match n {
                1 => vec![27, b'O', b'P'],
                2 => vec![27, b'O', b'Q'],
                3 => vec![27, b'O', b'R'],
                4 => vec![27, b'O', b'S'],
                5 => vec![27, b'[', b'1', b'5', b'~'],
                6 => vec![27, b'[', b'1', b'7', b'~'],
                7 => vec![27, b'[', b'1', b'8', b'~'],
                8 => vec![27, b'[', b'1', b'9', b'~'],
                9 => vec![27, b'[', b'2', b'0', b'~'],
                10 => vec![27, b'[', b'2', b'1', b'~'],
                11 => vec![27, b'[', b'2', b'3', b'~'],
                12 => vec![27, b'[', b'2', b'4', b'~'],
                _ => vec![],
            }
        }
        _ => vec![],
    }
}
