# Keybindings Specification

## Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `j` | Move down | Select next agent in list |
| `k` | Move up | Select previous agent in list |
| `Down` | Move down | Same as `j` |
| `Up` | Move up | Same as `k` |
| `g` | Go to top | Select first agent |
| `G` | Go to bottom | Select last agent |

## Agent Control

| Key | Action | Description |
|-----|--------|-------------|
| `p` | Pause | Pause the selected agent (sends SIGSTOP) |
| `r` | Resume | Resume the selected agent (sends SIGCONT) |
| `s` | Stop | Stop the selected agent (sends SIGTERM) |
| `S` | Start | Start a stopped agent |

## Loop Management

| Key | Action | Description |
|-----|--------|-------------|
| `n` | New loop | Create a new ralph loop project |
| `i` | Instruct | Send instruction to selected loop |
| `d` | Delete | Remove loop from cockpit (doesn't delete files) |
| `o` | Open | Open loop project in file manager |

## Output Pane

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Toggle focus | Switch focus to output pane for scrolling |
| `Esc` | Unfocus | Return focus to agent list |
| `Ctrl+d` | Page down | Scroll output down half page (when focused) |
| `Ctrl+u` | Page up | Scroll output up half page (when focused) |

## Application

| Key | Action | Description |
|-----|--------|-------------|
| `q` | Quit | Exit the application |
| `Ctrl+c` | Quit | Exit the application |

## Implementation Notes

### Event Handling
Use crossterm's `event::read()` with `poll()` for non-blocking input:

```rust
use crossterm::event::{self, Event, KeyCode, KeyModifiers};

if event::poll(Duration::from_millis(16))? {
    if let Event::Key(key) = event::read()? {
        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Char('p') => app.pause_selected(),
            KeyCode::Char('r') => app.resume_selected(),
            KeyCode::Enter => app.toggle_focus(),
            // ... etc
        }
    }
}
```

### Focus State
Track whether output pane is focused:
```rust
struct App {
    // ...
    output_focused: bool,
    scroll_offset: usize,
}
```

When `output_focused` is true:
- j/k scroll the output instead of changing selection
- Show visual indicator that output is focused (brighter border)
- Esc returns focus to agent list

### Footer Updates
Show different keybindings based on focus state:
- Agent list focused: "j/k: navigate  n: new  i: instruct  p: pause  s: stop  q: quit"
- Output focused: "j/k: scroll  Esc: back  Ctrl+d/u: page  q: quit"
