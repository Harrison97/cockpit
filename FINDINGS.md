# Audit Findings

## 2026-01-23 - Hardening Loop Iteration 1

### MUST FIX (Completed this iteration)

- [x] **Mutex unwrap() calls in spawn_claude_iteration**
  - Location: `src/loop_manager.rs:351,352,449,452,457,529,547,579,580`
  - Risk: If any thread panics while holding a mutex, subsequent lock attempts would panic, potentially corrupting terminal state or leaving orphaned processes
  - Fix: Replace all `.unwrap()` on mutex locks with proper poisoned mutex handling using `into_inner()` to recover the guard

### SHOULD FIX (Next iterations)

- [ ] **Unbounded search_matches Vec growth**
  - Location: `src/app.rs:67-69`
  - Risk: When searching large scrollback buffers, `search_matches` could grow very large
  - Note: Bounded by SCROLLBACK_SIZE (100K lines), but could still hold thousands of matches
  - Suggested fix: Add match limit (e.g., 10K matches) or use lazy iteration

- [ ] **Reader thread abandonment on timeout**
  - Location: `src/loop_manager.rs:564-576`
  - Risk: If reader thread doesn't exit within 2s, it's abandoned without join
  - Note: Thread will be cleaned up when it eventually exits, but could accumulate if PTY reads block indefinitely
  - Suggested fix: Consider using a PTY with non-blocking reads or a dedicated signal mechanism

### NICE TO HAVE

- [ ] **Add unit tests for mutex poisoning recovery**
  - Verify that the system continues operating correctly when a mutex is poisoned

- [ ] **Add integration test for clean shutdown**
  - Verify no zombie processes remain after `App::shutdown()`

## Verification Evidence

### Mutex poisoning fix - 2026-01-23

**Command**: `cargo clippy -- -D warnings`
**Result**: PASS

**Command**: `cargo build`
**Result**: PASS

**Command**: `cargo test`
**Result**: PASS (4 tests passed)
