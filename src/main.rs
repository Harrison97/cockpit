mod agent;
mod app;
mod loop_manager;
mod ui;

use std::io;

use app::App;
use crossterm::{
    event::{self, Event, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Run the main loop
    let result = run(&mut terminal);

    // Cleanup terminal (always runs, even if main loop panicked)
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    while app.running {
        // Draw the UI
        terminal.draw(|frame| {
            ui::render(
                frame,
                &app.agents,
                app.selected_index,
                app.scroll_offset,
                app.output_focused,
            );
        })?;

        // Poll for events with 16ms timeout (~60 FPS)
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events (not release)
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code, key.modifiers);
                }
            }
        }

        // Update application state (mock agent output, etc.)
        app.tick();
    }

    Ok(())
}
