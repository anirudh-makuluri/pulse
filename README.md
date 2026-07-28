# Pulse

<p align="center">
  <img src="apps/pulse-app/public/pulse-logo.png" alt="Pulse firefly logo" width="144">
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
- Brings user-enabled Claude and Codex session activity into the same work
  view, with the supporting evidence retained locally.
- Shows the work to focus on now, items that need triage, and unfinished work
  you can continue.
- Records a chronological activity timeline for tasks, sessions, reminders,
  checkpoints, evidence, memories, and artifacts.
- Supports local reminders, daily summaries, exports, settings, and a desktop
  companion for quick access.

## Local-first and private by default

Pulse is designed to stay useful without handing your work history to a
service.

- Its SQLite database, configuration, logs, and exports live under
  `%LOCALAPPDATA%\Pulse\`.
- Claude and Codex sources remain off until you enable them.
- Pulse does not store model-provider API keys. It can use a supported agent
  CLI already installed on your `PATH` when you explicitly allow it.
- Remote CLI-backed summaries require a privacy acknowledgement; otherwise
  Pulse uses local heuristics.
- Cloud sync is not required for tasks, reminders, sources, or the desktop
  experience.

## Install on Windows

1. [Download the latest Pulse installer](https://github.com/anirudh-makuluri/pulse/releases/latest/download/Pulse-Setup-x64.exe).
2. Run `Pulse-Setup-x64.exe`.
3. Open **Pulse** from the Start menu. Its local service starts automatically.

Pulse does not require Rust, Node.js, or an agent API key to install or use.

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
             activity capture, reminders, summaries, and JSON-RPC
                                        |
                                        v
                    SQLite at %LOCALAPPDATA%\Pulse\pulse.db
```

The desktop app bundles the local service and CLI pieces it needs. If the
service is unavailable, the app can still read and update the local database
directly.

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
