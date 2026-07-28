# Pulse: Product Context and Hackathon Build Brief

## 1. Purpose of This Document

This document captures the current product vision, interaction model, architecture, hackathon constraints, MVP scope, and implementation priorities for **Pulse**.

It is intended to be given directly to a coding agent working on the Pulse repository. The agent should use this as the primary product brief while inspecting and extending the existing codebase.

Do not treat Pulse as a conventional to-do application, a generic AI assistant, or a dashboard that merely displays agent processes. The central idea is broader:

> **Pulse is the activity and memory layer for human-AI work.**

Pulse observes ongoing work, organizes it into persistent activities and tasks, lets work move between AI agents without losing context, and resurfaces relevant work at the right time.

---

## 2. Product Summary

### One-sentence definition

> **Pulse remembers what the user and their AI agents are doing, lets work continue across applications and agents, and brings it back when it becomes relevant.**

### Short positioning

> **Pulse is an OS-level activity layer with an AI agent.**

### Suggested tagline

> **Your work, always in context.**

Other acceptable positioning lines:

- Nothing you start gets lost.
- Pick up anywhere. Continue with any agent.
- Your work does not belong to one agent.
- Pulse lets AI agents remember their work.

---

## 3. The Core Product Idea

Users increasingly work across:

- Claude Desktop or Claude Code
- Codex CLI
- Cursor
- Terminals
- IDEs
- Browsers
- Email
- Chat applications
- Documents
- Local files
- Other AI agents

The work itself is continuous, but the tools are fragmented.

A user may:

1. Start a task in Claude.
2. Stop before finishing.
3. Continue later in Codex.
4. Need to remember a decision from the original session.
5. Ask to be reminded about the task tomorrow.
6. Resume from a different application or machine.

Today, the user must manually reconstruct the context each time.

Pulse should make the **task or activity** the stable object. Individual applications and agent sessions are temporary workers attached to that activity.

```text
Task: Refactor authentication
    |
    +-- Claude session
    |     - explored existing implementation
    |     - chose refresh-token rotation
    |     - modified token logic
    |     - encountered failing tests
    |
    +-- Codex session
          - received a structured handoff
          - updated fixtures
          - reran tests
          - completed the implementation
```

The user should think:

> “Continue this task in Codex.”

They should not need to think:

> “Find the previous Claude transcript, summarize it, copy the relevant files, reconstruct the commands, and paste all of that into Codex.”

Pulse owns that continuity.

---

## 4. Product Pillars

Pulse has three primary responsibilities.

### 4.1 Observe

Understand what the user and their agents are currently doing.

Pulse should capture meaningful activity such as:

- A task being started
- An agent session beginning or ending
- A plan being created
- A file being changed
- A command being run
- A test failing
- A decision being made
- A checkpoint being saved
- A task becoming blocked
- A task being completed

Pulse should not indiscriminately record everything. It should prioritize useful, structured activity that helps the user resume, search, understand, or transfer work.

### 4.2 Continue

Allow work to move between agents and applications without losing context.

Example:

> “Continue the authentication task in Codex.”

Pulse should identify the correct task, recover the relevant state, construct a concise handoff package, launch Codex in the correct repository, and attach the new Codex session to the existing task.

### 4.3 Resurface

Bring activity back when it becomes relevant.

Example:

> “Remind me to reply to this guy tomorrow morning.”

Pulse should capture what “this guy” and “this” refer to from the active context. At the correct time, the pet should remind the user with enough context to act immediately.

---

## 5. Primary User Interface: The Pulse Pet

Pulse should have a persistent pet icon near the bottom-right of the desktop.

The pet is the primary interaction surface, not merely a decorative shortcut.

Clicking the pet opens a small text input or omnibox.

The user should be able to use the same text box for:

- Creating tasks
- Removing tasks
- Updating tasks
- Completing tasks
- Searching recent work
- Asking what an agent was doing
- Continuing a task
- Moving a task between agents
- Starting an agent
- Pausing an activity
- Creating reminders
- Snoozing reminders
- Opening relevant context
- Asking Pulse questions about recent activity

