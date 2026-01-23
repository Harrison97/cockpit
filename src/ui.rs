//! UI rendering module for the God Agent Console
//!
//! This module handles all TUI rendering using Ratatui.

#![allow(dead_code)] // Functions will be used as more features are implemented

use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::agent::{Agent, AgentStatus};

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
/// Left pane: 20% of width but minimum 18 characters
/// Right pane: remaining space
///
/// Returns a tuple of (agent_list_area, output_pane_area)
fn create_content_layout(area: Rect) -> (Rect, Rect) {
    // Calculate 20% of the area width, but enforce minimum of 18 chars
    let twenty_percent = (area.width * 20) / 100;
    let left_width = twenty_percent.max(18);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width), // Left pane: 20% width, min 18 chars
            Constraint::Fill(1),            // Right pane: remaining space
        ])
        .split(area);

    (chunks[0], chunks[1])
}

/// Renders the agent list in the left pane
///
/// Shows each agent with:
/// - Line 1: Arrow (if selected) + name
/// - Line 2: Status dot + status text
/// - Line 3: Uptime
/// - Line 4: Loop count (only for running/paused agents)
/// - Blank line between agents
fn render_agent_list(frame: &mut Frame, area: Rect, agents: &[Agent], selected_index: usize) {
    let block = Block::default()
        .title(Span::styled(
            "AGENTS",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Build lines for each agent
    let mut lines: Vec<Line> = Vec::new();

    for (i, agent) in agents.iter().enumerate() {
        let is_selected = i == selected_index;

        // Line 1: Arrow (if selected) + name
        let arrow = if is_selected { "→ " } else { "  " };
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let arrow_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::Cyan)
        };

        // Build the name line - pad to fill width for selected highlight
        let name_content = format!("{}{}", arrow, agent.name);
        let padded_name = if is_selected {
            format!("{:width$}", name_content, width = inner_area.width as usize)
        } else {
            name_content
        };

        if is_selected {
            lines.push(Line::from(Span::styled(padded_name, name_style)));
        } else {
            lines.push(Line::from(vec![
                Span::styled(arrow, arrow_style),
                Span::styled(&agent.name, name_style),
            ]));
        }

        // Line 2: Status dot + status text
        let status_dot = match agent.status {
            crate::agent::AgentStatus::Stopped => "○",
            _ => "●",
        };
        let status_color = agent.status.color();
        let status_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(status_color)
        };
        let status_text_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(status_color)
        };

        let status_line_content = format!("  {} {}", status_dot, agent.status);
        if is_selected {
            let padded_status = format!(
                "{:width$}",
                status_line_content,
                width = inner_area.width as usize
            );
            lines.push(Line::from(Span::styled(padded_status, status_style)));
        } else {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(status_dot, status_style),
                Span::styled(format!(" {}", agent.status), status_text_style),
            ]));
        }

        // Line 3: Uptime
        let uptime_text = format!("  Uptime: {}s", agent.uptime_secs());
        let uptime_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if is_selected {
            let padded_uptime =
                format!("{:width$}", uptime_text, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_uptime, uptime_style)));
        } else {
            lines.push(Line::from(Span::styled(uptime_text, uptime_style)));
        }

        // Line 4: Loop count (for running or paused agents)
        if agent.status != crate::agent::AgentStatus::Stopped {
            let loop_text = format!("  Loop #{}", agent.iteration);
            let loop_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            if is_selected {
                let padded_loop =
                    format!("{:width$}", loop_text, width = inner_area.width as usize);
                lines.push(Line::from(Span::styled(padded_loop, loop_style)));
            } else {
                lines.push(Line::from(Span::styled(loop_text, loop_style)));
            }
        }

        // Blank line between agents (but not after the last one)
        if i < agents.len() - 1 {
            lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

/// Renders the output pane showing the selected agent's output
///
/// Shows:
/// - Title: "Agent Output: {agent_name}"
/// - Output lines with timestamps
/// - Scrollable content based on scroll_offset
/// - Brighter border when focused
fn render_output_pane(
    frame: &mut Frame,
    area: Rect,
    agent: Option<&Agent>,
    scroll_offset: usize,
    output_focused: bool,
) {
    // Determine title based on selected agent
    let title = match agent {
        Some(a) => format!("Agent Output: {}", a.name),
        None => "Agent Output: (none)".to_string(),
    };

    // Border and title color is brighter when focused
    let border_color = if output_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title_color = if output_focused {
        Color::Cyan
    } else {
        Color::White
    };

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // If no agent selected, show empty pane
    let agent = match agent {
        Some(a) => a,
        None => return,
    };

    // Build output lines with timestamps
    // For mock purposes, we generate timestamps based on current time minus offset
    let now = Local::now();
    let mut lines: Vec<Line> = Vec::new();

    for (i, output_line) in agent.output.iter().enumerate() {
        // Generate a mock timestamp (decrement by 3 seconds per line from current time)
        let line_offset = (agent.output.len().saturating_sub(i + 1)) * 3;
        let line_time = now - chrono::Duration::seconds(line_offset as i64);
        let timestamp = line_time.format("%H:%M:%S").to_string();

        let line = Line::from(vec![
            Span::styled(
                format!("[{}] ", timestamp),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(output_line, Style::default().fg(Color::White)),
        ]);
        lines.push(line);
    }

    // If agent is running, show a cursor prompt
    if agent.status == AgentStatus::Running {
        let timestamp = now.format("%H:%M:%S").to_string();
        let cursor_line = Line::from(vec![
            Span::styled(
                format!("[{}] ", timestamp),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(">", Style::default().fg(Color::White)),
        ]);
        lines.push(cursor_line);
    }

    // Apply scroll offset - calculate visible range
    let visible_height = inner_area.height as usize;
    let total_lines = lines.len();

    // Clamp scroll offset to valid range
    let max_scroll = total_lines.saturating_sub(visible_height);
    let effective_offset = scroll_offset.min(max_scroll);

    // Get the visible slice of lines
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(effective_offset)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner_area);
}

/// Renders the header with title and timestamp
///
/// Shows "GOD AGENT CONSOLE" on the left (cyan, bold) and current time on the right.
fn render_header(frame: &mut Frame, area: Rect) {
    // Get current time formatted as HH:MM:SS
    let timestamp = Local::now().format("%H:%M:%S").to_string();

    // Create the header line with title on left and timestamp on right
    // Add 1 char left padding for visual alignment with content below
    let title = " GOD AGENT CONSOLE";
    let available_width = area.width as usize;

    // Build the line content: title + padding + timestamp + right padding
    let padding_width = available_width.saturating_sub(title.len() + timestamp.len() + 2);
    let padding = " ".repeat(padding_width);

    let header_line = Line::from(vec![
        Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(padding),
        Span::styled(&timestamp, Style::default().fg(Color::White)),
        Span::raw(" "), // Right padding for symmetry
    ]);

    // Create block with bottom border only
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(header_line).block(block);

    frame.render_widget(paragraph, area);
}

/// Renders the footer with keybinding hints
///
/// Shows different keybindings based on focus state:
/// - Agent list focused: "j/k: navigate  Enter: focus  p: pause  r: resume  q: quit"
/// - Output focused: "j/k: scroll  Esc: back  Ctrl+d/u: page  q: quit"
fn render_footer(frame: &mut Frame, area: Rect, output_focused: bool) {
    let hints = if output_focused {
        " j/k: scroll  Esc: back  Ctrl+d/u: page  q: quit"
    } else {
        " j/k: navigate  Enter: focus  p: pause  r: resume  q: quit"
    };

    let footer = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));

    frame.render_widget(footer, area);
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

    // Render header
    render_header(frame, header_area);

    // Render agent list
    render_agent_list(frame, agent_list_area, agents, selected_index);

    // Render output pane for selected agent
    let selected_agent = agents.get(selected_index);
    render_output_pane(
        frame,
        output_area,
        selected_agent,
        scroll_offset,
        output_focused,
    );

    // Render footer with keybinding hints
    render_footer(frame, footer_area, output_focused);
}
