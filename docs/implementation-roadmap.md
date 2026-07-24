# Pulse Implementation Roadmap

Pulse already has a local-first foundation: a Rust daemon, SQLite, CLI, Tauri
desktop UI, Claude/Codex session adapters, and installed-agent CLI summaries.
The work below extends that foundation incrementally into a persistent
cross-agent activity and memory layer.

## Workstream 1: Align the product foundation

1. [x] Update the technical design and README from an always-current to-do list to
   Pulse's activity-memory model.
2. [x] Define the new entities: activity/task, session, event, checkpoint, memory,
   reminder, artifact, and handoff.
3. [x] Add configuration for local AI provider selection, cloud sync endpoint, and
   opt-in sync.
4. [x] Document privacy boundaries: local cache by default; only approved,
   structured records and artifacts sync.

## Workstream 2: Local activity timeline

5. [ ] Add additive SQLite migrations for sessions, events, checkpoints, reminders,
   memories, and artifacts.
6. [ ] Extend `pulse-core` models and store operations.
7. [ ] Add IPC and CLI commands to create activities, attach sessions, record
   checkpoints, and inspect timelines.
8. [ ] Update the Tauri UI with an activity-detail view and chronological timeline.

## Workstream 3: Pet omnibox and reminders

9. [ ] Build the bottom-right pet shell and omnibox interaction.
10. [ ] Add deterministic intent handling for task CRUD, search, reminders, and
    resume or handoff requests.
11. [ ] Add active-window and selected-text context capture, with a preview before
    sensitive context is saved.
12. [ ] Implement the local Rust reminder scheduler plus OS notification actions:
    Open Context, Continue in Codex, Snooze, and Done.

## Workstream 4: CockroachDB memory

13. [ ] Provision and configure CockroachDB, then create the cloud schema for
    durable Pulse memory.
14. [ ] Implement an opt-in sync queue from local SQLite to the cloud activity
    graph.
15. [ ] Add local embedding-provider support, preferring Ollama or another
    configured local embedding tool.
16. [ ] Store vectors in CockroachDB and implement semantic retrieval for
    activities, decisions, failures, and reminders.
17. [ ] Configure CockroachDB Managed MCP Server for read-only agent access to
    Pulse memory.

## Workstream 5: AWS durability layer

18. [ ] Create a small AWS Lambda sync API that accepts queued Pulse events and
    writes them to CockroachDB.
19. [ ] Add Amazon S3 archival for raw checkpoint payloads, handoff packages, logs,
    diffs, and screenshots.
20. [ ] Make cloud sync retryable and non-blocking: local actions and reminders must
    continue working offline.
21. [ ] Add a minimal deployment and configuration path, including environment
    variable documentation.

## Workstream 6: continuity layer

22. [ ] Add structured checkpoint recording through a Pulse tool, skill, or
    CLI.
23. [ ] Implement task resolution and structured handoff-package generation using
    the configured local agent provider.

## Workstream 7: Demo and submission

24. [ ] Seed a deterministic authentication-refactor demo activity.
25. [ ] Demonstrate vector retrieval and Managed MCP Server queries.
26. [ ] Demonstrate offline queueing followed by cloud sync.
27. [ ] Update the README, architecture diagram, setup guide, license, demo script,
    and video checklist.

## Recommended implementation order

1. Workstream 2: local activity timeline.
2. Workstream 3: pet omnibox and reminders.
3. Workstream 4: CockroachDB memory.
4. Workstream 5: AWS durability layer.
5. Workstream 6: Claude-to-Codex continuity.
6. Workstream 7: demo and submission hardening.

This sequence produces a useful local product early, then makes CockroachDB and
AWS visibly essential to durable, cross-agent memory.