The experience should feel closer to an intelligent command surface than a form-based task manager.

### Example commands

```text
Continue the auth task in Codex
```

```text
Add a task to fix the onboarding loading state
```

```text
Mark the deployment dashboard task as done
```

```text
Remove the old landing page task
```

```text
What was Claude working on yesterday?
```

```text
Show me anything currently blocked
```

```text
Move the documentation task from Codex to Claude
```

```text
Remind me to reply to this guy tomorrow morning
```

```text
Bring this back up after lunch
```

```text
Remind me to continue this in Codex tonight
```

---

## 6. Pet States

The pet should communicate system state subtly.

Recommended states:

- **Idle**: Pulse is available and nothing needs attention.
- **Working**: One or more tracked agent sessions are active.
- **Blocked**: A task or agent needs input, approval, or clarification.
- **Completed**: A task recently finished.
- **Handoff**: Work is being transferred between agents.
- **Reminder due**: A reminder is ready.
- **Offline**: The local service or cloud memory layer is unavailable.

The pet should not constantly interrupt the user. It should use subtle animation or state changes and only expand when clicked or when a high-value reminder requires attention.

---

## 7. The Omnibox

The pet opens a single natural-language input box.

The omnibox should classify the user request into an intent.

Possible intents:

```text
CREATE_TASK
UPDATE_TASK
DELETE_TASK
COMPLETE_TASK
SEARCH_ACTIVITY
QUERY_STATUS
RESUME_TASK
TRANSFER_TASK
START_AGENT
STOP_AGENT
CREATE_REMINDER
UPDATE_REMINDER
DELETE_REMINDER
SNOOZE_REMINDER
OPEN_CONTEXT
ANSWER_CONTEXTUAL_QUESTION
```

The intent parser should return structured data, not directly execute arbitrary model output.

Example:

User input:

```text
Remind me to reply to this guy tomorrow morning
```

Structured interpretation:

```json
{
  "intent": "CREATE_REMINDER",
  "action": "reply",
  "target_reference": "current_person_or_thread",
  "scheduled_time": "resolved timestamp",
  "context_source": "active_context"
}
```

The model may help interpret language, but deterministic application logic must validate and execute the result.

---

## 8. The Activity Model

Pulse is not a traditional task manager.

A normal task manager often stores:

```text
title
description
status
due_date
```

Pulse must store the living state of work:

```text
goal
current_progress
agent_sessions
applications
repository
files_examined
files_changed
commands_executed
decisions_made
failed_attempts
errors_encountered
artifacts_created
pending_questions
current_blockers
next_recommended_action
checkpoints
reminders
```

### Stable and temporary objects

The **task/activity** is stable.

Agent sessions, windows, applications, and individual execution processes are temporary.

A single task may span:

- Multiple agents
- Multiple sessions
- Multiple days
- Multiple machines
- Multiple applications
- Multiple reminders
- Multiple checkpoints

---

## 9. Activity Graph

Represent Pulse data as connected entities rather than a flat task list.

Core entities:

```text
Task
Session
Agent
Application
Repository
Artifact
Decision
Event
Memory
Checkpoint
Reminder
Person
Conversation
```

Example relationship graph:

```text
Task
  ├── has Session
  ├── belongs to Repository
  ├── produced Artifact
  ├── contains Decision
  ├── contains Event
  ├── has Checkpoint
  ├── has Reminder
  ├── references Person
  └── references Conversation
```

Example:

```text
Task
  "Refactor authentication"

Session
  Claude Desktop, 10:42 AM–11:17 AM

Decisions
  - Use rotating refresh tokens
  - Keep access tokens stateless

Artifacts
  - src/auth/token.ts
  - tests/auth.test.ts

Failure
  - Integration tests use the old token shape

Checkpoint
  - Core implementation complete
  - Update test fixtures next
```

---

## 10. Core User Journey: Claude to Codex Handoff

### Scenario

The user asks Claude to work on a task.

Example:

> “Help me refactor authentication in this repository.”

Claude:

- Explores the repository
- Reads files
- Makes architectural decisions
- Changes code
- Runs tests
- Encounters a failure
- Stops before finishing

