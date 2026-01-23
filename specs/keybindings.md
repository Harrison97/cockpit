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
| `r` | Run/Resume | Start if stopped, resume if paused |
| `p` | Pause | Pause the selected agent (sends SIGSTOP) - ralph loops only |
| `s` | Stop | Stop the selected agent (sends SIGTERM) |

Note: Claude instances (agents without PROMPT.md) cannot be paused.

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
| `Enter`/`Tab` | Toggle focus | Switch focus to output pane for interaction |
| `Tab` | Unfocus | Return focus to agent list (when focused) |
| `Space` | Focus | Alternative way to focus output pane |
| Mouse wheel | Scroll | Scroll through terminal history (when focused) |
| `Ctrl+F` | Search | Enter search mode (vim-style navigation) |

## Search Mode (Ctrl+F)

When in search mode, the terminal pane becomes a read-only view with vim-style navigation.

### Entering Search
| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+F` | Start search | Enter search mode with query input |
| Type | Update query | Incremental search as you type |
| `Enter` | Confirm | Switch to navigation mode with current matches |
| `Esc` | Cancel | Exit search mode, return to focused mode |

### Navigation Mode (after search confirmed)
| Key | Action | Description |
|-----|--------|-------------|
| `n` | Next match | Jump to next search match |
| `N` | Prev match | Jump to previous search match |
| `j` / `Down` | Scroll down | Scroll down one line |
| `k` / `Up` | Scroll up | Scroll up one line |
| `Ctrl+D` | Half page down | Scroll down half a page |
| `Ctrl+U` | Half page up | Scroll up half a page |
| `g` | Go to top | Scroll to beginning of history |
| `G` | Go to bottom | Scroll to end (live output) |
| `q` / `Esc` | Exit | Exit search mode, return to focused mode |

Note: While in search mode, no input is sent to the subprocess. This is a read-only view mode.

## Application

| Key | Action | Description |
|-----|--------|-------------|
| `q` | Quit | Exit the application |
| `Ctrl+c` | Quit | Exit the application |

## Implementation Notes

### Event Handling
Use crossterm's `event::read()` with `poll()` for non-blocking input:

```rust
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEvent, MouseEventKind};

if event::poll(Duration::from_millis(16))? {
    match event::read()? {
        Event::Key(key) => {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                KeyCode::Char('p') => app.pause_selected(),
                KeyCode::Char('r') => app.run_or_resume_selected(), // Start if stopped, resume if paused
                KeyCode::Char('s') => app.stop_selected(),
                KeyCode::Enter | KeyCode::Tab => app.toggle_focus(),
                // ... etc
            }
        }
        Event::Mouse(mouse) => {
            // Handle mouse wheel scrolling when output focused
            if app.output_focused {
                match mouse.kind {
                    MouseEventKind::ScrollUp => app.scroll_terminal_up(),
                    MouseEventKind::ScrollDown => app.scroll_terminal_down(),
                    _ => {}
                }
            }
        }
        _ => {}
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
- Agent list focused: "j/k: nav │ r: run │ s: stop │ p: pause │ n: new │ i: msg │ ?: help │ q: quit"
- Output focused: "Type to interact │ Tab: back │ Scroll: mouse wheel"
