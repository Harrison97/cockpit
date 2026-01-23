# Changelog

All notable changes to this project will be documented in this file.

## 2026-01-23

### fix: prevent infinite loop in wait_for_child_exit if kill fails
- Added `killed` flag to track whether kill signal was already sent
- Added 3s total timeout (2s grace period + 1s post-kill wait)
- Previously: If child.kill() failed or process ignored SIGKILL, the function would loop forever
- Now: Function gives up after 3s total, accepting potential zombie (which is better than hanging)
- Verification: cargo fmt, cargo clippy -- -D warnings, cargo build, cargo test

### fix: handle mutex poisoning in main-thread accessible RalphLoop methods
- Fixed send_input(), start(), resume(), stop() to use `if let Ok(guard) = lock()` pattern
- Previously: .lock().unwrap() would panic and crash UI if background thread panicked while holding lock
- Now: Gracefully handles poisoned mutex by ignoring optional operations (activity timer updates)
- Critical operations (pty_writer.lock in send_input) already used .map_err() pattern
- Verification: cargo fmt, cargo clippy -- -D warnings, cargo build, cargo test

### fix: proper thread join timeout in RalphLoop::stop()
- Removed dead SIGTERM/SIGKILL code (self.pid was never set, always None)
- Removed unused nix crate imports (kill, Signal, Pid)
- Added proper timeout loop for reader thread join (3s timeout)
- Uses is_finished() polling instead of blocking join()
- Verification: cargo fmt, cargo clippy -- -D warnings, cargo build, cargo test

---

## 2025-01-23

### feat: add scroll position indicator to terminal pane
- Show "Line X / Y" format when scrolled up in history
- Display "[LIVE]" indicator when following output at bottom
- Verification: cargo build, cargo clippy -- -D warnings

### fix: history storage location and loading
- Changed history storage from ~/.cockpit/agents/ to .agents/{name}/
- Implemented lazy history loading on first resize
- Removed reset_terminal() call that wiped history on start
- Verification: cargo build, cargo clippy -- -D warnings

---

## Template for Future Entries

```markdown
## [DATE]

### <type>: <short description>
- What changed
- Why it changed
- Verification: [commands run]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `refactor`: Code restructuring
- `docs`: Documentation
- `audit`: Security/correctness audit
- `perf`: Performance improvement