The user later clicks the Pulse pet and says:

> “Continue the authentication refactor in Codex.”

### Expected Pulse behavior

Pulse should:

1. Parse the request as a task transfer or resume intent.
2. Resolve which activity the user means.
3. Find the Claude session associated with the activity.
4. Recover the task objective.
5. Recover completed work and current progress.
6. Recover important decisions.
7. Recover files examined and changed.
8. Recover commands already run.
9. Recover failures and rejected approaches.
10. Recover the next recommended step.
11. Generate a concise structured handoff.
12. Launch Codex in the correct repository.
13. Provide Codex the handoff context.
14. Attach the new Codex session to the same Pulse task.
15. Continue tracking events under that task.

The user should not need to manually select or copy the original transcript unless task resolution is genuinely ambiguous.

---

## 11. Handoff Package

Do not pass an entire raw transcript unless explicitly requested.

Generate a compact structured handoff containing only useful context.

Recommended format:

```text
Task
Refactor authentication to support refresh-token rotation.

Current state
Core token rotation logic has been implemented. Integration tests are failing.

Important decisions
- Access tokens remain stateless.
- Refresh tokens are rotated after every use.
- Reuse detection invalidates the complete token family.

Files changed
- src/auth/token.ts
- src/auth/session.ts
- tests/auth.test.ts

Commands already run
- npm test
- npm run typecheck

Known failure
Authentication fixtures still use the old token structure.

Next action
Update the fixtures and rerun the authentication test suite.

Do not repeat
Claude already evaluated storing access-token sessions in Redis and rejected it.
```

### Handoff fields

A structured handoff object may contain:

```json
{
  "task_id": "string",
  "goal": "string",
  "current_state": "string",
  "completed_steps": [],
  "important_decisions": [],
  "files_examined": [],
  "files_changed": [],
  "commands_executed": [],
  "known_failures": [],
  "rejected_approaches": [],
  "artifacts": [],
  "open_questions": [],
  "next_actions": [],
  "source_agent": "claude",
  "target_agent": "codex",
  "repository_path": "string",
  "checkpoint_id": "string"
}
```

---

## 12. Agent Integrations

### 12.1 Claude integration

The preferred approach is to connect Claude to Pulse using an MCP server and/or a Pulse agent skill.

Potential MCP tools:

```text
pulse_create_task
pulse_attach_session
pulse_record_event
pulse_record_checkpoint
pulse_record_decision
pulse_record_artifact
pulse_search_memory
pulse_get_task_context
pulse_pause_task
pulse_complete_task
pulse_create_reminder
```

Claude should periodically record meaningful checkpoints.

Example checkpoint:

```json
{
  "task": "Refactor authentication",
  "progress": "Implemented refresh-token rotation",
  "decisions": [
    "Keep access tokens stateless"
  ],
  "files_changed": [
    "src/auth/token.ts"
  ],
  "failure": "Tests still expect the old token structure",
  "next_step": "Update test fixtures"
}
```

Pulse should not depend on hidden chain-of-thought or private model reasoning. Store explicit summaries, decisions, actions, evidence, and outcomes.

### 12.2 Codex integration

Codex can integrate through:

- A Pulse CLI
- MCP, if supported in the implementation environment
- A generated bootstrap prompt
- A process wrapper that launches Codex in the correct directory
- Environment variables containing task or session identifiers

Example launch flow:

```text
pulse handoff <task_id> --to codex
```

Pulse should:

1. Generate the handoff package.
2. Launch Codex in the task repository.
3. Provide the package as startup context.
4. Set the Pulse task/session identifiers.
5. Begin recording the Codex session.

### 12.3 Unsupported applications

For applications without deep integration, Pulse should support graceful context capture through:

- Active window title
- Process/application name
- Current URL when available
- File path or document title
- Manual context paste
- “Capture current activity” action

Do not attempt universal screen scraping in the MVP.

---

## 13. Context Envelope

When the omnibox opens, Pulse should create a temporary context envelope.

Possible fields:

```text
active_app
active_process
window_title
current_url
current_document
active_task
active_agent_session
recent_activity
recognized_people
recognized_conversation
repository
working_directory
timestamp
```

