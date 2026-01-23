# God Agent Console - Operational Guide

## Project Overview
Ratatui TUI dashboard for monitoring Ralph-style autonomous AI agents.
Split-pane layout: agent list (left), live output (right), keybinding footer.

## Tech Stack
- Rust (edition 2021)
- ratatui 0.29 - TUI framework
- crossterm 0.28 - Terminal backend
- tokio - Async runtime (for future PTY integration)

## Directory Structure
```
/Users/harrison/dev/cockpit/
├── Cargo.toml
├── src/
│   ├── main.rs      # App loop, event handling, entry point
│   ├── agent.rs     # Agent struct, status enum, mock data
│   └── ui.rs        # UI rendering components
└── specs/           # Design specifications (reference only)
```

## Build Commands
```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run the application
cargo run

# Run with release optimizations
cargo run --release
```

## Test Commands
```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check for issues without building
cargo check

# Lint with clippy
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Quality Gates
Before committing, ensure:
1. `cargo build` succeeds
2. `cargo clippy -- -D warnings` passes
3. `cargo fmt --check` passes
4. Application runs without panic: `cargo run`

## Key Files
- `src/main.rs`: Entry point, App struct, event loop, terminal setup/restore
- `src/agent.rs`: Agent/AgentStatus definitions, mock data generation
- `src/ui.rs`: All render functions (header, agent list, output, footer)

## Specifications
Read specs/*.md for detailed requirements:
- `specs/tui_design.md` - Layout and visual design
- `specs/agent_model.md` - Data structures
- `specs/keybindings.md` - Keyboard shortcuts
