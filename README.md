# Pulse

<p align="center">
  <img src="logo.png" alt="Pulse firefly logo" width="144">
</p>

<p align="center"><strong>The activity layer for your work.</strong></p>

Pulse is a local-first Windows app that turns activity across your tools into a
clear view of what is in progress, what needs attention, and what to do next.
It keeps tasks, supported AI sessions, reminders, evidence, and handoffs in
one private workspace, so returning to work does not mean reconstructing the
context by hand.

[Download Pulse for Windows](https://github.com/anirudh-makuluri/pulse/releases/latest/download/Pulse-Setup-x64.exe) | [View releases](https://github.com/anirudh-makuluri/pulse/releases/latest) | [Read the roadmap](docs/implementation-roadmap.md)

> **Early release:** Pulse is currently for Windows and under active
> development. Your work data stays on your machine by default.

## What Pulse does

- Gives you a lightweight flow from **Inbox** to **Today**, **Next**,
  **Waiting**, and **Done**.
- Captures tasks quickly and keeps the next action visible alongside the task.
- Lets you set a task's observed outcome to **In progress** or **Completed**
  without changing its workflow status.
- Brings user-enabled Claude and Codex session activity into the same work
  view. **Sync latest sessions** reviews up to five recently modified
  transcripts from each enabled source and retains supporting evidence locally.
- Shows the work to focus on now, items that need triage, and unfinished work
  you can continue.
- Includes a **Task Copilot** for asking grounded questions such as "What
  should I work on today?", "Add a task to review the proposal", or "Move
  the proposal task to Today." It can create tasks and update their editable
  fields, but cannot delete or schedule tasks.
- Streams Copilot progress while it plans and looks up task context, then links
  the tasks that support its answer. Copilot conversations are saved locally;
  the UI exposes the five most recently updated sessions.
- Records a chronological activity timeline for tasks, sessions, reminders,
  checkpoints, evidence, memories, and artifacts.
- Supports local reminders, daily summaries, JSON or Markdown exports, app
  updates, and a desktop companion for quick access.

## Using session sync

1. In **Sources**, enable the local Claude and/or Codex session folders you
   want Pulse to watch.
2. Ensure a supported agent CLI is installed and signed in. Pulse resolves the
   first available CLI from its configured preference order.
3. Pulse automatically checks the latest sessions once an hour while its local
   service is running. Choose **Sync latest sessions** in the sidebar whenever
   you want to run that check immediately. Pulse creates or updates Inbox tasks
   when it finds concrete user-requested work.

Session sync requires permission to use the configured remote agent CLI. Pulse
shows the resolved backend in **Settings**. A sync that finds no actionable
work may complete without adding tasks.

## Using Task Copilot

Open **Task Copilot** from the sidebar and ask a question about your work. The
first prompt begins a local conversation; subsequent prompts stay in that
conversation. Choose **History** in the top-right corner to reopen one of the
five most recently updated conversations, or start a new one.

To answer a question, the Copilot may make up to two local tool calls. It can
look up tasks to ground an answer or an edit:

- list tasks, optionally by workflow status;
- search task titles, notes, projects, and suggested next actions; or
- open the current fields for a task returned by an earlier lookup.

For an explicit request, the Copilot can also create a task or update one task's
title, workflow status, notes, project, suggested next action, or observed
outcome. It cannot delete tasks, create reminders, or schedule work. Pulse's
service owns and validates every tool call, and the model can only cite tasks
returned by those tools.

When a configured remote agent CLI is approved, the Copilot uses it to choose
lookups and write the response. Otherwise Pulse uses its local heuristic
fallback. Progress travels over a loopback-only WebSocket between the local
service and desktop app; it is not a network service exposed to other devices.

## Desktop behavior

Pulse runs a local background service and system-tray icon while it is open.
The top-right close control hides the main window, keeping Pulse available from
the tray and, when enabled, the desktop companion. Use the tray or companion
context menu and choose **Close Pulse** to exit the app and its bundled service.

## Local-first and private by default

Pulse is designed to stay useful without handing your work history to a
service.

- Its SQLite database, configuration, logs, and exports live under
  `%LOCALAPPDATA%\Pulse\`.
- Claude and Codex sources remain off until you enable them.
- Pulse does not store model-provider API keys. It can use a supported agent
  CLI already installed on your `PATH` when you explicitly allow it.
- Remote CLI-backed summaries and Copilot responses require a privacy
  acknowledgement; otherwise Pulse uses local heuristics. When enabled, the
  approved CLI may receive the Copilot question and the bounded task results it
  explicitly requested.
- Copilot conversations, including questions, answers, and cited-task
  snapshots, are stored in the local SQLite database.
- Cloud sync is not required for tasks, reminders, sources, or the desktop
  experience.

## Install on Windows

1. [Download the latest Pulse installer](https://github.com/anirudh-makuluri/pulse/releases/latest/download/Pulse-Setup-x64.exe).
2. Run `Pulse-Setup-x64.exe`.
3. Open **Pulse** from the Start menu. Its local service starts automatically.

Pulse does not require Rust, Node.js, or an agent API key to install or use.

### Updating Pulse

Open **Settings** in Pulse and choose **Check for updates**. When an update is
available, Pulse downloads the signed installer, verifies it, and lets Windows
apply it without needing to download and run the setup file yourself.

If Windows shows a reputation warning, review the publisher and release page
before choosing whether to continue. Releases are not yet code-signed.

## How it fits together

```text
Claude / Codex sessions       Pulse desktop app / CLI
          |                             |
          +--------- work signals ------+
                                        |
                                        v
                    pulse-service (local background service)
                                        |
       activity capture, reminders, Copilot tools, summaries, and JSON-RPC
                    (plus loopback WebSocket progress)
                                        |
                                        v
                    SQLite at %LOCALAPPDATA%\Pulse\pulse.db
```

The desktop app bundles the local service and CLI pieces it needs. If the
service is unavailable, the regular task views can still read and update the
local database directly; Task Copilot requires the running local service for
its bounded tool loop and progress stream.

## CockroachDB × AWS agentic memory

Pulse is local-first by default, with an opt-in durable activity-memory layer
for cross-session continuity. It stores approved structured activities,
checkpoints, memories, reminders, and embeddings in CockroachDB; raw agent
transcripts and local files are not synced automatically.

```text
Pulse service
  | approved structured memory + local MiniLM embedding
  v
SQLite sync outbox --> API Gateway + AWS Lambda --> CockroachDB pulse_* tables
                                |                    |
                                v                    v
                           private S3 archive    VECTOR(384) retrieval
                                                     |
                                                     v
                                         Copilot / Inbox semantic search
```

### CockroachDB tools

- **Distributed Vector Indexing:** Pulse generates a local 384-dimensional
  MiniLM embedding for approved activity memory. Lambda stores it in
  CockroachDB's `VECTOR(384)` column, and CockroachDB performs cosine
  nearest-neighbor retrieval.
- **CockroachDB Cloud Managed MCP Server:** a separately authenticated,
  read-only MCP connection lets an approved coding agent inspect the live Pulse
  activity-memory schema and run `SELECT` queries. See the
  [safe setup and verification guide](docs/cockroach-managed-mcp.md).

When cloud sync is enabled, Task Copilot receives a read-only
`search_cloud_memory` tool for questions about prior work, decisions,
checkpoints, or continuity context. That tool generates the query embedding
locally, calls the authenticated Lambda search endpoint, and gives the retrieved
CockroachDB memory to the Copilot as reference context. The sync token and any
CockroachDB credential remain in the service/AWS path and are never sent to the
renderer or model.

### AWS services

- **AWS Lambda + API Gateway:** authenticate sync and semantic-search requests,
  validate embeddings, and idempotently write durable activity memory.
- **Amazon S3:** privately and versionedly archives explicitly approved
  checkpoint payloads and artifacts through short-lived upload URLs.

### Run the memory demo

1. Follow [CockroachDB memory setup](docs/cloud-memory-setup.md) and the
   [AWS deployment guide](infra/aws/README.md), then enable `[sync]` in
   `%LOCALAPPDATA%\Pulse\config.toml`.
2. Create or sync an activity with a distinctive decision or checkpoint; wait
   for the sync worker to deliver it.
3. In Task Copilot, ask, for example: **“What do you remember about the
   authentication refactor decision?”** Copilot uses the read-only cloud-memory
   tool and identifies the returned CockroachDB context in its response.
4. Configure the Managed MCP server using
   [the MCP guide](docs/cockroach-managed-mcp.md) and ask it to inspect the same
   `pulse_activities` or `pulse_embeddings` record. Its OAuth consent remains
   read-only.

## For developers

### Requirements

- Rust stable (see `rust-toolchain.toml`)
- Node.js and npm
- Windows for the current desktop build

### Run the desktop app

```powershell
cd apps/pulse-app
npm install
npm run tauri dev
```

The normal desktop dev and build commands run `prepare:sidecar` first. On
Windows it removes an orphaned debug `pulse-service.exe` left by an interrupted
dev session, preventing Tauri's sidecar-copy `Access is denied` error. If a
Pulse desktop window is still running, close it before rebuilding.

### Run the landing page

```powershell
cd apps/pulse-web
npm install
npm run dev
```

### Test and build

```powershell
# From the repository root
cargo test

# Build the Windows NSIS installer
cd apps/pulse-app
npx tauri build --bundles nsis
```

The installer is written to:

```text
target\release\bundle\nsis\Pulse_<version>_x64-setup.exe
```

### Publishing an update

Create a release from the repository root with one command:

```powershell
.\scripts\release.ps1 0.1.8 -Push
```

The script validates the SemVer version, updates the desktop app's version in
`package.json`, `package-lock.json`, `Cargo.toml`, and `tauri.conf.json`, then
commits the change and creates the matching annotated `v0.1.8` tag. `-Push`
pushes both the commit and tag to `origin`, which triggers the release
workflow. Omit it to prepare the release locally first, or preview the work
with `-DryRun`. A release run requires a clean working tree so the version
commit and tag contain only the intended release changes.

### CLI

```powershell
# Use a temporary data directory while developing
cargo run -p pulse-cli -- --data-dir ./tmp-data tasks add "Ship Pulse"
cargo run -p pulse-cli -- --data-dir ./tmp-data tasks list

# Default data directory: %LOCALAPPDATA%\Pulse\
cargo run -p pulse-cli -- tasks list
cargo run -p pulse-cli -- service start
```

Useful commands include `pulse tasks`, `pulse sources`, `pulse summary`,
`pulse checkin`, `pulse export`, `pulse service`, and `pulse config`.

## Repository layout

```text
apps/pulse-app/       Tauri desktop app
apps/pulse-web/       Pulse landing page
crates/pulse-core/    Domain models, SQLite, configuration, and IPC client
crates/pulse-cli/     Command-line interface
crates/pulse-service/ Local background daemon, reminders, and IPC server
crates/pulse-sources/ Claude and Codex session adapters
crates/pulse-llm/     Agent CLI discovery and heuristic fallback
docs/                 Product and technical documentation
```

## License

MIT