The context envelope helps Pulse resolve words such as:

- this
- that
- this guy
- her
- the current task
- what Claude was doing
- this error
- this project

### Privacy principle

Capture the minimum context required.

Do not silently store all visible content.

For sensitive or ambiguous context, show a preview before saving:

```text
Reminder context:
Rahul — Java Developer with Golang
Current conversation, last relevant messages

[Save reminder] [Edit context]
```

---

## 14. Contextual Reminders

Reminders are a core Pulse feature, not an unrelated add-on.

A traditional reminder stores:

```text
message + time
```

A Pulse reminder stores:

```text
intent
scheduled_time
activity_context
source_application
related_people
related_task
related_conversation
artifacts
deep_link_or_reopen_action
created_snapshot
latest_state
```

### Example

The user is viewing a conversation and says:

> “Remind me to reply to this guy tomorrow morning.”

Pulse should capture:

- The current application
- The current conversation or thread
- The person, when identifiable
- The topic
- Relevant recent context
- A deep link or reopening instruction
- The requested reminder time
- Any related Pulse task

A weak reminder would be:

```text
Reply to this guy
```

A good Pulse reminder would be:

```text
Reply to Rahul about the Java Developer with Golang role.
You were discussing the Phoenix hybrid contract, and he had not replied after seeing your message.
```

Possible reminder actions:

```text
Open conversation
Draft reply
Snooze
Mark done
```

### Coding reminder example

```text
Continue the authentication refactor.

Claude completed the token rotation logic.
The test fixtures still need updating.

[Continue in Codex] [Open task] [Later]
```

---

## 15. Reminder Lifecycle

Suggested reminder states:

```text
SCHEDULED
DUE
SNOOZED
OPENED
COMPLETED
CANCELLED
STALE
```

When a reminder fires, Pulse should retrieve the latest task state.

Example:

If the task was already completed by Codex, Pulse should not blindly show the old reminder. It can say:

> “This task appears to have been completed by Codex. Mark the reminder done?”

### Local scheduling

The Rust background service should own local scheduling.

```text
Pet / Omnibox
      |
      v
Intent and time extraction
      |
      v
Context capture
      |
      v
Local reminder scheduler
      |
      +---- CockroachDB durable record
      |
      +---- OS notification and pet state
```

This ensures reminders still work when cloud services are temporarily unavailable.

The model may interpret natural-language time expressions, but a deterministic date/time library must validate and schedule the reminder.

---

## 16. Suggested System Architecture

Pulse currently targets a lightweight desktop architecture using Tauri and Rust.

Recommended components:

```text
Pulse Pet / Omnibox
        |
        v
Tauri Desktop UI
        |
        v
Rust Background Service
  - local process monitoring
  - active-window context
  - local task state cache
  - reminder scheduling
  - agent process launch
  - local IPC
        |
        +----------------------+
        |                      |
        v                      v
Pulse Cloud/API          Local Agent Integrations
        |                Claude / Codex / configured provider
        v
CockroachDB
  - tasks
  - sessions
  - events
  - checkpoints
  - decisions
  - reminders
  - memories
  - embeddings
        |
        +----------------------+
        |                      |
        v                      v
Local AI provider        Optional AWS services
Claude / Codex /         - S3 for large artifacts
configured local agent   - Lambda or hosted worker for
  - intent parsing         non-critical asynchronous jobs
  - summarization
  - handoff generation
  - memory extraction
  - embeddings, if supported
```

Optional deployment:

- ECS Fargate for the hosted Pulse API or worker.
- Lambda for background processing.
- S3 for large artifacts.

Do not make AWS Bedrock a dependency of the MVP. Use an available local agent
integration (Claude, Codex, or another configured provider) for language and
summary work. AWS services may be added later only where they support a clear
deployment or hackathon-compliance need.

Do not add infrastructure solely to look impressive. Every service must support the core demo.

---

## 17. CockroachDB Hackathon Requirements

The hackathon requires:

