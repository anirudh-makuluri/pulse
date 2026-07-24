# Pulse Activity and Memory Model

Pulse is the activity and memory layer for human-AI work. A task is the stable
object; applications, agent sessions, windows, and processes are temporary
contributors to that task.

## Core entities

| Entity | Purpose | Example provenance |
|---|---|---|
| Activity | A durable unit of work. The existing `Task` model is the initial activity record. | User omnibox command |
| Session | A bounded period of work by an agent or application. | Claude session JSONL |
| Event | A meaningful observed action or state change. | File changed, command run, test failed |
| Checkpoint | A concise, explicit account of progress, decisions, failures, and next actions. | Claude checkpoint tool |
| Memory | A durable fact retrieved later to help continue work. | Derived from checkpoint event IDs |
| Reminder | A scheduled, contextual request to resurface work. | User command + active context |
| Artifact | A file, handoff package, patch, log, or screenshot related to an activity. | Local path or approved S3 object |
| Handoff | A structured package that lets an activity move from one agent to another. | Claude-to-Codex transfer |

## Relationships

```text
Activity
  |- has Sessions
  |- contains Events and Checkpoints
  |- produces Artifacts and Memories
  |- has Reminders
  `- produces Handoffs

Session
  `- belongs to one Activity and identifies its Agent, Application, and Repository
```

Every durable record must retain explicit provenance: user input, a source
session, a local event, an artifact, or a checkpoint. Pulse stores explicit
summaries and evidence, never hidden model reasoning.

## Local-first and cloud-backed behavior

Local SQLite is the immediate operational store. The desktop pet, omnibox,
reminder scheduler, timeline, and basic task operations must remain usable when
offline.

Cloud sync is an explicit opt-in. Once enabled, the local daemon queues approved
structured records for a Pulse sync API. That service persists durable activity
memory to CockroachDB and may archive approved large artifacts to Amazon S3.
The sync path must be retryable and non-blocking: a failed upload must not block
local actions or reminders.

## Privacy boundary

- Sources remain user-controlled and disabled by default.
- The local daemon captures only the minimum context necessary for a requested
  action.
- Raw transcripts, selected text, screenshots, and patches require an explicit
  capture or approval action before they can be synced or archived.
- Cloud settings contain endpoint and storage identifiers only. Authentication
  secrets stay outside `config.toml`, such as in the OS credential store or an
  environment variable consumed by the future sync client.
- Claude, Codex, or another configured local agent integration may interpret
  intent or generate summaries. AWS Bedrock is not a required inference path.

## Initial implementation boundary

The existing task model remains the activity root during the first migration.
Workstream 2 adds sessions, events, checkpoints, reminders, memories, and
artifacts as additive SQLite tables and models. The cloud mirror and vector
search follow only after the local activity timeline is usable.
