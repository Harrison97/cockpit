//! Application state for the God Agent Console
//!
//! This module contains the App struct which holds all application state.

#![allow(dead_code)] // Methods will be used as more features are implemented

use crate::agent::{create_mock_agents, Agent, AgentStatus};
use std::time::Instant;

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
    /// Last time we updated agent outputs
    last_tick: Instant,
    /// Frame counter for timing updates
    frame_count: u64,
}

impl App {
    /// Creates a new App with mock agents
    pub fn new() -> Self {
        let agents = create_mock_agents();
        Self {
            agents,
            selected_index: 0,
            scroll_offset: 0,
            output_focused: false,
            running: true,
            last_tick: Instant::now(),
            frame_count: 0,
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
        // Reset scroll offset when changing selection
        self.scroll_offset = 0;
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
        // Reset scroll offset when changing selection
        self.scroll_offset = 0;
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
        // Reset scroll offset when changing selection
        self.scroll_offset = 0;
    }

    /// Selects the last agent in the list
    pub fn select_last(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_index = self.agents.len() - 1;
        // Reset scroll offset when changing selection
        self.scroll_offset = 0;
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

    /// Stops the currently selected agent
    pub fn stop_selected(&mut self) {
        if let Some(agent) = self.selected_agent_mut() {
            agent.stop();
        }
    }

    /// Toggles focus between agent list and output pane
    pub fn toggle_focus(&mut self) {
        self.output_focused = !self.output_focused;
        // Reset scroll offset when focusing output pane
        if self.output_focused {
            // Scroll to bottom when entering focus
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
        }
    }

    /// Scrolls the output pane up by half a page (approximately)
    pub fn page_up(&mut self) {
        // Scroll up by 10 lines (approximate half page)
        self.scroll_offset = self.scroll_offset.saturating_sub(10);
    }

    /// Scrolls the output pane down by half a page (approximately)
    pub fn page_down(&mut self) {
        // Get max scroll based on selected agent's output length
        if let Some(agent) = self.selected_agent() {
            let output_len = agent.output.len();
            self.scroll_offset = (self.scroll_offset + 10).min(output_len);
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
    /// Updates running agents' mock output at randomized intervals (2-5 seconds per agent)
    pub fn tick(&mut self) {
        self.frame_count += 1;
        self.last_tick = Instant::now();

        // Check each agent for output updates (each has its own random timer)
        for agent in &mut self.agents {
            if agent.is_output_due() {
                agent.add_next_mock_output();
            }
        }
    }

    /// Handles a key press event
    ///
    /// Dispatches to the appropriate handler based on key code.
    /// Returns true if the key was handled.
    pub fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

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
                _ => false,
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
