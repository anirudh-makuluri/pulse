# Pulse

**The todo app that stays current for you.**

Pulse is a local-first task app that turns real work activity into an always-current todo list. It captures tasks from direct input and work signals (like Claude and Codex sessions), keeps them updated as work happens, and helps you see what to do next.

> Pulse is a local-first todo app that captures real work, keeps tasks current, and shows you what to do next.

## Status

**Early development.** Core library is in place; CLI, background service, and desktop app are next.

| Piece | Status |
|---|---|
| Design | [docs/design.md](docs/design.md) |
| `pulse-core` (models, SQLite, config) | Done |
| `pulse-cli` | Done (direct DB; no service yet) |
| `pulse-service` (background daemon) | Planned |
| Claude / Codex source adapters | Planned |
| LLM via installed agent CLIs | Planned |
| Tauri desktop app | Planned |

## Product principles

- **Task-first** — a todo app, not an analytics dashboard
- **Local-first** — data lives on your machine by default
- **Low-friction** — capture and updates should feel light
- **Trustworthy** — inferred tasks carry evidence and confidence
- **Cross-platform** — Windows first; Linux-ready abstractions
- **Lightweight** — quiet background service, no cloud required

## Architecture (planned)

```
pulse-cli  ──┐
             ├── JSON-RPC (named pipe on Windows) ──► pulse-service
pulse-app  ──┘                                            │
                                                          ├── SQLite (pulse-core)
                                                          ├── Claude / Codex session watchers
                                                          └── Agent CLIs: grok → claude → codex
```

| Crate | Role |
|---|---|
| `pulse-core` | Domain models, SQLite store, task state machine, config |
| `pulse-cli` | Command-line interface |
| `pulse-service` | Background daemon: watchers, inference, IPC |
| `pulse-sources` | Claude / Codex session adapters |
| `pulse-llm` | Discover and call headless agent CLIs; heuristic fallback |
| `pulse-app` | Tauri desktop UI (later) |

### Task states

`Inbox` → `Today` / `Next` / `Waiting` → `Done`

Inferred tasks always land in **Inbox** first. You triage; Pulse does not silently mark work done without strong evidence or a check-in.

### LLM backends

Pulse does **not** hold its own API keys. For inference and daily summaries it discovers installed agent CLIs on your `PATH`, in order:

1. `grok`
2. `claude`
3. `codex`

If none are available (or privacy ack is off), it falls back to local heuristics. You can pin or reorder backends in `config.toml`.

## Data on disk

Default root (Windows): `%LOCALAPPDATA%\Pulse\`

| Path | Purpose |
|---|---|
| `pulse.db` | SQLite system of record |
| `config.toml` | Runtime settings (sources, LLM preference, thresholds) |
| `logs/` | Service logs |
| `exports/` | User-initiated exports |
| `service.pid` | Background service PID (when running) |

## Development

### Requirements

- [Rust](https://rustup.rs/) stable (see `rust-toolchain.toml`)
- Windows is the primary v0 target

### Build & test

```bash
# From repo root
cargo test -p pulse-core
cargo test -p pulse-cli
cargo build --release
```

### CLI (PR2)

```bash
# Use a temp data dir while developing
cargo run -p pulse-cli -- --data-dir ./tmp-data tasks add "Ship the CLI"
cargo run -p pulse-cli -- --data-dir ./tmp-data tasks list
cargo run -p pulse-cli -- --data-dir ./tmp-data tasks done <id-prefix>

# Default data: %LOCALAPPDATA%\Pulse\
cargo run -p pulse-cli -- tasks list
cargo run -p pulse-cli -- config show
```

| Command | Description |
|---|---|
| `pulse tasks list [--status …] [--json]` | List tasks |
| `pulse tasks add <title> [--today] [--notes …]` | Create task |
| `pulse tasks show <id>` | Detail + evidence |
| `pulse tasks done <id>` | Mark done |
| `pulse tasks update <id> [--title] [--status] [--notes]` | Patch fields |
| `pulse tasks move <id> <status>` | Change status |
| `pulse config show` / `path` | Config |
| `pulse version` | Version |

Global: `--data-dir <DIR>` overrides the data root.

### Workspace layout

```text
pulse/
  Cargo.toml
  crates/
    pulse-core/     # domain + SQLite + config
    pulse-cli/      # `pulse` binary
  docs/
    design.md       # full technical design
  README.md
```

## Roadmap (MVP)

1. **PR1** — Workspace + `pulse-core` *(done)*
2. **PR2** — `pulse-cli` (list / add / done / show) *(done)*
3. **PR3** — `pulse-service` + Windows named-pipe IPC
4. **PR4** — Claude / Codex sources + heuristic inference
5. **PR5** — Agent CLI LLM backends, summaries, check-ins
6. **PR6** — Tauri Inbox / Today / detail
7. **PR7** — Settings, export, summary panel, autostart

Details and acceptance criteria: [docs/design.md](docs/design.md).

## Non-goals (MVP)

- Full project management (Jira / Linear / Notion replacement)
- Team collaboration or cloud sync by default
- Surveillance-style activity tracking
- Broad third-party integrations early

## License

MIT (see workspace package metadata).
