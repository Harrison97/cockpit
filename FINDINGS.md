# Audit Findings

## 2026-01-23 - Hardening Loop Iteration 5

### Audit Summary

Full audit completed. No new MUST FIX or SHOULD FIX issues found.

**Verification performed:**
- Code formatting: `cargo fmt --check` - PASS
- Lint checks: `cargo clippy -- -D warnings` - PASS
- Build: `cargo build` - PASS
- Tests: `cargo test` - PASS (4 tests)

**Areas audited:**
- `.unwrap()` usage: Only in signal handler setup (acceptable fail-fast)
- `.expect()` usage: Only for compile-time regex constants (acceptable)
- `let _ =` patterns: All for cleanup paths or non-critical operations (acceptable)
- TODO/FIXME comments: None remaining
- Line counts: Verified accurate in INVENTORY.md (4195 total)
- Line number references: Fixed in CHECKLIST.md (agent.rs:296, loop_manager.rs:488)

**Status:**
- All MUST FIX issues: Resolved (4 iterations)
- SHOULD FIX (remaining): Reader thread abandonment - documented limitation
- NICE TO HAVE: Unit tests, integration tests - deferred
- Optional incomplete: History file rotation (13.4 in IMPLEMENTATION_PLAN)

---

## 2026-01-23 - Hardening Loop Iteration 4

### SHOULD FIX (Completed this iteration)

- [x] **Incorrect mutex lock pattern (double lock attempt)**
  - Location: `src/loop_manager.rs:608-617` (spawn_claude_iteration cleanup)
  - Risk: The pattern `if let Ok(...) = lock() {} else if let Err(...) = lock() {}` calls `lock()` twice.
    If mutex is poisoned, the second lock attempt will also return Err, but we waste a lock attempt
    and the logic doesn't properly handle the poisoned case since both branches call lock().
  - Fix: Changed to single `match` expression like other corrected code in the same file.
  - Note: This was a bug introduced during the earlier mutex poisoning fix iteration - the cleanup
    code at the end of spawn_claude_iteration was missed and used the wrong pattern.

---

## 2026-01-23 - Hardening Loop Iteration 3

### MUST FIX (Completed this iteration)

- [x] **Path traversal vulnerability in agent name validation**
  - Location: `src/app.rs:510-525` (InputMode::EnteringName handling)
  - Risk: User-supplied agent name was used directly in path construction without validation.
    A malicious name like `../../../tmp/evil` would escape the `.agents` directory.
  - Fix: Added validation to reject names containing `/`, `\`, or starting with `.`
  - Defense-in-depth: Added canonicalize check to verify final path stays within `.agents` directory

---

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

### Double mutex lock pattern fix - 2026-01-23

**Command**: `cargo fmt`
**Result**: PASS (no changes)

**Command**: `cargo clippy -- -D warnings`
**Result**: PASS

**Command**: `cargo build`
**Result**: PASS

**Command**: `cargo test`
**Result**: PASS (4 tests passed)

### Path traversal fix - 2026-01-23

**Command**: `cargo fmt --check`
**Result**: PASS

**Command**: `cargo clippy -- -D warnings`
**Result**: PASS

**Command**: `cargo build`
**Result**: PASS

**Command**: `cargo test`
**Result**: PASS (4 tests passed)

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
