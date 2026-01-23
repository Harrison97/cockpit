//! Application state for the God Agent Console

#![allow(dead_code)]

use crate::agent::{Agent, AgentStatus, AgentType};
use crate::loop_manager::TerminalData;
use crate::persistence::{load_state, save_state, LoopState, PersistedState};
use crate::project::RalphProject;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

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

const OUTPUT_CHANNEL_SIZE: usize = 1000;

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
    pending_project_path: Option<PathBuf>,
    pending_agent_name: Option<String>,
    pub status_message: Option<String>,
    pub show_help: bool,
    pub show_delete_confirm: bool,
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
            pending_project_path: None,
            pending_agent_name: None,
            status_message: None,
            show_help: false,
            show_delete_confirm: false,
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

        self.status_message = Some(format!("Removed: {}", name));
        self.save_state();
    }

    pub fn toggle_focus(&mut self) {
        self.output_focused = !self.output_focused;
    }

    pub fn unfocus_output(&mut self) {
        self.output_focused = false;
    }

    /// Called each frame to update application state.
    /// Drains terminal data and routes to appropriate agents.
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
        self.pending_project_path = None;
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.paste_info = None;
        self.pending_project_path = None;
        self.pending_agent_name = None;
    }

    pub fn start_instruction(&mut self) {
        if let Some(agent) = self.selected_agent() {
            if agent.project_path.is_none() {
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
                self.pending_project_path = Some(path);
                self.input_buffer.clear();
                self.paste_info = None;
                self.input_mode = InputMode::EnteringName;
            }
            InputMode::EnteringName => {
                let name = self.input_buffer.trim().to_string();
                if name.is_empty() {
                    if let Some(ref path) = self.pending_project_path {
                        let derived_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unnamed")
                            .to_string();
                        self.pending_agent_name = Some(derived_name);
                    } else {
                        self.pending_agent_name = Some("unnamed".to_string());
                    }
                } else {
                    self.pending_agent_name = Some(name);
                }
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
        let Some(base_path) = self.pending_project_path.take() else {
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

        let project_path = base_path.join(".agents").join(&agent_name);
        let working_dir = base_path.clone();

        match RalphProject::create(project_path.clone(), &prompt_content) {
            Ok(_project) => {
                let agent = Agent::with_project(&agent_name, project_path, working_dir, agent_type);
                self.agents.push(agent);
                self.selected_index = self.agents.len() - 1;
                let type_label = match agent_type {
                    AgentType::ClaudeInstance => "Claude instance",
                    AgentType::RalphLoop => "Ralph loop",
                };
                self.status_message = Some(format!("Created {}: {}", type_label, agent_name));
                self.save_state();
            }
            Err(e) => {
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
                if let Some(ref path) = agent.project_path {
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

        // When output is focused and agent is running or paused, forward keys to PTY
        if self.output_focused {
            if let Some(agent) = self.selected_agent() {
                if agent.status == AgentStatus::Running || agent.status == AgentStatus::Paused {
                    // TAB unfocuses the terminal
                    if code == KeyCode::Tab {
                        self.unfocus_output();
                        return true;
                    }

                    // Forward ALL keys to PTY (including Ctrl+C, Esc)
                    let bytes = key_to_bytes(code, modifiers);
                    if !bytes.is_empty() {
                        self.send_input_to_agent(&bytes);
                        return true;
                    }
                }
            }

            // If not running or unhandled, Tab or Esc unfocuses
            match code {
                KeyCode::Tab | KeyCode::Esc => {
                    self.unfocus_output();
                    true
                }
                _ => false,
            }
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
                KeyCode::Enter | KeyCode::Tab => {
                    self.toggle_focus();
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

    pub fn save_state(&self) {
        let mut state = PersistedState::new();

        for agent in &self.agents {
            if let Some(ref project_path) = agent.project_path {
                state.upsert_loop(LoopState {
                    name: agent.name.clone(),
                    project_path: project_path.clone(),
                    working_dir: agent.working_dir.clone(),
                    last_iteration: agent.iteration,
                    agent_type: agent.agent_type,
                });
            }
        }

        if let Err(e) = save_state(&state) {
            eprintln!("Failed to save state: {}", e);
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
                // For backwards compatibility, derive working_dir from project_path if not set
                let working_dir = loop_state.working_dir.unwrap_or_else(|| {
                    // project_path is .agents/<name>, so working_dir is two levels up
                    loop_state
                        .project_path
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| loop_state.project_path.clone())
                });
                let mut agent = Agent::with_project(
                    &loop_state.name,
                    loop_state.project_path,
                    working_dir,
                    loop_state.agent_type,
                );
                agent.iteration = loop_state.last_iteration;
                agent
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to load state: {}", e);
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
