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
  - Line 2: Status dot + status text (for Claude instances, show "INSTANCE" instead of "RUNNING")
  - Line 3: "Uptime: Xs" or "PID: X"
  - Line 4: "Loop #N" (for ralph loops only; omit for claude instances)
  - Line 5: Working directory path (truncated with ellipsis if needed)
  - Blank line between agents
- **Scrollable**: When agent count exceeds visible area:
  - Track `list_scroll_offset` in App
  - Arrow keys (j/k) move selection; auto-scroll to keep selected visible
  - Show scroll indicators (▲/▼) when more items above/below viewport

### Output Pane (Right Pane)
- Title: "{name}" with focus indicator
  - Unfocused: "{name} [Tab/Enter to focus]"
  - Focused: "{name} [FOCUSED - Tab to exit]"
- Full terminal emulation via vt100 parser
- **Scrolling**:
  - Mouse wheel scrolling when focused (via `EnableMouseCapture`)
  - Track `scroll_offset` per agent in vt100 scrollback buffer
  - Auto-scroll to bottom when new content arrives (unless user scrolled up)

### Footer
- Single line showing context-sensitive keybindings
- Format: "key: action │ key: action │ ..."
- Styled with dim text (`Color::DarkGray`)
- Content varies by state:
  - Agent list focused: "j/k: nav │ r: run │ s: stop │ p: pause │ n: new │ i: msg │ ?: help │ q: quit"
  - Output focused: "Type to interact │ Tab: back │ Scroll: mouse wheel"

## Rendering Frequency
- Target: 60 FPS (16ms per frame)
- Use crossterm's event polling with timeout
- Only re-render on input or state change (or use tick for animations)

## Terminal Setup

### Alternate Screen Mode
The application uses alternate screen to take over the entire terminal:

```rust
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};

// On startup
io::stdout().execute(EnterAlternateScreen)?;
io::stdout().execute(EnableMouseCapture)?;
enable_raw_mode()?;

// On cleanup (always runs, even on panic)
io::stdout().execute(DisableMouseCapture)?;
disable_raw_mode()?;
io::stdout().execute(LeaveAlternateScreen)?;
```

This ensures:
- User cannot scroll past the application UI
- Terminal scrollback is isolated from cockpit
- Mouse events are captured for scroll wheel handling
- On exit, terminal returns to normal state

### Mouse Capture
Enable mouse capture for:
- Scroll wheel events in the terminal pane
- Future: click-to-select in copy mode