- CockroachDB as the persistent memory layer
- Deployment on or use of AWS
- At least two approved CockroachDB tools
- At least one AWS service
- Public open-source repository
- Functional demo URL
- Public video under three minutes
- Clear documentation and setup instructions
- Open-source license

### Recommended CockroachDB tools

Use at least these two:

#### 17.1 Distributed Vector Indexing

Use vector search for:

- Task summaries
- Session summaries
- Decisions
- Errors
- Previous solutions
- Repository knowledge
- Reminder context
- Similar activities

Example queries:

- Find the authentication task Claude worked on.
- Have I solved a similar test failure before?
- Which previous task modified this file?
- Find work related to OAuth token refresh errors.
- What was I doing when I created this reminder?

#### 17.2 Managed MCP Server

Use CockroachDB’s Managed MCP Server to allow supported AI tools to inspect Pulse’s persistent activity data.

Possible queries:

- Show unresolved tasks in this repository.
- Find recent sessions involving authentication.
- Which agent last worked on this component?
- Show decisions associated with this task.
- Find reminders related to this person.

#### 17.3 Agent Skills Repository

Strong optional third tool.

Create or reuse Pulse-oriented skills:

```text
pulse-load-task-context
pulse-record-checkpoint
pulse-search-project-memory
pulse-handoff-task
pulse-close-task
pulse-create-contextual-reminder
```

#### 17.4 ccloud CLI

Optional stretch feature.

It may be used for setup, cluster administration, backup inspection, or operational diagnostics. It is not required for the core user experience.

---

## 18. AWS Usage (Deferred / Compliance)

AWS is not part of the critical path for the local MVP. If an AWS service is
needed for hackathon compliance, choose the smallest practical addition after
the core workflow is stable.

### Local AI providers

Use the locally available or configured agent integration—such as Claude,
Codex, or another provider—for:

- Omnibox intent classification
- Natural-language task parsing
- Session summarization
- Durable memory extraction
- Handoff package generation
- Contextual reminder summarization
- Embedding generation, if appropriate

Pulse should select the active or connected agent first, then fall back to
another configured provider. When no provider is available, deterministic
parsing and the latest saved checkpoint should keep basic task and reminder
flows usable.

### AWS Lambda

Use Lambda for asynchronous work such as:

- Processing saved checkpoints
- Updating reminder context
- Synchronizing optional artifact metadata
- Running non-critical maintenance jobs

### Amazon S3

Use S3 for large or unstructured artifacts:

- Raw transcripts
- Agent logs
- Patches
- Screenshots
- Generated reports
- Large diff files

### ECS Fargate

Optional for hosting the Pulse API, worker, or synchronization service.

The MVP does not require all of these. S3 is the preferred minimal AWS service
if one is required, because it can store optional artifacts without becoming a
dependency for intent parsing, summaries, or handoffs.

---

## 19. Suggested Data Model

The exact database schema should be adapted to the current repository, but the conceptual model should include the following.

### Tasks

```text
id
title
goal
status
priority
repository_id
created_at
updated_at
completed_at
current_checkpoint_id
current_agent
current_session_id
summary
embedding
```

Suggested statuses:

```text
INBOX
PLANNED
ACTIVE
PAUSED
BLOCKED
COMPLETED
CANCELLED
```

### Sessions

```text
id
task_id
agent_id
application_id
started_at
ended_at
status
working_directory
source_session_reference
summary
handoff_source_session_id
```

### Events

```text
id
task_id
session_id
event_type
timestamp
payload_json
source
artifact_id
importance
```

Suggested event types:

```text
TASK_CREATED
TASK_UPDATED
SESSION_STARTED
SESSION_ENDED
PLAN_CREATED
FILE_READ
FILE_CHANGED
COMMAND_RUN
TEST_FAILED
TEST_PASSED
DECISION_RECORDED
ERROR_RECORDED
CHECKPOINT_CREATED
HANDOFF_STARTED
HANDOFF_COMPLETED
REMINDER_CREATED
REMINDER_TRIGGERED
TASK_BLOCKED
TASK_COMPLETED
```

### Checkpoints

```text
id
task_id
session_id
progress_summary
completed_steps
decisions
files_changed
commands_run
known_failures
rejected_approaches
open_questions
next_actions
created_at
embedding
```

