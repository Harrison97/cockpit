//! UI rendering module for the God Agent Console

#![allow(dead_code)]

use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use tui_term::widget::PseudoTerminal;

use crate::agent::{Agent, AgentStatus};
use crate::app::{App, InputMode};

fn create_main_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    (chunks[0], chunks[1], chunks[2])
}

fn create_content_layout(area: Rect) -> (Rect, Rect) {
    let twenty_percent = (area.width * 20) / 100;
    let left_width = twenty_percent.max(18);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_width), Constraint::Fill(1)])
        .split(area);

    (chunks[0], chunks[1])
}

/// Main draw function - renders the entire UI
pub fn draw(frame: &mut Frame, app: &App) {
    let (header_area, main_area, footer_area) = create_main_layout(frame.area());
    let (agent_list_area, output_pane_area) = create_content_layout(main_area);

    render_header(frame, header_area);
    render_agent_list(frame, agent_list_area, &app.agents, app.selected_index);
    render_terminal_pane(
        frame,
        output_pane_area,
        app.selected_agent(),
        app.output_focused,
    );
    render_footer(frame, footer_area, app);

    // Render input box if in input mode
    if app.input_mode != InputMode::Normal {
        render_input_box(frame, app);
    }

    // Render help screen if showing
    if app.show_help {
        render_help_screen(frame);
    }
}

