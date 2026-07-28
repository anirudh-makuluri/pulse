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

5. [x] Add additive SQLite migrations for sessions, events, checkpoints, reminders,
   memories, and artifacts.
6. [x] Extend `pulse-core` models and store operations.
7. [x] Add IPC and CLI commands to create activities, attach sessions, record
   checkpoints, and inspect timelines.
8. [x] Update the Tauri UI with an activity-detail view and chronological timeline.

## Workstream 3: Pet omnibox and reminders

9. [x] Build the bottom-right pet shell and omnibox interaction.
10. [x] Add deterministic intent handling for task CRUD, search, reminders, and
    resume or handoff requests.
11. [x] Add active-window and selected-text context capture, with a preview before
    sensitive context is saved.
12. [x] Implement the local Rust reminder scheduler plus OS notification actions:
    Open Context, Continue in Codex, Snooze, and Done.

## Workstream 4: CockroachDB memory

13. [x] Provision and configure CockroachDB, then create the cloud schema for
    durable Pulse memory.
14. [x] Implement an opt-in sync queue from local SQLite to the cloud activity
    graph. The durable local outbox and retry worker are complete; its HTTPS
    sync API is the remaining Workstream 5 dependency.
15. [x] Add local embedding-provider support, preferring Ollama or another
    configured local embedding tool. Pulse uses local Hugging Face MiniLM ONNX
    inference with 384-dimensional vectors.
16. [x] Create the CockroachDB embedding schema and cosine vector index for
    384-dimensional local MiniLM vectors.

**Current checkpoint (2026-07-26):** `defaultdb` on the CockroachDB Basic
cluster contains the Pulse activity, timeline, memory, artifact, and
`VECTOR(384)` embedding tables, including a cosine vector index. Embedding
storage and semantic retrieval remain to be implemented.

## Workstream 5: AWS durability layer

18. [x] Create a small AWS Lambda sync API that accepts queued Pulse events and
    writes them to CockroachDB.
19. [x] Add Amazon S3 archival for raw checkpoint payloads, handoff packages, logs,
    diffs, and screenshots.
20. [x] Make cloud sync retryable and non-blocking: local actions and reminders must
    continue working offline.
21. [x] Add a minimal deployment and configuration path, including environment
    variable documentation.
22. [x] Implement the end-to-end vector flow: generate approved local MiniLM
    embeddings for activities, checkpoints, memories, and reminders; sync them
    through Lambda; and expose cosine semantic retrieval.

## Workstream 6: continuity layer

23. [ ] Add structured checkpoint recording through a Pulse tool, skill, or
    CLI.
24. [ ] Implement task resolution and structured handoff-package generation using
    the configured local agent provider.

## Workstream 7: Demo and submission

25. [ ] Seed a deterministic authentication-refactor demo activity.
26. [ ] Demonstrate vector retrieval through the Pulse sync API.
27. [ ] Demonstrate offline queueing followed by cloud sync.
28. [ ] Update the README, architecture diagram, setup guide, license, demo script,
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
