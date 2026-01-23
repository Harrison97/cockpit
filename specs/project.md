# Ralph Project Specification

## Overview

A ralph project is a directory containing the files needed to run a ralph loop.

## Required Files

- `PROMPT.md` - The prompt that Claude reads each iteration (required)
- `IMPLEMENTATION_PLAN.md` - Task list that Claude works through (optional, created if missing)

## Optional Files

- `specs/` - Directory containing specification files
- `AGENTS.md` - Build commands and project context
- `PRIORITY_INSTRUCTIONS.md` - Urgent instructions for next iteration

## RalphProject Struct

```rust
pub struct RalphProject {
    /// Root directory of the project
    pub root: PathBuf,

    /// Path to PROMPT.md
    pub prompt_path: PathBuf,

    /// Path to IMPLEMENTATION_PLAN.md
    pub plan_path: PathBuf,

    /// Path to specs/ directory
    pub specs_dir: PathBuf,

    /// Path to PRIORITY_INSTRUCTIONS.md
    pub instructions_path: PathBuf,
}
```

## Methods

### `from_path(path: PathBuf) -> Result<Self>`

Open an existing ralph project:

1. Verify path is a directory
2. Check PROMPT.md exists
3. Set paths for other files (may not exist yet)
4. Return RalphProject

### `is_ralph_project(path: &Path) -> bool`

Quick check if a directory is a ralph project:

```rust
path.join("PROMPT.md").exists()
```

### `create(path: PathBuf, prompt_content: &str) -> Result<Self>`

Create a new ralph project:

1. Create directory if it doesn't exist
2. Write PROMPT.md with provided content
3. Create empty IMPLEMENTATION_PLAN.md:
   ```markdown
   # Implementation Plan

   ## Tasks

   - [ ] First task goes here
   ```
4. Create specs/ directory
5. Return RalphProject

### `append_instruction(&self, text: &str) -> Result<()>`

Add an instruction for the next iteration:

1. Open PRIORITY_INSTRUCTIONS.md in append mode (create if needed)
2. Write timestamp and instruction text
3. Format:
   ```markdown
   ## [2024-01-15 14:30:22]

   {instruction text}

   ---
   ```

Claude's prompt should include: "Check PRIORITY_INSTRUCTIONS.md first for urgent tasks."

### `prepend_task(&self, task: &str) -> Result<()>`

Add a high-priority task to the plan:

1. Read current IMPLEMENTATION_PLAN.md
2. Find first `- [ ]` task
3. Insert new task before it
4. Write file back

### `read_prompt(&self) -> Result<String>`

Read the current PROMPT.md content.

### `read_plan(&self) -> Result<String>`

Read the current IMPLEMENTATION_PLAN.md content.

## Project Creation UI Flow

When user presses `n` for new loop:

1. Prompt: "Project path: " (text input)
2. Prompt: "Prompt content (or 'default'): " (text input or file path)
3. Create project with RalphProject::create()
4. Add new Agent to app with this project
5. Show confirmation: "Created loop: {name}"

## Default Prompt Template

If user selects "default" prompt:

```markdown
# Task Loop

## Your Task

1. Read IMPLEMENTATION_PLAN.md for the task list
2. Pick the most important uncompleted item
3. Implement it completely
4. Run any relevant tests or checks
5. Mark the item complete and commit
6. Exit

## Rules

- One task per iteration
- Don't assume - verify by reading files
- Keep changes minimal and focused
- Commit with clear message

## Exit

After completing one task, exit immediately.
```
