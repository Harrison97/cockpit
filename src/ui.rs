//! UI rendering module for the God Agent Console

// Allow dead_code for UI components that may be conditionally rendered
// or used in different modes/themes in future versions
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

use crate::agent::{Agent, AgentStatus, AgentType, ProcessState};
use crate::app::{App, ImportableAgent, InputMode, SearchMode, LINES_PER_AGENT};

/// Search state passed to terminal rendering
pub struct SearchState {
    pub mode: SearchMode,
    pub matches: Vec<(usize, usize)>,
    pub current: usize,
    pub query_len: usize,
    pub total_matches: usize,
}

/// Selection state passed to terminal rendering
pub struct SelectionState {
    pub start: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
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

    // Check for terminal width changes and clear selection if needed
    // (selection coordinates become invalid when content re-wraps)
    app.on_terminal_resize(terminal_inner.width);

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
        total_matches: app.search_matches_absolute_count(),
    };
    let selection_state = SelectionState {
        start: app.selection_start,
        end: app.selection_end,
    };
    let terminal_area = render_terminal_pane(
        frame,
        output_pane_area,
        app.selected_agent_mut(),
        output_focused,
        &search_state,
        &selection_state,
    );
    // Store terminal area for mouse coordinate translation
    app.terminal_area = terminal_area;
    render_footer(frame, footer_area, app);

    // Render input box if in input mode
    if app.input_mode != InputMode::Normal {
        render_input_box(frame, app);
    }

    // Render import selection if in import mode
    if let InputMode::SelectingImport(ref agents) = app.input_mode {
        render_import_selection(frame, agents, app.import_selection_index);
    }

    // Render help screen if showing
    if app.show_help {
        render_help_screen(frame);
    }

    // Render delete confirmation if showing
    if app.show_delete_confirm {
        render_delete_confirm(frame, app);
    }

    // Render stop confirmation if showing
    if app.show_stop_confirm {
        render_stop_confirm(frame, app);
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
        let display_idx = i - scroll_offset; // 0-based index of visible agents
        let is_alternate = display_idx % 2 == 1;
        let width = inner_area.width as usize;

        // Base background for alternating rows (subtle visual separation)
        let alt_bg = if is_alternate {
            Color::Rgb(30, 30, 35) // Very subtle alternating background
        } else {
            Color::Reset
        };

        // Line 1: Arrow + Name
        let arrow = if is_selected { "▶ " } else { "  " };
        let name_content = format!("{}{}", arrow, agent.name);
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::White)
                .bg(alt_bg)
                .add_modifier(Modifier::BOLD)
        };
        let padded_name = format!("{:width$}", name_content, width = width);
        if is_selected {
            lines.push(Line::from(Span::styled(padded_name, name_style)));
        } else {
            // Use separate spans for arrow and name to color them differently
            let arrow_style = Style::default().fg(Color::DarkGray).bg(alt_bg);
            let name_only_style = Style::default()
                .fg(Color::White)
                .bg(alt_bg)
                .add_modifier(Modifier::BOLD);
            let name_only = &agent.name;
            let padding_len = width.saturating_sub(arrow.len() + name_only.len());
            let padding = " ".repeat(padding_len);
            lines.push(Line::from(vec![
                Span::styled(arrow, arrow_style),
                Span::styled(name_only.clone(), name_only_style),
                Span::styled(padding, Style::default().bg(alt_bg)),
            ]));
        }
        lines_used += 1;

        // Line 2: Status (with process state hint when not ready)
        let process_hint = match agent.process_state {
            ProcessState::Starting => " (starting...)",
            ProcessState::Stopping => " (stopping...)",
            ProcessState::Exiting => " (exiting...)",
            _ => "",
        };
        let status_text = format!("  {}{}", agent.status, process_hint);
        let status_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(agent.status.color())
                .bg(alt_bg)
                .add_modifier(Modifier::BOLD)
        };
        let padded_status = format!("{:width$}", status_text, width = width);
        lines.push(Line::from(Span::styled(padded_status, status_style)));
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
            Style::default().fg(Color::DarkGray).bg(alt_bg)
        };
        let padded_info = format!("{:width$}", info_text, width = width);
        lines.push(Line::from(Span::styled(padded_info, info_style)));
        lines_used += 1;

        // Line 4: Type indicator or iteration count
        let type_text = match agent.agent_type {
            AgentType::ClaudeInstance => "  Claude Instance".to_string(),
            AgentType::RalphLoop => format!("  Loop: #{}", agent.iteration),
        };
        let type_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray).bg(alt_bg)
        };
        let padded_type = format!("{:width$}", type_text, width = width);
        lines.push(Line::from(Span::styled(padded_type, type_style)));
        lines_used += 1;

        // Line 5: Working directory (the target repo)
        if let Some(ref working_dir) = agent.working_dir {
            let max_path_len = width.saturating_sub(2);
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
                Style::default().fg(Color::DarkGray).bg(alt_bg)
            };
            let padded_path = format!("{:width$}", path_text, width = width);
            lines.push(Line::from(Span::styled(padded_path, path_style)));
            lines_used += 1;
        }

        // Separator line between agents (dim horizontal divider instead of blank)
        if i < agents.len() - 1 && lines_used < available_lines {
            let separator = "─".repeat(width);
            lines.push(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::Rgb(50, 50, 55)),
            )));
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
    selection: &SelectionState,
) -> Option<(u16, u16, u16, u16)> {
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

            // Check if input is blocked during process transitions
            let process_state_hint = match a.process_state {
                ProcessState::Starting => " [Starting...]",
                ProcessState::Stopping => " [Stopping...]",
                ProcessState::Exiting => " [Exiting...]",
                _ => "",
            };

            let focus_hint = if is_searching {
                if search.total_matches > 0 {
                    format!(
                        " [SEARCH: Match {}/{}]",
                        search.current + 1,
                        search.total_matches
                    )
                } else if search.query_len > 0 {
                    " [SEARCH: No matches]".to_string()
                } else {
                    " [SEARCH MODE]".to_string()
                }
            } else if output_focused {
                if a.scroll_offset > 0 {
                    // Show "Line X / Y" format when scrolled
                    // X = current line from top (scrollback_size - scroll_offset + 1)
                    // Y = total lines in history (scrollback_size + terminal_height)
                    let terminal_height = a.terminal_height() as usize;
                    let total_lines = scrollback_size + terminal_height;
                    let current_line = scrollback_size.saturating_sub(a.scroll_offset as usize) + 1;
                    format!(" [Line {} / {} - Tab to exit]", current_line, total_lines)
                } else {
                    " [LIVE - Tab to exit]".to_string()
                }
            } else {
                " [Space/Enter to focus]".to_string()
            };
            (
                format!("{}{}{}", a.name, process_state_hint, focus_hint),
                a.scroll_offset,
            )
        }
        None => ("Terminal (none)".to_string(), 0),
    };

    let (border_color, border_type) = if is_searching {
        (Color::Yellow, BorderType::Double)
    } else if output_focused {
        (Color::Cyan, BorderType::Double)
    } else {
        (Color::DarkGray, BorderType::Rounded)
    };

    let title_style = if is_searching {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if output_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    let block = Block::default()
        .title(Span::styled(format!(" {} ", title), title_style))
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let agent = match agent {
        Some(a) => a,
        None => {
            let msg =
                Paragraph::new("No agent selected").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner_area);
            return None;
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
        let terminal_height = terminal_area.height as usize;

        // Get max scrollback by setting to max and reading clamped value
        term.set_scrollback(usize::MAX);
        let scrollback_max = term.screen().scrollback();

        // Set scrollback offset for rendering (clamped to max available)
        let render_offset = (scroll_offset as usize).min(scrollback_max);
        term.set_scrollback(render_offset);

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

        // Apply selection highlighting
        if let (Some((start_row, start_col)), Some((end_row, end_col))) =
            (selection.start, selection.end)
        {
            // Normalize selection (start <= end)
            let ((sel_start_row, sel_start_col), (sel_end_row, sel_end_col)) =
                if start_row < end_row || (start_row == end_row && start_col <= end_col) {
                    ((start_row, start_col), (end_row, end_col))
                } else {
                    ((end_row, end_col), (start_row, start_col))
                };

            // Compute visibility inline (avoid calling agent.absolute_row_to_visible
            // which would deadlock since we already hold the terminal lock)
            let base_row = scrollback_max.saturating_sub(scroll_offset as usize);
            let end_row_visible = base_row + terminal_height;

            let buf = frame.buffer_mut();
            let cols = terminal_area.width as usize;

            for abs_row in sel_start_row..=sel_end_row {
                // Check if this row is visible (inline visibility check)
                if abs_row >= base_row && abs_row < end_row_visible {
                    let vis_row = abs_row - base_row;
                    let screen_y = terminal_area.y + vis_row as u16;

                    // Determine column range for this row
                    let col_start = if abs_row == sel_start_row {
                        sel_start_col
                    } else {
                        0
                    };
                    let col_end = if abs_row == sel_end_row {
                        sel_end_col + 1
                    } else {
                        cols
                    };

                    for col in col_start..col_end.min(cols) {
                        let screen_x = terminal_area.x + col as u16;

                        if screen_x < terminal_area.x + terminal_area.width
                            && screen_y < terminal_area.y + terminal_area.height
                        {
                            if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                                // Use inverted colors for selection
                                let fg = cell.fg;
                                let bg = cell.bg;
                                cell.set_fg(bg);
                                cell.set_bg(if fg == Color::Reset { Color::White } else { fg });
                            }
                        }
                    }
                }
            }
        }

        // Return terminal area for mouse coordinate translation
        let result = Some((
            terminal_area.x,
            terminal_area.y,
            terminal_area.width,
            terminal_area.height,
        ));

        // Render search box if in search mode
        if let Some(search_area) = search_area {
            render_search_box(
                frame,
                search_area,
                &search.mode,
                search.total_matches,
                search.current,
            );
        }

        return result;
    }

    // No agent case
    let msg = Paragraph::new("Terminal unavailable").style(Style::default().fg(Color::Red));
    frame.render_widget(msg, terminal_area);

    // Render search box if in search mode
    if let Some(search_area) = search_area {
        render_search_box(
            frame,
            search_area,
            &search.mode,
            search.total_matches,
            search.current,
        );
    }

    Some((
        terminal_area.x,
        terminal_area.y,
        terminal_area.width,
        terminal_area.height,
    ))
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
                    "j/k: nav │ Space/Enter: focus │ r: run │ s: stop │ p: pause │ n: new │ i: import │ d: delete │ ?: help │ q: quit".to_string()
                } else {
                    "j/k: nav │ Space/Enter: focus │ r: run │ s: stop │ n: new │ i: import │ d: delete │ ?: help │ q: quit".to_string()
                }
            }
        }
    };

    // Show status message if available, otherwise show keybindings
    let (content, bg_color) = if let Some(ref msg) = app.status_message {
        (
            Line::from(vec![
                Span::styled(" ", Style::default().bg(Color::Rgb(40, 40, 30))),
                Span::styled(
                    msg.clone(),
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Rgb(40, 40, 30))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default().bg(Color::Rgb(40, 40, 30))),
            ]),
            Color::Rgb(40, 40, 30),
        )
    } else {
        // Parse keybindings to highlight keys differently from descriptions
        let parts: Vec<&str> = keybindings.split('│').collect();
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(
            " ",
            Style::default().bg(Color::Rgb(25, 25, 30)),
        ));

        for (idx, part) in parts.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(
                    " │ ",
                    Style::default()
                        .fg(Color::Rgb(60, 60, 70))
                        .bg(Color::Rgb(25, 25, 30)),
                ));
            }

            let part = part.trim();
            // Split on ": " to separate key from description
            if let Some(colon_pos) = part.find(": ") {
                let key = &part[..colon_pos];
                let desc = &part[colon_pos + 2..];
                spans.push(Span::styled(
                    key.to_string(),
                    Style::default().fg(Color::Cyan).bg(Color::Rgb(25, 25, 30)),
                ));
                spans.push(Span::styled(
                    ": ",
                    Style::default()
                        .fg(Color::Rgb(80, 80, 90))
                        .bg(Color::Rgb(25, 25, 30)),
                ));
                spans.push(Span::styled(
                    desc.to_string(),
                    Style::default()
                        .fg(Color::Rgb(140, 140, 150))
                        .bg(Color::Rgb(25, 25, 30)),
                ));
            } else {
                spans.push(Span::styled(
                    part.to_string(),
                    Style::default()
                        .fg(Color::Rgb(140, 140, 150))
                        .bg(Color::Rgb(25, 25, 30)),
                ));
            }
        }

        (Line::from(spans), Color::Rgb(25, 25, 30))
    };

    let footer = Paragraph::new(content).style(Style::default().bg(bg_color));
    frame.render_widget(footer, area);
}

