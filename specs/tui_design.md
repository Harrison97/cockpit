# TUI Design Specification

## Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ GOD AGENT CONSOLE                                    19:35:42  │
├─────────────────┬───────────────────────────────────────────────┤
│ AGENTS          │ Agent Output: alpha                          │
│                 │                                              │
│ → alpha         │ [19:34:12] Analyzing market data...         │
│   ● RUNNING     │ [19:34:15] Found RSI divergence pattern     │
│   Uptime: 342s  │ [19:34:18] Backtesting strategy...          │
│   Loop #12      │ [19:34:21] Sharpe: 2.3, Win: 67%            │
│                 │ [19:34:24] Writing specs/strategy_v13.py    │
│   beta          │ [19:34:27] >                                │
│   ○ STOPPED     │                                              │
│   Uptime: 0s    │                                              │
│                 │                                              │
│   gamma         │                                              │
│   ● RUNNING     │                                              │
│   Uptime: 156s  │                                              │
│   Loop #7       │                                              │
├─────────────────┴───────────────────────────────────────────────┤
│ j/k: navigate  Enter: focus  p: pause  r: resume  q: quit     │
└─────────────────────────────────────────────────────────────────┘
```

## Layout Structure

Use Ratatui's Layout with constraints:

1. **Vertical split** (main layout):
   - Header: 3 lines (fixed)
   - Main content: remaining space (Min(0))
   - Footer: 1 line (fixed)

2. **Horizontal split** (main content):
   - Left pane (agent list): 20% width, minimum 18 chars
   - Right pane (output): 80% width, remaining space

## Color Scheme

### Status Indicators
- Running: `Color::Green` with filled circle "●"
- Stopped: `Color::Red` with empty circle "○"
- Paused: `Color::Yellow` with filled circle "●"

### UI Elements
- Header title: `Color::Cyan`, Bold
- Header timestamp: `Color::White`
- Selected agent row: `Color::Black` on `Color::Cyan` background
- Selection arrow: `Color::Cyan`, "→"
- Agent name: `Color::White`, Bold for selected
- Uptime/Loop text: `Color::DarkGray`
- Borders: `Color::DarkGray`
- Output timestamps: `Color::DarkGray`
- Output text: `Color::White`

### Border Style
- Use `Block::bordered()` with `BorderType::Rounded`
- Title alignment: Left for most, Right for timestamp

## Components

### Header Block
- Title: "GOD AGENT CONSOLE" (left aligned, cyan, bold)
- Timestamp: Current time HH:MM:SS (right aligned)
- Full width border on bottom

### Agent List (Left Pane)
- Title: "AGENTS"
- Each agent entry shows:
  - Line 1: Arrow (if selected) + name
  - Line 2: Status dot + status text
  - Line 3: "Uptime: Xs"
  - Line 4: "Loop #N"
  - Blank line between agents
- Scrollable if many agents

### Output Pane (Right Pane)
- Title: "Agent Output: {name}"
- Scrollable list of output lines
- Each line prefixed with timestamp: "[HH:MM:SS]"
- Auto-scroll to bottom when new content arrives
- Manual scroll with Up/Down when focused

### Footer
- Single line showing keybindings
- Format: "key: action  key: action  ..."
- Styled with dim text

## Rendering Frequency
- Target: 60 FPS (16ms per frame)
- Use crossterm's event polling with timeout
- Only re-render on input or state change (or use tick for animations)
