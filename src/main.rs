mod agent;
mod app;
mod loop_manager;
mod persistence;
mod project;
mod ui;

use std::io;

use app::App;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEventKind, MouseEventKind,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableBracketedPaste)?;
    io::stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Run the main loop
    let result = run(&mut terminal);

    // Cleanup terminal (always runs, even if main loop panicked)
    io::stdout().execute(DisableMouseCapture)?;
    io::stdout().execute(DisableBracketedPaste)?;
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    while app.running {
        // Draw the UI
        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;

        // Poll for events with 16ms timeout (~60 FPS)
        if event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    // Only handle key press events (not release)
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key.code, key.modifiers);
                    }
                }
                Event::Paste(text) => {
                    // Handle pasted text (preserves newlines)
                    app.handle_paste(&text);
                }
                Event::Mouse(mouse) => {
                    // Handle mouse scroll events when output is focused
                    if app.output_focused {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => {
                                app.scroll_terminal_up(3);
                            }
                            MouseEventKind::ScrollDown => {
                                app.scroll_terminal_down(3);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // Update application state (mock agent output, etc.)
        app.tick();
    }

    // Gracefully shutdown: stop all subprocesses and save state
    app.shutdown();

    Ok(())
}