fn render_agent_list(frame: &mut Frame, area: Rect, agents: &[Agent], selected_index: usize) {
    let block = Block::default()
        .title(Span::styled(
            "Agents",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if agents.is_empty() {
        let empty_msg = Paragraph::new("No agents. Press 'n' to create one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty_msg, inner_area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for (i, agent) in agents.iter().enumerate() {
        let is_selected = i == selected_index;

        // Line 1: Arrow + Name
        let arrow = if is_selected { "▶ " } else { "  " };
        let name_content = format!("{}{}", arrow, agent.name);
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        if is_selected {
            let padded_name = format!("{:width$}", name_content, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_name, name_style)));
        } else {
            lines.push(Line::from(vec![
                Span::styled(arrow, Style::default().fg(Color::DarkGray)),
                Span::styled(&agent.name, name_style),
            ]));
        }

        // Line 2: Status
        let status_text = format!("  {}", agent.status);
        let status_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(agent.status.color())
        };
        if is_selected {
            let padded_status =
                format!("{:width$}", status_text, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_status, status_style)));
        } else {
            lines.push(Line::from(Span::styled(status_text, status_style)));
        }

        // Line 3: Uptime or PID
        let info_text = if agent.status == AgentStatus::Running {
            if let Some(pid) = agent.pid() {
                format!("  PID: {}", pid)
            } else {
                format!("  Up: {}s", agent.uptime_secs())
            }
        } else {
            "  --".to_string()
        };
        let info_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if is_selected {
            let padded_info = format!("{:width$}", info_text, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_info, info_style)));
        } else {
            lines.push(Line::from(Span::styled(info_text, info_style)));
        }

        // Line 4: Iteration count
        let loop_text = format!("  Loop: #{}", agent.iteration);
        let loop_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if is_selected {
            let padded_loop = format!("{:width$}", loop_text, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_loop, loop_style)));
        } else {
            lines.push(Line::from(Span::styled(loop_text, loop_style)));
        }

        // Line 5: Project path
        if let Some(ref project_path) = agent.project_path {
            let max_path_len = (inner_area.width as usize).saturating_sub(2);
            let path_str = project_path.to_string_lossy();
            let truncated_path = if path_str.len() > max_path_len {
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

        // Blank line between agents
        if i < agents.len() - 1 {
            lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

/// Renders the terminal pane showing the selected agent's embedded terminal
fn render_terminal_pane(
    frame: &mut Frame,
    area: Rect,
    agent: Option<&Agent>,
    output_focused: bool,
) {
    let title = match agent {
        Some(a) => {
            let focus_hint = if output_focused {
                " [FOCUSED - Esc to exit]"
            } else {
                " [Enter to focus]"
            };
            format!("{}{}", a.name, focus_hint)
        }
        None => "Terminal (none)".to_string(),
    };

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

    let agent = match agent {
        Some(a) => a,
        None => {
            let msg =
                Paragraph::new("No agent selected").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner_area);
            return;
        }
    };

    // Get the terminal screen from the agent
    if let Ok(term) = agent.terminal.lock() {
        let screen = term.screen();
        let pseudo_term = PseudoTerminal::new(screen);
        frame.render_widget(pseudo_term, inner_area);
    } else {
        let msg = Paragraph::new("Terminal unavailable").style(Style::default().fg(Color::Red));
        frame.render_widget(msg, inner_area);
    }
}

fn render_header(frame: &mut Frame, area: Rect) {
    let timestamp = Local::now().format("%H:%M:%S").to_string();

    let title = " COCKPIT";
    let available_width = area.width as usize;
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
        Span::raw(" "),
    ]);

    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = header_block.inner(area);
    frame.render_widget(header_block, area);

    let header = Paragraph::new(vec![Line::from(""), header_line]);
    frame.render_widget(header, inner_area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keybindings = if app.output_focused {
        "Type to interact │ Esc: back │ Ctrl+C: quit"
    } else {
        "j/k: nav │ Enter: focus │ S: start │ s: stop │ p: pause │ r: resume │ n: new │ i: msg │ d: delete │ ?: help │ q: quit"
    };

    // Show status message if available, otherwise show keybindings
    let content = if let Some(ref msg) = app.status_message {
        Span::styled(format!(" {} ", msg), Style::default().fg(Color::Yellow))
    } else {
        Span::styled(
            format!(" {} ", keybindings),
            Style::default().fg(Color::DarkGray),
        )
    };

    let footer = Paragraph::new(Line::from(content));
    frame.render_widget(footer, area);
}

fn render_input_box(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let input_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 5,
    };

    // Clear the area behind the input box
    let clear = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(clear, input_area);

    let title = match app.input_mode {
        InputMode::EnteringPath => "New Agent - Step 1/3",
        InputMode::EnteringName => "New Agent - Step 2/3",
        InputMode::EnteringPrompt => "New Agent - Step 3/3",
        InputMode::EnteringInstruction => "Send Message",
        InputMode::Normal => "",
    };

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(input_area);
    frame.render_widget(block, input_area);

    let prompt = app.input_prompt();
    let input_text = format!("{}{}_", prompt, app.input_buffer);

    let input_line = Line::from(Span::styled(input_text, Style::default().fg(Color::White)));

    let hint = Line::from(Span::styled(
        "Enter to confirm, Esc to cancel",
        Style::default().fg(Color::DarkGray),
    ));

    let content = Paragraph::new(vec![input_line, Line::from(""), hint]);
    frame.render_widget(content, inner_area);
}

fn render_help_screen(frame: &mut Frame) {
    let area = frame.area();
    let help_width = 60.min(area.width - 4);
    let help_height = 20.min(area.height - 4);

    let help_area = Rect {
        x: (area.width - help_width) / 2,
        y: (area.height - help_height) / 2,
        width: help_width,
        height: help_height,
    };

    let clear = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(clear, help_area);

    let block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(help_area);
    frame.render_widget(block, help_area);

    let help_text = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/k, ↑/↓    Navigate agents"),
        Line::from("  Enter       Focus terminal (type to interact)"),
        Line::from("  Esc         Unfocus terminal"),
        Line::from(""),
        Line::from(Span::styled(
            "Agent Control",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  S           Start agent"),
        Line::from("  s           Stop agent"),
        Line::from("  p           Pause agent (SIGSTOP)"),
        Line::from("  r           Resume agent (SIGCONT)"),
        Line::from(""),
        Line::from(Span::styled(
            "Management",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  n           Create new agent"),
        Line::from("  i           Send message to Claude"),
        Line::from("  d           Delete agent"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_text);
    frame.render_widget(help, inner_area);
}
