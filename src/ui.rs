//! UI rendering module for the God Agent Console
//!
//! This module handles all TUI rendering using Ratatui.

#![allow(dead_code)] // Functions will be used as more features are implemented

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::agent::Agent;

/// Creates the main vertical layout: header, main content, footer
///
/// Returns a tuple of (header_area, main_area, footer_area)
fn create_main_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header: 3 lines fixed
            Constraint::Min(0),    // Main content: remaining space
            Constraint::Length(1), // Footer: 1 line fixed
        ])
        .split(area);

    (chunks[0], chunks[1], chunks[2])
}

/// Creates the horizontal split for main content: agent list and output pane
///
/// Returns a tuple of (agent_list_area, output_pane_area)
fn create_content_layout(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Left pane: 20% width (agent list)
            Constraint::Percentage(80), // Right pane: 80% width (output)
        ])
        .split(area);

    (chunks[0], chunks[1])
}

/// Renders the complete UI for the God Agent Console
///
/// # Arguments
/// * `frame` - The Ratatui frame to render to
/// * `agents` - Slice of agents to display
/// * `selected_index` - Index of the currently selected agent
/// * `scroll_offset` - Scroll position in the output pane
/// * `output_focused` - Whether the output pane is focused
pub fn render(
    frame: &mut Frame,
    agents: &[Agent],
    selected_index: usize,
    scroll_offset: usize,
    output_focused: bool,
) {
    let area = frame.area();

    // Create main vertical layout: header, main content, footer
    let (header_area, main_area, footer_area) = create_main_layout(area);

    // Create horizontal split for main content: agent list (left) and output (right)
    let (agent_list_area, output_area) = create_content_layout(main_area);

    // TODO: Render header (task 3.3)
    // TODO: Render agent list (task 3.4)
    // TODO: Render output pane (task 3.5)
    // TODO: Render footer (task 3.6)

    // Suppress unused variable warnings for now
    let _ = (
        header_area,
        agent_list_area,
        output_area,
        footer_area,
        agents,
        selected_index,
        scroll_offset,
        output_focused,
    );
}
