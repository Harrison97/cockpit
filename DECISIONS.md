# Decision Log

## 2025-01-23 - History Storage Location

**Context**: History files were stored in ~/.cockpit/agents/{name}/ but this separated agent data from project data.

**Options Considered**:
1. Keep in ~/.cockpit/ (global location)
   - Pro: Central location for all cockpit data
   - Con: Data separated from project, harder to backup
2. Store in .agents/{name}/ (project folder)
   - Pro: All agent data together, travels with project
   - Con: Different from state.json location

**Decision**: Store history in .agents/{name}/history.log

**Rationale**: Agents are project-specific, their data should live with their configuration.

**Consequences**: Must handle case where project_path doesn't exist. Old history files in ~/.cockpit/ orphaned.

---

## 2025-01-23 - Lazy History Loading

**Context**: History was loaded at agent creation time with fixed terminal size (180x40), causing formatting issues when actual display size differed.

**Options Considered**:
1. Load at creation, accept formatting issues
   - Pro: Simple implementation
   - Con: History looks wrong on different terminal sizes
2. Load lazily on first resize
   - Pro: History loaded at correct size
   - Con: Slight complexity, need history_loaded flag
3. Don't preserve history across sessions
   - Pro: Simplest
   - Con: Loses valuable context

**Decision**: Lazy loading on first resize

**Rationale**: Terminal size unknown until first render, loading at correct size ensures proper formatting.

**Consequences**: Added history_loaded flag to Agent. First resize may be slightly slower.

---

## 2025-01-23 - Terminal Reset on Start Removed

**Context**: Agent::start() called reset_terminal() which wiped loaded history.

**Options Considered**:
1. Keep reset_terminal() for clean state
   - Pro: Each session starts fresh
   - Con: Loses history, user loses context
2. Remove reset_terminal(), preserve history
   - Pro: History preserved across start/stop
   - Con: Could accumulate stale state

**Decision**: Remove reset_terminal(), only reset scroll_offset

**Rationale**: History is valuable for observing agent behavior. Scroll reset ensures user sees new output.

**Consequences**: Old terminal state persists. If terminal gets corrupted, user must delete agent and recreate.

---

## Template for Future Decisions

```markdown
## [DATE] - [Decision Title]

**Context**: [Why this decision was needed]

**Options Considered**:
1. [Option A]
   - Pro: [benefit]
   - Con: [drawback]
2. [Option B]
   - Pro: [benefit]
   - Con: [drawback]

**Decision**: [What was chosen]

**Rationale**: [Why this option was selected]

**Consequences**: [What this enables/prevents/requires]
```
