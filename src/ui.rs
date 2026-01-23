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
use crate::app::{App, InputMode};

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
/// - Line 2: Status dot + status text + PID (if running)
/// - Line 3: Uptime
/// - Line 4: Loop count (only for running/paused agents)
/// - Line 5: Project path (truncated, only for agents with projects)
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

        // Line 2: Status dot + status text + PID (if running)
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

        // Include PID if the agent has a running subprocess
        let pid_suffix = agent
            .pid()
            .map(|pid| format!(" [{}]", pid))
            .unwrap_or_default();
        let status_line_content = format!("  {} {}{}", status_dot, agent.status, pid_suffix);
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
                Span::styled(
                    format!(" {}{}", agent.status, pid_suffix),
                    status_text_style,
                ),
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

        // Line 5: Project path (truncated to fit, only for agents with projects)
        if let Some(ref project_path) = agent.project_path {
            // Truncate path to fit in the available width, minus the "  " prefix
            let max_path_len = (inner_area.width as usize).saturating_sub(2);
            let path_str = project_path.to_string_lossy();
            let truncated_path = if path_str.len() > max_path_len {
                // Show the end of the path (most relevant part)
                format!("…{}", &path_str[path_str.len() - max_path_len + 1..])
            } else {
                path_str.to_string()
            };
            let path_text = format!("  {}", truncated_path);
            let path_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            if is_selected {
                let padded_path =
                    format!("{:width$}", path_text, width = inner_area.width as usize);
                lines.push(Line::from(Span::styled(padded_path, path_style)));
            } else {
                lines.push(Line::from(Span::styled(path_text, path_style)));
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
        // Check if this is a separator line (iteration boundary)
        if output_line.starts_with("─────") {
            // Render separator line in dim style, centered without timestamp
            let line = Line::from(Span::styled(
                output_line.as_str(),
                Style::default().fg(Color::DarkGray),
            ));
            lines.push(line);
            continue;
        }

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

/// Renders the help screen overlay
///
/// Shows all keybindings in a centered popup.
fn render_help(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;

    // Calculate centered area for help popup
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 20.min(area.height.saturating_sub(4));
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Help content
    let help_lines = vec![
        Line::from(Span::styled(
            "Keybindings",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Navigation",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  j/↓     Move down"),
        Line::from("  k/↑     Move up"),
        Line::from("  g       Go to first"),
        Line::from("  G       Go to last"),
        Line::from("  Enter   Toggle output focus"),
        Line::from(""),
        Line::from(Span::styled(
            "Agent Control",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  S       Start selected"),
        Line::from("  s       Stop selected"),
        Line::from("  p       Pause selected"),
        Line::from("  r       Resume selected"),
        Line::from(""),
        Line::from(Span::styled(
            "Loop Management",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  n       New loop"),
        Line::from("  i       Send instruction"),
        Line::from(""),
        Line::from(Span::styled(
            "Output (when focused)",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  j/k     Scroll up/down"),
        Line::from("  Ctrl+u  Page up"),
        Line::from("  Ctrl+d  Page down"),
        Line::from("  Esc     Return to list"),
        Line::from(""),
        Line::from(Span::styled(
            "Application",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("  ?       Show this help"),
        Line::from("  q       Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .title(Span::styled(
            "Help",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(help_lines).block(block);
    frame.render_widget(paragraph, popup_area);
}

/// Renders the footer with keybinding hints or input prompt
///
/// Shows different content based on input mode and focus state:
/// - Input mode: Shows input prompt and buffer
/// - Agent list focused: "j/k: navigate  n: new  Enter: focus  p: pause  r: resume  q: quit"
/// - Output focused: "j/k: scroll  Esc: back  Ctrl+d/u: page  q: quit"
fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    match app.input_mode {
        InputMode::Normal => {
            // Show status message if present, otherwise keybindings
            let content = if let Some(ref msg) = app.status_message {
                format!(" {}", msg)
            } else if app.output_focused {
                " j/k: scroll  Esc: back  Ctrl+d/u: page  ?: help  q: quit".to_string()
            } else {
                " j/k: navigate  n: new  i: instruct  p: pause  r: resume  ?: help  q: quit"
                    .to_string()
            };

            let style = if app.status_message.is_some() {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let footer = Paragraph::new(content).style(style);
            frame.render_widget(footer, area);
        }
        _ => {
            // Input mode: show prompt and input buffer
            let prompt = app.input_prompt();
            let input = &app.input_buffer;

            let line = Line::from(vec![
                Span::styled(
                    format!(" {}", prompt),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(input, Style::default().fg(Color::White)),
                Span::styled(
                    "_",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ]);

            let footer = Paragraph::new(line);
            frame.render_widget(footer, area);
        }
    }
}

/// Renders the complete UI for the God Agent Console
///
/// # Arguments
/// * `frame` - The Ratatui frame to render to
/// * `app` - The application state
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Create main vertical layout: header, main content, footer
    let (header_area, main_area, footer_area) = create_main_layout(area);

    // Create horizontal split for main content: agent list (left) and output (right)
    let (agent_list_area, output_area) = create_content_layout(main_area);

    // Render header
    render_header(frame, header_area);

    // Render agent list
    render_agent_list(frame, agent_list_area, &app.agents, app.selected_index);

    // Render output pane for selected agent
    let selected_agent = app.agents.get(app.selected_index);
    render_output_pane(
        frame,
        output_area,
        selected_agent,
        app.scroll_offset,
        app.output_focused,
    );

    // Render footer with keybinding hints or input prompt
    render_footer(frame, footer_area, app);

    // Render help overlay if showing
    if app.show_help {
        render_help(frame, area);
    }
}