### Memories

```text
id
task_id
session_id
memory_type
content
source_event_ids
importance
confidence
created_at
embedding
```

Suggested memory types:

```text
EPISODIC
SEMANTIC
PROCEDURAL
DECISION
FAILURE
PREFERENCE
```

Do not store highly personal preferences without a clear user action or product requirement.

### Reminders

```text
id
task_id
title
action
scheduled_at
timezone
status
context_snapshot
latest_context
source_application
source_reference
person_reference
conversation_reference
reopen_action
created_at
triggered_at
completed_at
```

### Artifacts

```text
id
task_id
session_id
artifact_type
name
uri
local_path
content_hash
metadata_json
created_at
```

### Agents

```text
id
name
type
capabilities
launch_configuration
integration_type
```

### Repositories

```text
id
name
local_path
remote_url
default_branch
metadata_json
```

---

## 20. Local CLI

Pulse should remain usable through a CLI even if the pet is the primary UX.

Potential commands:

```bash
pulse task add "Fix onboarding loading state"
pulse task list
pulse task show <task-id>
pulse task complete <task-id>
pulse task remove <task-id>
```

```bash
pulse session attach --task <task-id> --agent claude
pulse checkpoint save --task <task-id>
pulse memory search "authentication failure"
pulse handoff <task-id> --to codex
pulse resume <task-id> --agent codex
```

```bash
pulse remind "Reply to Rahul" --at "tomorrow 9am"
pulse reminder list
pulse reminder snooze <reminder-id> --for "30 minutes"
pulse reminder complete <reminder-id>
```

The CLI should call the same internal service/API as the pet UI.

---

## 21. One-Month Solo MVP

There is approximately one month and one developer.

The MVP must remain narrow.

### Required MVP features

1. Pulse pet in the bottom-right.
2. Click pet to open one text input.
3. Natural-language task creation.
4. Natural-language task completion and deletion.
5. Search and status questions over recent activity.
6. Local Rust background service.
7. Claude integration through MCP or a Pulse skill.
8. Structured Claude checkpoints.
9. One-way Claude-to-Codex handoff.
10. Launch Codex in the correct repository.
11. Generate and pass a structured handoff package.
12. Attach Claude and Codex sessions to one task timeline.
13. Create contextual reminders from the omnibox.
14. Capture current Pulse task or agent session automatically.
15. Capture selected text and basic active-window metadata for external apps.
16. Store tasks, sessions, checkpoints, reminders, and memories in CockroachDB.
17. Use CockroachDB vector search.
18. Use CockroachDB Managed MCP Server or Agent Skills as the second required tool.
19. Use an available local agent integration (Claude, Codex, or another configured provider) for intent parsing, summarization, or handoff generation.
20. Trigger reminders locally using the Rust daemon.
21. Show reminders through both the pet and OS notifications.
22. Provide actions such as Open Context, Continue in Codex, Snooze, and Done.
23. Provide a full desktop view for inspecting task timelines and memories.

### Strongly preferred

- Seeded example tasks for a reliable demo
- Clear activity timeline
- Visible retrieved memories
- “Why this context was selected” explanations
- Graceful offline behavior for local reminders
- A resettable demo workflow

---

## 22. Non-Goals for the MVP

Do not build these during the hackathon unless all required features are complete and stable:

- Universal activity tracking across every application
- Full screen recording
- Continuous screenshot analysis
- Deep parsing of every email and chat client
- Multi-user collaboration
- Enterprise authentication
- Complex permissions
- Cross-device synchronization beyond a simple proof
- Team workspaces
- Billing
- Mobile applications
- Kubernetes
- Multi-agent autonomous swarms
- A general workflow builder
- Arbitrary shell execution by the model
- Fully autonomous code changes without user control
- Every possible coding agent integration
- Advanced calendar integrations
- Fine-tuning
- A custom embedding model
- A highly polished marketing website
- A complete replacement for existing task managers

---

## 23. Four-Week Implementation Plan

### Week 1: Local foundation and activity model

Goals:

