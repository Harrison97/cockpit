# Cockpit - Ralph Loop Control System

## Project Overview

CLI tool for creating, deploying, observing, and intervening with ralph loops.

A ralph loop is a bash loop running Claude Code autonomously:
```bash
while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions; done
```

Cockpit provides a TUI to manage multiple loops simultaneously.

## Tech Stack

- Rust (edition 2021)
- ratatui 0.29 - TUI framework
- crossterm 0.28 - Terminal backend
- tokio - Async runtime for subprocess I/O
- nix - Unix signal handling (SIGSTOP/SIGCONT/SIGTERM)

## Directory Structure

```
/Users/harrison/dev/cockpit/
├── Cargo.toml
├── PROMPT.md              # Ralph build prompt (this project builds itself)
├── IMPLEMENTATION_PLAN.md # Task list for ralph loop
├── src/
│   ├── main.rs           # App loop, event handling, entry point
│   ├── app.rs            # Application state management
│   ├── agent.rs          # Agent struct, status enum
│   ├── ui.rs             # UI rendering components
│   ├── loop_manager.rs   # Subprocess spawning and control
│   ├── project.rs        # Ralph project file operations
│   └── persistence.rs    # State file and output logging
└── specs/                # Design specifications
    ├── tui_design.md     # Layout and visual design
    ├── keybindings.md    # Keyboard shortcuts
    ├── loop_manager.md   # Subprocess management spec
    └── project.md        # Ralph project spec
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

## Quality Gates

Before committing, ensure:
1. `cargo build` succeeds
2. `cargo clippy -- -D warnings` passes
3. `cargo fmt` applied
4. Application runs without panic: `cargo run`

## Running as Ralph Loop

This project uses its own pattern to build itself:

```bash
# Start the build loop
while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions; done
```

Each iteration picks one task from IMPLEMENTATION_PLAN.md, implements it, and exits.

## Key Concepts

### Ralph Loop
A subprocess running the bash while loop with Claude. Managed by RalphLoop struct.

### Ralph Project
A directory containing PROMPT.md and optionally IMPLEMENTATION_PLAN.md, specs/, etc.

### Agent
UI representation of a ralph loop. Shows status, output, iteration count.

### Intervention
Sending instructions to a loop by writing to PRIORITY_INSTRUCTIONS.md.
