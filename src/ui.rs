//! UI rendering module for the God Agent Console

#![allow(dead_code)]

use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tui_term::widget::PseudoTerminal;

use crate::agent::{Agent, AgentStatus, AgentType};
use crate::app::{App, InputMode, SearchMode, LINES_PER_AGENT};

/// Search state passed to terminal rendering
pub struct SearchState {
    pub mode: SearchMode,
    pub matches: Vec<(usize, usize)>,
    pub current: usize,
    pub query_len: usize,
}

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
pub fn draw(frame: &mut Frame, app: &mut App) {
    let (header_area, main_area, footer_area) = create_main_layout(frame.area());
    let (agent_list_area, output_pane_area) = create_content_layout(main_area);

    // Calculate inner area for agent list to determine visible height
    let agent_list_inner = {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        block.inner(agent_list_area)
    };

    // Ensure selected agent is visible before rendering
    app.ensure_selected_visible(agent_list_inner.height as usize);

    // Calculate terminal pane inner area for resizing
    let terminal_inner = {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        block.inner(output_pane_area)
    };

    // Resize selected agent's terminal to match display area
    if let Some(agent) = app.selected_agent_mut() {
        agent.resize(terminal_inner.height, terminal_inner.width);
    }

    render_header(frame, header_area);
    render_agent_list(
        frame,
        agent_list_area,
        &app.agents,
        app.selected_index,
        app.list_scroll_offset,
    );
    // Read output_focused and search state before borrowing app mutably
    let output_focused = app.output_focused;
    let search_state = SearchState {
        mode: app.search_mode.clone(),
        matches: app.search_matches.clone(),
        current: app.search_current,
        query_len: app.search_query().map(|q| q.len()).unwrap_or(0),
    };
    render_terminal_pane(
        frame,
        output_pane_area,
        app.selected_agent_mut(),
        output_focused,
        &search_state,
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

    // Render delete confirmation if showing
    if app.show_delete_confirm {
        render_delete_confirm(frame, app);
    }
}

fn render_agent_list(
    frame: &mut Frame,
    area: Rect,
    agents: &[Agent],
    selected_index: usize,
    scroll_offset: usize,
) {
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

    // Calculate how many agents can be displayed
    let visible_lines = inner_area.height as usize;
    let has_more_above = scroll_offset > 0;
    let has_more_below = {
        // Calculate total lines needed for agents from scroll_offset onwards
        let remaining_agents = agents.len().saturating_sub(scroll_offset);
        let lines_for_remaining = remaining_agents * LINES_PER_AGENT;
        // Account for scroll indicator line at top if needed
        let available = if has_more_above {
            visible_lines.saturating_sub(1)
        } else {
            visible_lines
        };
        lines_for_remaining > available
    };

    let mut lines: Vec<Line> = Vec::new();

    // Add "more above" indicator if needed
    if has_more_above {
        let indicator = format!("  ▲ {} more above", scroll_offset);
        lines.push(Line::from(Span::styled(
            indicator,
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Calculate available lines for agent entries
    let available_lines = visible_lines
        .saturating_sub(if has_more_above { 1 } else { 0 })
        .saturating_sub(if has_more_below { 1 } else { 0 });

    // Render visible agents
    let mut lines_used = 0;
    let mut agents_displayed = 0;
    for (i, agent) in agents.iter().enumerate().skip(scroll_offset) {
        // Check if we have room for this agent (need LINES_PER_AGENT - 1 for last agent, full for others)
        let lines_needed = if i < agents.len() - 1 {
            LINES_PER_AGENT
        } else {
            LINES_PER_AGENT - 1 // Last agent doesn't need trailing blank line
        };

        if lines_used + lines_needed > available_lines {
            break;
        }

        agents_displayed += 1;

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
        lines_used += 1;

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
        lines_used += 1;

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
        lines_used += 1;

        // Line 4: Type indicator or iteration count
        let type_text = match agent.agent_type {
            AgentType::ClaudeInstance => "  Claude Instance".to_string(),
            AgentType::RalphLoop => format!("  Loop: #{}", agent.iteration),
        };
        let type_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        if is_selected {
            let padded_type = format!("{:width$}", type_text, width = inner_area.width as usize);
            lines.push(Line::from(Span::styled(padded_type, type_style)));
        } else {
            lines.push(Line::from(Span::styled(type_text, type_style)));
        }
        lines_used += 1;

        // Line 5: Working directory (the target repo)
        if let Some(ref working_dir) = agent.working_dir {
            let max_path_len = (inner_area.width as usize).saturating_sub(2);
            let path_str = working_dir.to_string_lossy();
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
            lines_used += 1;
        }

        // Blank line between agents (if not last visible agent)
        if i < agents.len() - 1 && lines_used < available_lines {
            lines.push(Line::from(""));
            lines_used += 1;
        }
    }

    // Add "more below" indicator if needed
    if has_more_below {
        let hidden_count = agents
            .len()
            .saturating_sub(scroll_offset + agents_displayed);
        if hidden_count > 0 {
            let indicator = format!("  ▼ {} more below", hidden_count);
            lines.push(Line::from(Span::styled(
                indicator,
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner_area);
}

/// Renders the terminal pane showing the selected agent's embedded terminal
fn render_terminal_pane(
    frame: &mut Frame,
    area: Rect,
    agent: Option<&mut Agent>,
    output_focused: bool,
    search: &SearchState,
) {
    let is_searching = !matches!(search.mode, SearchMode::Off);

    // Build title - need to access terminal to get scrollback info
    let (title, scroll_offset) = match agent.as_ref() {
        Some(a) => {
            // Get max scrollback size by setting to max and reading clamped value
            let scrollback_size = a
                .terminal
                .lock()
                .ok()
                .map(|mut t| {
                    t.set_scrollback(usize::MAX);
                    let max = t.screen().scrollback();
                    t.set_scrollback(0);
                    max
                })
                .unwrap_or(0);
            let focus_hint = if is_searching {
                if !search.matches.is_empty() {
                    format!(
                        " [SEARCH: Match {}/{}]",
                        search.current + 1,
                        search.matches.len()
                    )
                } else if search.query_len > 0 {
                    " [SEARCH: No matches]".to_string()
                } else {
                    " [SEARCH MODE]".to_string()
                }
            } else if output_focused {
                if a.scroll_offset > 0 {
                    format!(
                        " [SCROLLED +{}/{} - Tab to exit]",
                        a.scroll_offset, scrollback_size
                    )
                } else if scrollback_size > 0 {
                    format!(
                        " [FOCUSED - {} lines history - Tab to exit]",
                        scrollback_size
                    )
                } else {
                    " [FOCUSED - Tab to exit]".to_string()
                }
            } else {
                " [Space to focus]".to_string()
            };
            (format!("{}{}", a.name, focus_hint), a.scroll_offset)
        }
        None => ("Terminal (none)".to_string(), 0),
    };

    let border_color = if is_searching {
        Color::Yellow
    } else if output_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title_color = if is_searching {
        Color::Yellow
    } else if output_focused {
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

    // If in search mode, reserve space for search box at bottom
    let (terminal_area, search_area) = if is_searching {
        let terminal_height = inner_area.height.saturating_sub(1);
        (
            Rect {
                x: inner_area.x,
                y: inner_area.y,
                width: inner_area.width,
                height: terminal_height,
            },
            Some(Rect {
                x: inner_area.x,
                y: inner_area.y + terminal_height,
                width: inner_area.width,
                height: 1,
            }),
        )
    } else {
        (inner_area, None)
    };

    // Get the terminal screen from the agent, applying scroll offset
    if let Ok(mut term) = agent.terminal.lock() {
        // vt100 bug workaround: set_scrollback clamps to scrollback.len(), but
        // visible_rows() panics if scrollback_offset > rows.len() (terminal height).
        // We must clamp to BOTH the scrollback size AND the terminal height.
        let terminal_height = terminal_area.height as usize;

        // Get max scrollback by setting to max and reading clamped value
        term.set_scrollback(usize::MAX);
        let scrollback_max = term.screen().scrollback();

        // Clamp to both scrollback size and terminal height (vt100 bug workaround)
        let safe_max = scrollback_max.min(terminal_height.saturating_sub(1));
        let safe_offset = (scroll_offset as usize).min(safe_max);

        if safe_offset != scroll_offset as usize {
            agent.scroll_offset = safe_offset as u16;
        }

        // Now set the validated scrollback offset
        term.set_scrollback(safe_offset);

        let screen = term.screen();
        let pseudo_term = PseudoTerminal::new(screen);
        frame.render_widget(pseudo_term, terminal_area);

        // Apply search highlighting if there are matches
        // Matches are (row, col, len) where row is the display row (0 = top)
        if !search.matches.is_empty() && search.query_len > 0 {
            let buf = frame.buffer_mut();

            for (match_idx, &(row, col_start)) in search.matches.iter().enumerate() {
                // Check if this row is within visible terminal area
                if row < terminal_height {
                    let screen_y = terminal_area.y + row as u16;

                    // Highlight each character of the match
                    let highlight_bg = if match_idx == search.current {
                        Color::Cyan // Current match
                    } else {
                        Color::Yellow // Other matches
                    };

                    for offset in 0..search.query_len {
                        let screen_x = terminal_area.x + (col_start + offset) as u16;

                        // Make sure we're within bounds
                        if screen_x < terminal_area.x + terminal_area.width
                            && screen_y < terminal_area.y + terminal_area.height
                        {
                            if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                                cell.set_bg(highlight_bg);
                                // For visibility, set foreground to black for highlighted cells
                                cell.set_fg(Color::Black);
                            }
                        }
                    }
                }
            }
        }
    } else {
        let msg = Paragraph::new("Terminal unavailable").style(Style::default().fg(Color::Red));
        frame.render_widget(msg, terminal_area);
    }

    // Render search box if in search mode
    if let Some(search_area) = search_area {
        render_search_box(
            frame,
            search_area,
            &search.mode,
            search.matches.len(),
            search.current,
        );
    }
}

/// Renders the search input box at the bottom of the terminal pane
fn render_search_box(
    frame: &mut Frame,
    area: Rect,
    search_mode: &SearchMode,
    match_count: usize,
    current_match: usize,
) {
    let (query, is_navigating) = match search_mode {
        SearchMode::Searching(q) => (q.as_str(), false),
        SearchMode::Navigating(q) => (q.as_str(), true),
        SearchMode::Off => return,
    };

    let prefix = if is_navigating { "/" } else { "Search: " };
    let cursor = if is_navigating { "" } else { "_" };

    // Build match count suffix
    let match_suffix = if match_count > 0 {
        format!(" [{}/{}]", current_match + 1, match_count)
    } else if !query.is_empty() {
        " [No matches]".to_string()
    } else {
        String::new()
    };

    let available_width = area.width as usize;
    let prefix_len = prefix.chars().count();
    let cursor_len = cursor.chars().count();
    let suffix_len = match_suffix.chars().count();
    let max_query_display = available_width.saturating_sub(prefix_len + cursor_len + suffix_len);

    // Truncate query from left if too long
    let displayed_query = if query.chars().count() > max_query_display {
        let skip = query.chars().count() - max_query_display;
        query.chars().skip(skip).collect::<String>()
    } else {
        query.to_string()
    };

    let line = Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Yellow)),
        Span::styled(displayed_query, Style::default().fg(Color::White)),
        Span::styled(cursor, Style::default().fg(Color::White)),
        Span::styled(match_suffix, Style::default().fg(Color::Cyan)),
    ]);

    let search_bar = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(search_bar, area);
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
    let keybindings = match &app.search_mode {
        SearchMode::Searching(_) => "Type to search │ Enter: confirm │ Esc: cancel".to_string(),
        SearchMode::Navigating(_) => {
            "n/N: next/prev │ j/k: scroll │ Ctrl+D/U: page │ g/G: top/bottom │ q: exit".to_string()
        }
        SearchMode::Off => {
            if app.output_focused {
                // Show scroll indicator if user has scrolled up
                let scroll_hint = app
                    .selected_agent()
                    .map(|a| {
                        if a.scroll_offset > 0 {
                            format!(" │ Scrolled +{}", a.scroll_offset)
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                format!(
                    "Type to interact │ Tab: back │ Scroll: Shift+↑/↓ │ Ctrl+F: search │ Ctrl+C: interrupt{}",
                    scroll_hint
                )
            } else {
                // Show different hints based on selected agent type
                let can_pause = app.selected_agent().map(|a| a.can_pause()).unwrap_or(true);

                if can_pause {
                    "j/k: nav │ Space: focus │ r: run │ s: stop │ p: pause │ n: new │ i: msg │ d: delete │ ?: help │ q: quit".to_string()
                } else {
                    "j/k: nav │ Space: focus │ r: run │ s: stop │ n: new │ i: msg │ d: delete │ ?: help │ q: quit".to_string()
                }
            }
        }
    };

    // Show status message if available, otherwise show keybindings
    let content = if let Some(ref msg) = app.status_message {
        Span::styled(format!(" {} ", msg), Style::default().fg(Color::Yellow))
    } else {
        Span::styled(
            format!(" {} ", &keybindings),
            Style::default().fg(Color::DarkGray),
        )
    };

    let footer = Paragraph::new(Line::from(content));
    frame.render_widget(footer, area);
}

fn render_input_box(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Use different sizes based on input mode - larger for prompts/instructions
    let is_multiline_mode = matches!(
        app.input_mode,
        InputMode::EnteringPrompt | InputMode::EnteringInstruction
    );

    let (box_width, box_height) = if is_multiline_mode {
        // Larger box for prompts and instructions (70% width, up to 16 lines)
        let w = (area.width * 70 / 100).max(60).min(area.width - 4);
        let h = 16.min(area.height - 4);
        (w, h)
    } else {
        // Smaller box for path/name (50% width, 5 lines)
        let w = area.width / 2;
        let h = 5u16;
        (w, h)
    };

    let input_area = Rect {
        x: (area.width - box_width) / 2,
        y: (area.height - box_height) / 2,
        width: box_width,
        height: box_height,
    };

    // Clear the area behind the input box completely
    frame.render_widget(Clear, input_area);

    let line_count = app.input_buffer.lines().count();
    let title = match app.input_mode {
        InputMode::EnteringPath => "New Agent - Step 1/3".to_string(),
        InputMode::EnteringName => "New Agent - Step 2/3".to_string(),
        InputMode::EnteringPrompt => {
            if line_count > 1 {
                format!("New Agent - Step 3/3 ({} lines)", line_count)
            } else {
                "New Agent - Step 3/3".to_string()
            }
        }
        InputMode::EnteringInstruction => {
            if line_count > 1 {
                format!("Send Message ({} lines)", line_count)
            } else {
                "Send Message".to_string()
            }
        }
        InputMode::Normal => String::new(),
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
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(input_area);
    frame.render_widget(block, input_area);

    let prompt = app.input_prompt();
    let available_width = inner_area.width as usize;
    // Reserve 2 lines: one for the hint, one blank line before it
    let available_lines = (inner_area.height as usize).saturating_sub(2);

    if is_multiline_mode && available_lines > 1 {
        // Calculate where to render the main content (leave room for hint at bottom)
        let content_area = Rect {
            x: inner_area.x,
            y: inner_area.y,
            width: inner_area.width,
            height: inner_area.height.saturating_sub(2),
        };

        // Check if we have paste info - if so, show a compact indicator instead of full text
        if let Some((char_count, line_count)) = app.paste_info {
            let paste_indicator = if line_count > 1 {
                format!("[Pasted {} chars, {} lines]", char_count, line_count)
            } else {
                format!("[Pasted {} chars]", char_count)
            };

            let lines = vec![Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::DarkGray)),
                Span::styled(paste_indicator, Style::default().fg(Color::Cyan)),
            ])];

            let content = Paragraph::new(lines).style(Style::default().bg(Color::Black));
            frame.render_widget(content, content_area);
        } else {
            // Normal display - show actual text
            let mut lines: Vec<Line> = Vec::new();

            // First line includes the prompt (gray) followed by first line of input (white)
            let input_lines: Vec<&str> = app.input_buffer.split('\n').collect();

            if input_lines.is_empty() || (input_lines.len() == 1 && input_lines[0].is_empty()) {
                // Empty input - just show prompt and cursor
                lines.push(Line::from(vec![
                    Span::styled(prompt, Style::default().fg(Color::DarkGray)),
                    Span::styled("_", Style::default().fg(Color::White)),
                ]));
            } else {
                // First line: prompt + first line of input
                lines.push(Line::from(vec![
                    Span::styled(prompt, Style::default().fg(Color::DarkGray)),
                    Span::styled(input_lines[0], Style::default().fg(Color::White)),
                ]));

                // Remaining lines of input
                for line in input_lines.iter().skip(1) {
                    lines.push(Line::from(Span::styled(
                        *line,
                        Style::default().fg(Color::White),
                    )));
                }

                // Add cursor to last line (or new line if buffer ends with newline)
                if app.input_buffer.ends_with('\n') {
                    lines.push(Line::from(Span::styled(
                        "_",
                        Style::default().fg(Color::White),
                    )));
                } else if let Some(last) = lines.last_mut() {
                    last.spans
                        .push(Span::styled("_", Style::default().fg(Color::White)));
                }
            }

            // Build the content with proper wrapping
            let content = Paragraph::new(lines)
                .style(Style::default().bg(Color::Black))
                .wrap(Wrap { trim: false });

            frame.render_widget(content, content_area);
        }

        // Render hint at the bottom
        let hint_area = Rect {
            x: inner_area.x,
            y: inner_area.y + inner_area.height.saturating_sub(1),
            width: inner_area.width,
            height: 1,
        };
        let hint = Paragraph::new(Span::styled(
            "Enter: submit │ \\+Enter: newline │ Esc: cancel",
            Style::default().fg(Color::DarkGray),
        ))
        .style(Style::default().bg(Color::Black));
        frame.render_widget(hint, hint_area);
    } else {
        // Single-line display mode (for path/name)
        let cursor = "_";
        let full_text = format!("{}{}{}", prompt, app.input_buffer, cursor);

        // Scroll horizontally to show end of text (where cursor is)
        let visible_text = if full_text.chars().count() > available_width {
            let skip = full_text.chars().count() - available_width;
            full_text.chars().skip(skip).collect::<String>()
        } else {
            full_text
        };

        let input_line = Line::from(Span::styled(
            visible_text,
            Style::default().fg(Color::White),
        ));
        let hint = Line::from(Span::styled(
            "Enter: submit │ Esc: cancel",
            Style::default().fg(Color::DarkGray),
        ));

        let content = Paragraph::new(vec![input_line, Line::from(""), hint])
            .style(Style::default().bg(Color::Black));
        frame.render_widget(content, inner_area);
    }
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

    // Clear the area behind the help screen completely
    frame.render_widget(Clear, help_area);

    let block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(help_area);
    frame.render_widget(block, help_area);

    let help_text = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/k, ↑/↓    Navigate agents"),
        Line::from("  Tab         Next agent / Unfocus terminal"),
        Line::from("  Space       Focus terminal (type to interact)"),
        Line::from("  Mouse wheel Scroll terminal (when focused)"),
        Line::from(""),
        Line::from(Span::styled(
            "Agent Control",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  r           Run/Resume agent"),
        Line::from("  s           Stop agent"),
        Line::from("  p           Pause agent (ralph loops only)"),
        Line::from("  Ctrl+C      Interrupt Claude (when focused)"),
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
            "Input Mode",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter       Submit input"),
        Line::from("  \\+Enter     Add newline"),
        Line::from("  Esc         Cancel"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_text).style(Style::default().bg(Color::Black));
    frame.render_widget(help, inner_area);
}

fn render_delete_confirm(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let box_width = 40.min(area.width - 4);
    let box_height = 5;

    let confirm_area = Rect {
        x: (area.width - box_width) / 2,
        y: (area.height - box_height) / 2,
        width: box_width,
        height: box_height,
    };

    frame.render_widget(Clear, confirm_area);

    let agent_name = app
        .selected_agent()
        .map(|a| a.name.as_str())
        .unwrap_or("agent");

    let block = Block::default()
        .title(Span::styled(
            " Confirm Delete ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(confirm_area);
    frame.render_widget(block, confirm_area);

    let text = vec![
        Line::from(format!("Delete \"{}\"?", agent_name)),
        Line::from(""),
        Line::from(Span::styled(
            "y: confirm │ any other key: cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let content = Paragraph::new(text)
        .style(Style::default().bg(Color::Black))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(content, inner_area);
}