- Inspect and stabilize the existing Tauri/Rust architecture.
- Implement or refine the background service.
- Define task, session, event, checkpoint, and reminder models.
- Build local IPC between pet UI and background service.
- Implement basic omnibox opening/closing behavior.
- Add deterministic task CRUD.
- Add local reminder scheduling.
- Add basic active-window metadata capture.
- Connect CockroachDB.
- Create the initial schema and migrations.

End-of-week result:

- User can click the pet.
- User can create or remove a task.
- User can create a reminder.
- Reminder fires locally.
- Data persists in CockroachDB.

### Week 2: Claude capture and structured memory

Goals:

- Implement Pulse MCP server or skill integration.
- Let Claude create or attach to a task.
- Let Claude record decisions, artifacts, events, and checkpoints.
- Build task timeline UI.
- Generate embeddings for task and checkpoint summaries.
- Add semantic search using CockroachDB vector indexing.
- Add memory panel showing retrieved prior activity.

End-of-week result:

- A Claude session can produce a structured Pulse task history.
- Pulse can search and retrieve relevant previous work.

### Week 3: Codex handoff

Goals:

- Generate handoff package from the latest checkpoint.
- Resolve a natural-language request to the correct task.
- Launch Codex in the repository.
- Pass the handoff package to Codex.
- Create a Codex session linked to the same task.
- Track handoff lifecycle events.
- Add confirmation UI for ambiguous or high-impact actions.

End-of-week result:

- User can say “Continue this task in Codex.”
- Codex starts with useful context from Claude.
- Both sessions appear under one task.

### Week 4: Reminder context, hardening, and submission

Goals:

- Improve contextual reminder capture.
- Add Open Context, Continue in Codex, Snooze, and Done actions.
- Make demo flows deterministic.
- Add fallback context capture using selected text and window metadata.
- Polish the pet states.
- Write README and setup instructions.
- Add architecture diagram.
- Add MIT or Apache 2.0 license.
- Deploy required cloud components.
- Record a reliable sub-three-minute demo video.
- Test from a clean setup.

Do not add major new features during the final week.

---

## 24. Hackathon Demo Script

The demo should explain the product in under three minutes.

### Part 1: Start work in Claude

- Open a repository.
- Ask Claude to implement or refactor a small feature.
- Claude records a Pulse task and checkpoint through MCP.
- Show that Pulse captured:
  - Goal
  - Progress
  - Decisions
  - Files changed
  - Failure
  - Next step

### Part 2: Stop before completion

- Close or stop Claude.
- Show the task remains active in Pulse.

### Part 3: Transfer to Codex

- Click the Pulse pet.
- Type:

```text
Continue the authentication task in Codex
```

- Pulse resolves the task.
- Show the generated handoff summary.
- Launch Codex in the correct repository.
- Codex receives:
  - Previous decisions
  - Changed files
  - Failed command
  - Next action
  - Rejected approach
- Codex completes the task.

### Part 4: Create a contextual reminder

While viewing a relevant conversation, task, or session:

- Click the pet.
- Type:

```text
Remind me to follow up on this tomorrow morning
```

- Show Pulse resolving the current context.
- Show the reminder record.
- Trigger a demo reminder immediately or through a shortened test schedule.
- Show:
  - Contextual description
  - Open Context
  - Continue in Codex or Draft Reply
  - Snooze
  - Done

### Part 5: Show memory

- Open the timeline.
- Show one task spanning Claude and Codex.
- Show CockroachDB-stored checkpoints and semantic memory.
- Briefly identify the CockroachDB tools and AWS services used.

---

## 25. Success Criteria

The MVP is successful when it reliably demonstrates:

### Continuity

A task can move from Claude to Codex without the user manually rebuilding context.

### Persistence

Task state, checkpoints, reminders, and memories survive application and process restarts.

### Relevance

Pulse retrieves the correct task or prior memory from natural-language references.

### Context

A reminder preserves what “this,” “him,” or “that task” meant at creation time.

### Actionability

A reminder gives the user an immediate next action, not merely a vague notification.

### Transparency

The UI shows what context was captured, what memory was retrieved, and what was passed to the next agent.

### Safety

