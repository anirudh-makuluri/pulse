# Pulse

**Your work, always in context.**

Pulse is the local-first activity and memory layer for human-AI work. It keeps
tasks, agent sessions, decisions, checkpoints, reminders, and handoffs connected
so work can continue across Claude, Codex, and other tools without rebuilding
context by hand.

> Pulse remembers what you and your agents are doing, lets work continue across
> applications and agents, and brings it back when it matters.

## Status

**Early development.** Core library is in place; CLI, background service, and desktop app are next.

| Piece | Status |
|---|---|
| Technical design | [docs/design.md](docs/design.md) |
| Activity model | [docs/activity-model.md](docs/activity-model.md) |
| Implementation roadmap | [docs/implementation-roadmap.md](docs/implementation-roadmap.md) |
| `pulse-core` (models, SQLite, config) | Done |
| `pulse-cli` | Done (IPC when service up; direct DB fallback) |
| `pulse-service` (background daemon) | Done (named-pipe JSON-RPC + poller) |
| Claude / Codex source adapters | Done (heuristic inference) |
| LLM via installed agent CLIs | Done (PR5; gated by privacy ack) |
| Tauri desktop app | Done (tasks + settings + summary + export) |

## Product principles

- **Task-first** — activities outlive individual agents and applications
- **Local-first** — immediate state, reminders, and core actions work on-device
- **Structured memory** — checkpoints, decisions, failures, and evidence beat raw transcripts
- **Transparent provenance** — every durable memory identifies its source
- **Privacy by default** — capture and sync only the context required for a user action
- **Cloud-backed durability** — optional sync makes activity memory available across sessions and agents

## Architecture (planned)

```
pulse-cli / pulse-app
          |
          +-- local JSON-RPC --> pulse-service
                                  |- SQLite activity cache
                                  |- Claude / Codex session watchers
                                  |- installed agent CLIs for summaries and handoffs
                                  `- optional sync queue --> AWS Lambda --> CockroachDB / S3
```

| Crate | Role |
|---|---|
| `pulse-core` | Domain models, SQLite store, task state machine, config |
| `pulse-cli` | Command-line interface |
| `pulse-service` | Background daemon: watchers, inference, IPC |
| `pulse-sources` | Claude / Codex session adapters |
| `pulse-llm` | Discover and call headless agent CLIs; heuristic fallback |
| `pulse-app` | Tauri desktop UI and activity timeline |

### Activity states

`Inbox` → `Today` / `Next` / `Waiting` → `Done`

The existing task model is the first activity root. Inferred tasks always land in
**Inbox** first; Pulse does not silently mark work done without strong evidence
or a check-in. Sessions, events, checkpoints, memories, reminders, artifacts,
and handoffs are being added as linked activity records.

### LLM backends

Pulse does **not** hold model-provider API keys. For intent interpretation,
summaries, and handoffs it discovers installed agent CLIs on your `PATH`, in
order:

1. `grok`
2. `claude`
3. `codex`

If none are available (or privacy ack is off), it falls back to local heuristics. You can pin or reorder backends in `config.toml`.

## Data on disk

Default root (Windows): `%LOCALAPPDATA%\Pulse\`

| Path | Purpose |
|---|---|
| `pulse.db` | Local activity cache and operational system of record |
| `config.toml` | Runtime settings (sources, local agent preference, sync opt-in, thresholds) |
| `logs/` | Service logs |
| `exports/` | User-initiated exports |
| `service.pid` | Background service PID (when running) |

Cloud sync is opt-in. When enabled, Pulse queues approved structured activity
records for its sync API, which persists durable memory in CockroachDB and may
archive approved artifacts in S3. Local actions and reminders remain available
if the cloud endpoint is unavailable.

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

### Desktop app (PR6)

```bash
cd apps/pulse-app
npm install
npm run tauri dev
```

Shows Inbox / Today / other statuses, quick add, task detail with evidence, **Summary** and **Settings** panels (sources, privacy ack, export), and polls every 4s. Uses the same DB/IPC as the CLI (service if running, else direct SQLite).

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

| `pulse service start\|stop\|status\|run` | Background service control |
| `pulse sources list\|enable\|disable\|scan` | Work-signal sources (claude/codex) |
| `pulse privacy acknowledge` | Allow agent-CLI inference (redacted remote) |
| `pulse llm status` | Which backend resolved (heuristic/grok/…) |
| `pulse summary generate\|show` | Daily summary |
| `pulse checkin list\|answer` | Light check-ins |
| `pulse export history [--format json\|md]` | Export tasks + evidence |
| `pulse service install-autostart` | Windows logon Task Scheduler entry |
| `pulse service uninstall-autostart` | Remove autostart task |
| `pulse config reload` | Reload config in running service |

Global: `--data-dir <DIR>` overrides the data root.

With the service running, task commands use JSON-RPC over a Windows named pipe. If the service is down, the CLI falls back to direct SQLite access.

### Workspace layout

```text
pulse/
  Cargo.toml
  crates/
    pulse-core/     # domain + SQLite + config + IPC
    pulse-cli/      # `pulse` binary
    pulse-service/  # background daemon + inference poller
    pulse-sources/  # Claude/Codex session adapters
    pulse-llm/      # heuristic + agent CLIs
  apps/
    pulse-app/      # Tauri desktop UI
  fixtures/         # sample session JSONL
  docs/
    design.md       # full technical design
  README.md
```

## Roadmap (MVP)

1. **PR1** — Workspace + `pulse-core` *(done)*
2. **PR2** — `pulse-cli` (list / add / done / show) *(done)*
3. **PR3** — `pulse-service` + Windows named-pipe IPC *(done)*
4. **PR4** — Claude / Codex sources + heuristic inference *(done)*
5. **PR5** — Agent CLI LLM backends, summaries, check-ins *(done)*
6. **PR6** — Tauri Inbox / Today / detail *(done)*
7. **PR7** — Settings, export, summary panel, autostart *(done)*

Details and acceptance criteria: [docs/design.md](docs/design.md).

## Non-goals (MVP)

- Full project management (Jira / Linear / Notion replacement)
- Team collaboration or mandatory cloud sync
- Surveillance-style activity tracking
- Broad third-party integrations early

## License

MIT (see workspace package metadata).
