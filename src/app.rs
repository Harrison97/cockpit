//! Application state for the God Agent Console
//!
//! This module contains the App struct which holds all application state.

#![allow(dead_code)] // Methods will be used as more features are implemented

use crate::agent::{create_mock_agents, Agent};

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
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
