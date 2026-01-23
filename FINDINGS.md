# Audit Findings

## 2026-01-23 - Hardening Loop Iteration 2

### SHOULD FIX (Completed this iteration)

- [x] **Unbounded search_matches Vec growth**
  - Location: `src/agent.rs:638` (find_all_matches function)
  - Risk: When searching large scrollback buffers (100K lines), search_matches_absolute could grow unbounded
  - Fix: Added MAX_SEARCH_MATCHES constant (10,000) and early-exit logic when limit is reached
  - Note: find_visible_matches is inherently bounded by visible terminal size (rows × cols)

### SHOULD FIX (Remaining)

- [ ] **Reader thread abandonment on timeout**
  - Location: `src/loop_manager.rs:589-605`
  - Risk: If reader thread doesn't exit within 2s, it's abandoned without join
  - Note: Thread will be cleaned up when it eventually exits, but could accumulate if PTY reads block indefinitely
  - Suggested fix: Consider using a PTY with non-blocking reads or a dedicated signal mechanism

### NICE TO HAVE

- [ ] **Add unit tests for mutex poisoning recovery**
  - Verify that the system continues operating correctly when a mutex is poisoned

- [ ] **Add integration test for clean shutdown**
  - Verify no zombie processes remain after `App::shutdown()`

---

## 2026-01-23 - Hardening Loop Iteration 1

### MUST FIX (Completed)

- [x] **Mutex unwrap() calls in spawn_claude_iteration**
  - Location: `src/loop_manager.rs:351,352,449,452,457,529,547,579,580`
  - Risk: If any thread panics while holding a mutex, subsequent lock attempts would panic, potentially corrupting terminal state or leaving orphaned processes
  - Fix: Replace all `.unwrap()` on mutex locks with proper poisoned mutex handling using `into_inner()` to recover the guard

## Verification Evidence

### Search matches limit fix - 2026-01-23

**Command**: `cargo clippy -- -D warnings`
**Result**: PASS

**Command**: `cargo build`
**Result**: PASS

**Command**: `cargo test`
**Result**: PASS (4 tests passed)

### Mutex poisoning fix - 2026-01-23

**Command**: `cargo clippy -- -D warnings`
**Result**: PASS

**Command**: `cargo build`
**Result**: PASS

**Command**: `cargo test`
**Result**: PASS (4 tests passed)
