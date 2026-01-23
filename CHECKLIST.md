# Verification Checklist

Last verified: 2026-01-23

## Memory Safety

- [x] Scrollback bounded - verified 2026-01-23 (SCROLLBACK_SIZE = 100,000 lines in agent.rs:105)
- [x] History file capped at 1MB on load - verified 2026-01-23 (MAX_HISTORY_BYTES in agent.rs:294)
- [x] Channel bounded - verified 2026-01-23 (OUTPUT_CHANNEL_SIZE = 1000 in app.rs:36)
- [ ] Search matches bounded - needs limit added (currently bounded only by scrollback size)

## Process Safety

- [x] All spawned processes tracked - verified 2026-01-23 (child handle stored, wait_for_child_exit called)
- [x] SIGTERM sent before SIGKILL - verified 2026-01-23 (wait_for_child_exit: 2s grace, then kill)
- [ ] Process group used for signal propagation - NOT IMPLEMENTED (uses portable_pty, not raw process groups)
- [x] Zombie processes reaped - verified 2026-01-23 (wait_for_child_exit loops until reaped or timeout)

## Thread Safety

- [x] All Arc<Mutex<>> access handles poisoned locks - verified 2026-01-23 (fixed this iteration)
- [x] AtomicBool uses SeqCst ordering - verified 2026-01-23 (all Ordering::SeqCst in loop_manager.rs)
- [x] No data races between PTY reader and main thread - verified 2026-01-23 (mpsc channel isolation)
- [x] Channel senders dropped on shutdown - verified 2026-01-23 (agent.ralph_loop = None in stop())

## Terminal Safety

- [x] Cleanup runs even on panic - verified 2026-01-23 (panic hook in main.rs:42-46)
- [x] Alternate screen always exited - verified 2026-01-23 (cleanup_terminal() in main.rs)
- [x] Raw mode always disabled - verified 2026-01-23 (cleanup_terminal() in main.rs)
- [x] Mouse capture always released - verified 2026-01-23 (cleanup_terminal() in main.rs)
- [x] Cursor always shown on exit - verified 2026-01-23 (cleanup_terminal() in main.rs:30)
- [x] SIGINT/SIGTERM triggers graceful shutdown - verified 2026-01-23 (signal handler in main.rs:48-64)

## File Safety

- [x] State file atomic writes - verified 2026-01-23 (write to temp, rename in persistence.rs:95-97)
- [x] History file opened in append mode - verified 2026-01-23 (OpenOptions::append in agent.rs:266)
- [x] No sensitive data in state file - verified 2026-01-23 (only paths, names, iteration count)
- [ ] Paths validated before file operations - needs review (some path validation exists)

## Performance

- [x] Tick rate 60 FPS active, 10 FPS idle - verified 2026-01-23 (main.rs:105-109)
- [x] PTY reads in 4KB chunks - verified 2026-01-23 (buf = [0u8; 4096] in loop_manager.rs:465)
- [x] Mouse events coalesced - verified 2026-01-23 (scroll_delta accumulation in main.rs:113-148)
