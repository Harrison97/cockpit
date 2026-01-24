# Cockpit

A UI to Pilot AI Agents

````
┌─────────────────────────────────────────────────────────────────────────────┐
│ COCKPIT                                                          14:32:07   │
├───────────────────────┬─────────────────────────────────────────────────────┤
│ Agents                │ Agent Output: architect                             │
│                       │                                                     │
│ ▶ architect           │  Looking at the codebase structure...               │
│   ● RUNNING           │                                                     │
│   iter #23            │  I'll refactor the auth module to use the new       │
│   [loop]              │  token validation pattern we established.           │
│                       │                                                     │
│   builder             │  ```typescript                                      │
│   ◐ PAUSED            │  export class AuthService {                         │
│   iter #8             │    constructor(private tokenValidator: Validator)   │
│   [loop]              │  }                                                  │
│                       │  ```                                                │
│   scratch             │                                                     │
│   ○ STOPPED           │  ✓ feat(auth): refactor to dependency injection     │
│   [claude]            │                                                     │
├───────────────────────┴─────────────────────────────────────────────────────┤
│ j/k: nav │ r: run │ s: stop │ p: pause │ n: new │ i: import │ ?: help       │
└─────────────────────────────────────────────────────────────────────────────┘
````

## The Idea

The most capable AI agents aren't magic. They're loops.

```bash
while true; do
  cat PROMPT.md | claude --dangerously-skip-permissions
done
```

Give Claude a task, let it work, let it finish, repeat. This primitive scales further than you'd expect.

Cockpit gives you a workspace to run these loops, observe them, and iterate on them.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) — `npm install -g @anthropic-ai/claude-code`
- macOS or Linux

### Install

```bash
git clone https://github.com/Harrison97/cockpit
cd cockpit
cargo build --release
./target/release/cockpit
```

### Create an Agent

1. Press `n`
2. Enter the path to a codebase
3. Name it
4. Enter a prompt — or leave empty for a plain Claude session

Press `r` to run. Press `Space` to focus the terminal.

## Two Modes

**Claude Instances** — Leave the prompt empty. You get a standard Claude Code session. Useful for quick tasks or when you want to drive.

**Ralph Loops** — Enter a prompt. Claude works until it commits or says it's done, then restarts. Useful for larger tasks that benefit from iteration.

Even if you never touch Ralph loops, running one or two Claude sessions in cockpit beats wrestling with them in a normal terminal. Scrollback that actually works, pause and resume, persistent history, clean switching between sessions.

## Controls

| Key     | Action           |
| ------- | ---------------- |
| `j/k`   | Navigate agents  |
| `r`     | Run / resume     |
| `s`     | Stop             |
| `p`     | Pause            |
| `n`     | New agent        |
| `d`     | Delete           |
| `i`     | Import from disk |
| `Space` | Focus terminal   |
| `?`     | Help             |

When focused: `Tab` to go back, `Ctrl+F` to search, `Ctrl+C` to interrupt.

## The Lab

Everything lives in `.cockpit/`:

```
.cockpit/
├── state.json
├── agents/
│   └── my-agent/
│       ├── PROMPT.md
│       └── history.log
└── logs/
```

Each agent is just a directory with a prompt file. Edit it directly, version control it, copy it between projects.

History persists. Close cockpit, reopen it, scroll back through what happened.

## Going Further

The prompt is the agent. A well-crafted one can do more than you'd think.

You can pause a running agent, edit its prompt, and resume. You can point an agent at cockpit itself. You can use agents to build other agents.

How far this goes is up to you.

---

```bash
cargo build --release
cargo test
COCKPIT_LOG=debug cargo run  # verbose logging
```