fn render_input_box(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Don't render input box for SelectingImport mode (has its own renderer)
    if matches!(app.input_mode, InputMode::SelectingImport(_)) {
        return;
    }

    // Use different sizes based on input mode - larger for prompts
    let is_multiline_mode = matches!(app.input_mode, InputMode::EnteringPrompt);

    let (box_width, box_height) = if is_multiline_mode {
        // Larger box for prompts (70% width, up to 16 lines)
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
        InputMode::Normal => String::new(),
        InputMode::SelectingImport(_) => return, // Handled by render_import_selection
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

fn render_import_selection(frame: &mut Frame, agents: &[ImportableAgent], selected_index: usize) {
    let area = frame.area();

    // Calculate box size based on number of agents
    let box_width = 50.min(area.width - 4).max(40);
    // Height: title + border (2) + agents (1 each) + hint (2) + border (1)
    let min_height = 6u16;
    let max_height = (area.height - 4).min(20);
    let agents_height = agents.len() as u16;
    let box_height = (min_height + agents_height).min(max_height);

    let select_area = Rect {
        x: (area.width - box_width) / 2,
        y: (area.height - box_height) / 2,
        width: box_width,
        height: box_height,
    };

    // Clear the area behind the selection box
    frame.render_widget(Clear, select_area);

    let block = Block::default()
        .title(Span::styled(
            " Import Agent ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(select_area);
    frame.render_widget(block, select_area);

    // Calculate visible agents with scrolling
    let available_lines = inner_area.height.saturating_sub(2) as usize; // Reserve 2 for hint
    let scroll_offset = if selected_index >= available_lines {
        selected_index - available_lines + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();

    // Render visible agents
    for (idx, agent) in agents.iter().enumerate().skip(scroll_offset) {
        if lines.len() >= available_lines {
            break;
        }

        let is_selected = idx == selected_index;
        let arrow = if is_selected { "▶ " } else { "  " };

        let type_indicator = match agent.agent_type {
            AgentType::RalphLoop => "[loop]",
            AgentType::ClaudeInstance => "[claude]",
        };

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let type_style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Pad to full width for highlighting
        let name_width = inner_area.width as usize - arrow.len() - type_indicator.len() - 2;
        let display_name = if agent.name.len() > name_width {
            format!("{}...", &agent.name[..name_width.saturating_sub(3)])
        } else {
            agent.name.clone()
        };
        let padding = " ".repeat(name_width.saturating_sub(display_name.len()));

        lines.push(Line::from(vec![
            Span::styled(arrow, style),
            Span::styled(display_name, style),
            Span::styled(padding, style),
            Span::styled(" ", style),
            Span::styled(type_indicator, type_style),
        ]));
    }

    // Add scroll indicator if needed
    if agents.len() > available_lines {
        let indicator = format!(" ({}/{})", selected_index + 1, agents.len());
        lines.push(Line::from(Span::styled(
            indicator,
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(""));
    }

    // Add hint at bottom
    lines.push(Line::from(Span::styled(
        "j/k: select │ Enter: import │ Esc: cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let content = Paragraph::new(lines).style(Style::default().bg(Color::Black));
    frame.render_widget(content, inner_area);
}

fn render_help_screen(frame: &mut Frame) {
    let area = frame.area();
    let help_width = 72.min(area.width - 4);
    let help_height = 32.min(area.height - 4);

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
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(help_area);
    frame.render_widget(block, help_area);

    // Helper to create a styled key-description line
    fn key_line(key: &str, desc: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{:13}", key),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(Color::White)),
        ])
    }

    // Helper for section headers
    fn section(title: &str) -> Line<'static> {
        Line::from(Span::styled(
            title.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    }

    let help_text = vec![
        section("Navigation"),
        key_line("j / k", "Move selection up/down in agent list"),
        key_line("↑ / ↓", "Move selection up/down in agent list"),
        key_line("Tab", "Cycle agents / toggle terminal focus"),
        key_line("Space/Enter", "Focus terminal pane for interaction"),
        Line::from(""),
        section("Agent Control"),
        key_line("r", "Run agent (or resume if paused)"),
        key_line("s", "Stop agent (kills session)"),
        key_line("p", "Pause agent (ralph loops only)"),
        key_line("Ctrl+C", "Interrupt Claude (when terminal focused)"),
        Line::from(""),
        section("Agent Management"),
        key_line("n", "Create new agent"),
        key_line("d", "Delete selected agent"),
        key_line("i", "Import existing agent from disk"),
        Line::from(""),
        section("Terminal Scrolling (when focused)"),
        key_line("Shift+↑/↓", "Scroll up/down one line"),
        key_line("Mouse wheel", "Scroll up/down"),
        key_line("Ctrl+D/U", "Scroll half-page down/up"),
        key_line("g / G", "Jump to top / bottom of history"),
        Line::from(""),
        section("Search Mode (Ctrl+F when focused)"),
        key_line("Ctrl+F", "Enter search mode"),
        key_line("n / N", "Next / previous match"),
        key_line("Enter", "Confirm search, switch to navigation"),
        key_line("q / Esc", "Exit search mode"),
        Line::from(""),
        section("General"),
        key_line("?", "Toggle this help screen"),
        key_line("q", "Quit cockpit"),
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

fn render_stop_confirm(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let box_width = 44.min(area.width - 4);
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
            " Confirm Stop ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(confirm_area);
    frame.render_widget(block, confirm_area);

    let text = vec![
        Line::from(format!("Stop \"{}\"? Session will be killed.", agent_name)),
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
