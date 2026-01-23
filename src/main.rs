mod agent;
mod app;
mod loop_manager;
mod persistence;
mod project;
mod ui;

use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
use tracing::info;

static TERMINAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn cleanup_terminal() {
    if TERMINAL_INITIALIZED.swap(false, Ordering::SeqCst) {
        let _ = io::stdout().execute(DisableMouseCapture);
        let _ = io::stdout().execute(DisableBracketedPaste);
        let _ = io::stdout().execute(crossterm::cursor::Show);
        let _ = io::stdout().execute(LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// Shared flag to signal shutdown from signal handler
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    // Enable tokio-console if TOKIO_CONSOLE=1 is set
    if std::env::var("TOKIO_CONSOLE").is_ok_and(|v| v == "1") {
        // tokio-console subscriber for async task inspection
        // Connect with: tokio-console http://127.0.0.1:6669
        console_subscriber::init();
        return;
    }

    // Set up file logging to .cockpit/logs/
    // Log level controlled by COCKPIT_LOG env var (default: info)
    let logs_dir = persistence::get_logs_dir();
    if std::fs::create_dir_all(&logs_dir).is_err() {
        return; // Can't create logs dir, skip logging
    }

    // Create timestamped log file
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let log_filename = format!("cockpit-{}.log", timestamp);

    let log_file = match std::fs::File::create(logs_dir.join(&log_filename)) {
        Ok(f) => f,
        Err(_) => return, // Can't create log file, skip logging
    };

    let filter = EnvFilter::try_from_env("COCKPIT_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = fmt::layer()
        .with_writer(log_file)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .init();
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize tracing - logs to .cockpit/logs/ by default
    // Use COCKPIT_LOG=debug for verbose logging
    // Use TOKIO_CONSOLE=1 for async task inspection
    init_tracing();

    info!(version = env!("CARGO_PKG_VERSION"), "cockpit starting");

    // Set up panic hook to cleanup terminal on panic
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        cleanup_terminal();
        default_hook(info);
    }));

    // Set up SIGINT/SIGTERM handler to trigger graceful shutdown
    // This ensures terminal cleanup runs even when killed externally
    let shutdown_flag = Arc::new(&SHUTDOWN_REQUESTED);
    tokio::spawn(async move {
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }

        // Signal received - request graceful shutdown
        shutdown_flag.store(true, Ordering::SeqCst);
    });

    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(crossterm::terminal::Clear(
        crossterm::terminal::ClearType::All,
    ))?;
    io::stdout().execute(crossterm::cursor::MoveTo(0, 0))?;
    io::stdout().execute(crossterm::cursor::Hide)?;
    io::stdout().execute(EnableBracketedPaste)?;
    io::stdout().execute(EnableMouseCapture)?;
    TERMINAL_INITIALIZED.store(true, Ordering::SeqCst);
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Run the main loop
    let result = run(&mut terminal);

    info!("cockpit shutting down");

    // Cleanup terminal
    cleanup_terminal();

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    let mut last_input = std::time::Instant::now();

    while app.running && !SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
        // Update application state FIRST (process agent output, refresh search matches)
        // This ensures render sees fresh data
        app.tick();

        // Draw the UI with updated state
        terminal.draw(|frame| {
            ui::draw(frame, &mut app);
        })?;

        // Use adaptive poll rate: 60 FPS when user recently interacted, 10 FPS when idle
        // This reduces CPU usage when just watching terminal output
        let idle_duration = last_input.elapsed();
        let poll_timeout = if idle_duration < std::time::Duration::from_secs(2) {
            std::time::Duration::from_millis(16) // 60 FPS when recently active
        } else {
            std::time::Duration::from_millis(100) // 10 FPS when idle
        };

        // Drain ALL pending events in one tick to prevent mouse event backup
        // Coalesce scroll events into a single delta
        let mut scroll_delta: i32 = 0;
        let mut had_event = false;

        while event::poll(std::time::Duration::ZERO)? {
            had_event = true;
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
                    // Coalesce scroll events, ignore other mouse events
                    if app.output_focused {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => scroll_delta += 3,
                            MouseEventKind::ScrollDown => scroll_delta -= 3,
                            _ => {} // Ignore move, click, drag events
                        }
                    }
                }
                _ => {}
            }
        }

        // Apply coalesced scroll after draining all events
        if scroll_delta > 0 {
            app.scroll_terminal_up(scroll_delta as u16);
        } else if scroll_delta < 0 {
            app.scroll_terminal_down((-scroll_delta) as u16);
        }

        if had_event {
            last_input = std::time::Instant::now();
        } else {
            // No events ready, wait for next event or timeout
            if event::poll(poll_timeout)? {
                continue; // Loop back to drain events
            }
        }
    }

    // Gracefully shutdown: stop all subprocesses and save state
    app.shutdown();

    Ok(())
}
