# Cockpit - Ralph Loop Control System

You are building a CLI tool called "Cockpit" for creating, deploying, observing, and intervening with ralph loops (autonomous Claude Code agents).

## What is a Ralph Loop?

A ralph loop is a bash loop that continuously runs Claude Code:

```bash
while :; do cat PROMPT.md | claude -p --dangerously-skip-permissions; done
```

Each iteration: Claude reads the prompt, picks a task from IMPLEMENTATION_PLAN.md, implements it, commits, and exits. The loop restarts with fresh context.

## Your Task

1. Read `@IMPLEMENTATION_PLAN.md` to see the prioritized task list
2. Choose the **most important uncompleted item** to work on
3. Implement that single item completely
4. Run quality checks: `cargo build`, `cargo clippy -- -D warnings`
5. If checks pass, mark the item complete in IMPLEMENTATION_PLAN.md and commit
6. Exit when done with that one item

## Critical Rules

1. **One task per iteration.** Complete one item, commit, exit. Fresh context next loop.

2. **Don't assume functionality exists.** Always check the actual source files before assuming code is there. Read the file first.

3. **Tests must pass.** Never commit if `cargo build` fails or clippy has warnings.

4. **Follow the specs.** Read the specification files in `specs/` for exact requirements:
    - `specs/tui_design.md` - Layout, colors, visual design
    - `specs/keybindings.md` - Keyboard shortcuts
    - `specs/loop_manager.md` - Subprocess management
    - `specs/project.md` - Ralph project file operations

5. **Keep it simple.** Implement exactly what's specified. No extra features.

6. **Commit message format:**

    ```
    feat: <short description>

    - What was implemented
    - Any notable decisions
    ```

## Reference Files

- `@AGENTS.md` - Build commands and project structure
- `@specs/` - All specification files

## Quality Checklist Before Commit

- [ ] `cargo build` succeeds
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt` applied
- [ ] Code matches specification
- [ ] IMPLEMENTATION_PLAN.md updated (item marked complete)

## Exit Behavior

After completing ONE task and committing:

- Update IMPLEMENTATION_PLAN.md to mark the item done
- Commit all changes
- Run cmd: say "I'm done with {task}."
- Exit immediately

The outer loop will restart you with fresh context for the next task.
