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

    /// Called each frame to update application state
    ///
    /// Updates running agents' mock output periodically (every ~2 seconds)
    pub fn tick(&mut self) {
        self.frame_count += 1;

        // Update mock output approximately every 2 seconds (120 frames at 60 FPS)
        // Only check if enough time has passed since last tick
        if self.last_tick.elapsed().as_millis() >= 2000 {
            self.last_tick = Instant::now();

            // Update all running agents
            for agent in &mut self.agents {
                if agent.status == AgentStatus::Running {
                    agent.add_next_mock_output();
                }
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
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Char('q') => {
                self.running = false;
                true
            }
            KeyCode::Char('c') if modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                self.running = false;
                true
            }
            // Navigation keybindings
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
            // Other keybindings will be added in subsequent tasks
            _ => false,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
