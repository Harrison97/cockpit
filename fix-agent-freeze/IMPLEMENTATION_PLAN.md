# Implementation Plan: Fix Agent Freeze Bug

## Problem Summary

Agents appear frozen because `send_input()` silently drops ALL keyboard input when `ProcessState != Ready`. This includes Ctrl+C, making it impossible for users to interrupt long-running operations.

## Fixes (in priority order)

### 1. Allow Interrupt Signals Through During Starting State
- [x] **Task**: Modify `send_input()` in `src/agent.rs` to allow Ctrl+C (byte `0x03`) through when state is `Starting`
- **File**: `src/agent.rs` around line 556-568
- **Change**: Add special case before the `ProcessState::Ready` check to let interrupt signals pass through
- **Rationale**: Users must be able to interrupt stuck operations. Ctrl+C should never be silently dropped.
- **Note**: Still block input during `Stopping` and `Exiting` to allow graceful shutdown sequences

### 2. Add Timeout for Stuck Starting State
- [x] **Task**: Add a timeout mechanism that transitions from `Starting` to `Ready` after 30 seconds
- **File**: `src/agent.rs`
- **Change**: Track when `Starting` state began. In `process_terminal_data()`, if in `Starting` for >30s without seeing ready indicator, auto-transition to `Ready`
- **Rationale**: Prevents permanent freeze if Claude never outputs the prompt character
- **Fields to add**: `starting_since: Option<Instant>` to Agent struct

### 3. Add More Ready Indicators
- [x] **Task**: Expand `READY_INDICATORS` to include more patterns that indicate Claude is ready
- **File**: `src/agent.rs` around line 363
- **Add indicators**:
  - `"Thinking"` - Claude's thinking indicator
  - `">"` - Alternative prompt character
  - `"$"` - Shell prompt (for tool execution)
- **Rationale**: More patterns = faster transition to Ready state

### 4. Log When Input is Dropped
- [x] **Task**: Add tracing::debug log when input is dropped due to state check
- **File**: `src/agent.rs` in `send_input()`
- **Change**: Before returning `Ok(())` for non-Ready state, log `debug!("Input dropped: process_state={:?}", self.process_state)`
- **Rationale**: Helps debugging; makes silent drops observable

### 5. Add Visual Indicator for Not-Ready State
- [x] **Task**: Show "(starting...)" or similar in the UI when agent is not Ready
- **File**: `src/ui.rs` in the agent status rendering
- **Change**: Display `process_state` alongside `AgentStatus` when not Ready
- **Rationale**: Users can see why their input isn't working

## Verification

After all fixes, verify:
1. Ctrl+C works to interrupt during `Starting` state
2. Agent auto-transitions to Ready after timeout
3. New ready indicators trigger faster transitions
4. Debug logs appear when input is dropped (when running with RUST_LOG=debug)
5. UI shows when agent is not ready for input

## Files Modified

- `src/agent.rs` - Main fixes (tasks 1-4)
- `src/ui.rs` - Visual indicator (task 5)
