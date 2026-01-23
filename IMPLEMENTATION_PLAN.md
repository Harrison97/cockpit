# Implementation Plan

## Phase 1: Project Setup

- [x] **1.1 Initialize Cargo project**
  - Create Cargo.toml with dependencies: ratatui 0.29, crossterm 0.28, tokio (full features)
  - Create src/main.rs with minimal "Hello, world!" to verify setup
  - Run `cargo build` to confirm dependencies resolve

- [x] **1.2 Create basic terminal setup**
  - In main.rs: enable raw mode, enter alternate screen
  - Create Terminal with CrosstermBackend
  - Add cleanup on exit (disable raw mode, leave alternate screen)
  - Handle Ctrl+C gracefully

## Phase 2: Data Model

- [x] **2.1 Create agent.rs with AgentStatus enum**
  - Define AgentStatus: Running, Stopped, Paused
  - Implement Display trait for status text
  - Add method to get status color (for UI)

- [ ] **2.2 Create Agent struct**
  - Fields: name, status, start_time (Option<Instant>), output (Vec<String>), iteration
  - Implement new(), uptime_secs(), add_output()
  - Implement start(), stop(), pause(), resume() methods

- [ ] **2.3 Add mock data generation**
  - Create ALPHA_OUTPUTS and GAMMA_OUTPUTS constants with sample log lines
  - Create create_mock_agents() function returning 3 agents
  - Alpha: Running with initial output, Gamma: Running, Beta: Stopped

## Phase 3: UI Components

- [ ] **3.1 Create ui.rs module structure**
  - Create src/ui.rs file
  - Add module declaration in main.rs
  - Define render function signature that takes frame and app state

- [ ] **3.2 Implement main layout**
  - Create vertical layout: header (3), main (flex), footer (1)
  - Create horizontal split for main: left (20%), right (80%)
  - Use Constraint::Percentage and Constraint::Length

- [ ] **3.3 Implement header rendering**
  - Block with title "GOD AGENT CONSOLE" (cyan, bold)
  - Right-aligned timestamp showing current time HH:MM:SS
  - Bottom border only

- [ ] **3.4 Implement agent list rendering**
  - Block with title "AGENTS" and full border
  - List each agent with: arrow (if selected), name, status dot, uptime, loop count
  - Highlight selected agent with cyan background
  - Status colors: green=running, red=stopped, yellow=paused

- [ ] **3.5 Implement output pane rendering**
  - Block with title "Agent Output: {name}" and full border
  - Display output lines from selected agent
  - Each line prefixed with timestamp
  - Support scrolling (track scroll offset)

- [ ] **3.6 Implement footer rendering**
  - Single line with keybinding hints
  - Format: "j/k: navigate  Enter: focus  p: pause  r: resume  q: quit"
  - Dim styling

## Phase 4: App State and Event Loop

- [ ] **4.1 Create App struct**
  - Fields: agents, selected_index, scroll_offset, output_focused, running
  - Implement new() initializing with mock agents
  - Implement select_next(), select_prev(), selected_agent()

- [ ] **4.2 Implement main event loop**
  - Poll for events with 16ms timeout (60 FPS)
  - On tick: update mock agent output (if running)
  - On key event: dispatch to handlers
  - On quit: set running = false

- [ ] **4.3 Implement navigation keybindings**
  - j/Down: select next agent
  - k/Up: select previous agent
  - g: select first agent
  - G: select last agent

- [ ] **4.4 Implement agent control keybindings**
  - p: pause selected agent
  - r: resume selected agent
  - s: stop selected agent

- [ ] **4.5 Implement output focus keybindings**
  - Enter: toggle output_focused
  - When focused: j/k scroll output, Esc unfocuses
  - Visual indicator when focused (brighter border)

- [ ] **4.6 Implement quit keybindings**
  - q: quit application
  - Ctrl+C: quit application

## Phase 5: Polish

- [ ] **5.1 Add mock output generation**
  - Running agents periodically add new output lines
  - Random interval 2-5 seconds between lines
  - Cycle through predefined output messages
  - Increment iteration counter every ~10 messages

- [ ] **5.2 Add auto-scroll behavior**
  - When new output added, auto-scroll to bottom
  - Only if user hasn't manually scrolled up
  - Track "pinned to bottom" state

- [ ] **5.3 Final visual polish**
  - Ensure consistent colors per spec
  - Rounded borders everywhere
  - Proper spacing between elements
  - Test at various terminal sizes

## Completion Criteria

All items checked. Application:
- Builds without warnings (`cargo clippy`)
- Runs without panics
- Displays correct layout matching spec
- All keybindings work
- Mock agents update in real-time
- Smooth 60fps rendering