Risky actions require confirmation. The model does not directly execute arbitrary commands.

### Hackathon fit

CockroachDB is visibly essential to persistent activity memory, semantic retrieval, and cross-agent continuity.

---

## 26. Product Principles

### Task-first, not agent-first

Agents are replaceable workers. The task is the persistent unit.

### Structured memory over raw transcripts

Store concise, useful facts, decisions, failures, and checkpoints. Raw transcripts may be archived, but they should not be the primary memory format.

### Explicit provenance

Every memory should have a source:

- Session
- Event
- Artifact
- User input
- Agent checkpoint

### Local-first interaction

The pet, scheduler, and basic task operations should remain useful even when cloud services are unavailable.

### Cloud-backed durability

CockroachDB provides persistent cross-session memory and a durable activity history.

### Minimal interruption

Pulse should stay quiet until the user asks for it or an important reminder is due.

### Natural language, deterministic execution

The model interprets. Application code validates and executes.

### Privacy by default

Capture only the context required. Give users visibility and control.

### Progressive integration

Deep support for Claude and Codex first. Graceful generic support for other applications later.

---

## 27. Implementation Guidance for the Coding Agent

Before changing code:

1. Inspect the existing repository structure.
2. Identify what already exists for:
   - Tauri
   - Rust background services
   - Pet UI
   - CLI
   - Local persistence
   - Agent process tracking
3. Preserve working functionality.
4. Avoid rewriting major subsystems without evidence that the current design blocks the MVP.
5. Prefer incremental vertical slices.

Recommended implementation order:

1. Pet opens omnibox.
2. Omnibox calls local service.
3. Deterministic task CRUD.
4. Local reminders.
5. CockroachDB persistence.
6. Activity/session model.
7. Claude checkpoint integration.
8. Vector search.
9. Handoff generation.
10. Codex launch.
11. Contextual reminder enrichment.
12. UI polish and demo hardening.

When making architectural choices, optimize for:

- Reliability
- Simplicity
- Demo clarity
- Recoverability
- Observable state transitions
- A one-month solo build

Avoid speculative abstractions that do not directly support the demo.

---

## 28. Suggested First Vertical Slice

The first complete vertical slice should be:

1. User clicks pet.
2. Omnibox opens.
3. User types:

```text
Add a task to update the Pulse onboarding
```

4. Intent parser returns structured `CREATE_TASK`.
5. Local service validates the command.
6. Task is written to CockroachDB.
7. Task appears in the desktop timeline.
8. Pet confirms creation.

Second slice:

1. User clicks pet.
2. User types:

```text
Remind me to work on this tomorrow at 9
```

3. Pulse resolves “this” to the active task.
4. Reminder is stored.
5. Local scheduler registers it.
6. At trigger time, pet enters reminder state.
7. Notification includes Open Task, Snooze, and Done.

Third slice:

1. Claude records a checkpoint.
2. User says:

```text
Continue this in Codex
```

3. Pulse generates a handoff.
4. Codex launches in the correct repository.

---

## 29. Future Vision

After the hackathon, Pulse may expand into:

- Cross-device activity synchronization
- Browser extension
- Email and chat integrations
- Calendar-aware reminders
- Location-aware or application-aware reminders
- Multi-agent task routing
- Team activity spaces
- Agent performance and cost tracking
- Automated task detection
- Rich activity graph visualization
- Approval workflows
- Agent scheduling
- Long-running background tasks
- Shared organizational memory
- A broader plugin or skill ecosystem

These are future directions, not MVP requirements.

---

## 30. Final Product Definition

> **Pulse is the activity and memory layer for human-AI work. It observes meaningful activity, organizes it into persistent tasks, lets work continue across agents and applications, and resurfaces the right context when the user needs it.**

The three foundational concepts are:

1. **Activity graph**: the durable model of tasks, sessions, events, decisions, artifacts, memories, and reminders.
2. **Omnibox and pet**: the universal natural-language interface to create, search, modify, continue, and resurface work.
3. **Agent handoff protocol**: the structured mechanism that moves an unfinished task from one agent to another without losing context.

Every implementation decision should reinforce those concepts.
