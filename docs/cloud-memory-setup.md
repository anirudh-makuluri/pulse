# CockroachDB memory setup

Workstream 4 keeps Pulse local by default. Enabling cloud sync sends only the
structured records in the SQLite `sync_outbox`; it does not upload raw session
transcripts, selected text, screenshots, patches, or local artifact files.

## Provision CockroachDB

1. Use the free-tier cluster's existing `defaultdb` database. Do not create a
   separate database; free-tier clusters do not permit it.
2. Connect to `defaultdb` with an administrator and apply
   `infra/cockroach/001_pulse_memory.sql`.
3. Enable the vector-index feature and create the commented cosine index after
   confirming that the configured embedding dimension matches the schema.
4. Create a write identity for the sync API and a separate read-only identity
   for the Managed MCP Server. Do not use either database credential in the
   desktop configuration.

CockroachDB's `VECTOR` values and cosine operator support semantic retrieval;
the schema uses a cosine vector index for the `pulse_embeddings` table. See the
[CockroachDB vector documentation](https://www.cockroachlabs.com/docs/stable/vector)
and [vector-index setup](https://www.cockroachlabs.com/docs/stable/vector-indexes).

## Configure the desktop app

Set the bearer token in the user environment, then restart the Pulse service:

```powershell
[Environment]::SetEnvironmentVariable("PULSE_SYNC_TOKEN", "replace-with-sync-api-token", "User")
```

Update `%LOCALAPPDATA%\Pulse\config.toml` only after the sync API is deployed:

```toml
[sync]
enabled = true
endpoint = "https://sync.example.com/v1/pulse/sync"
token_env = "PULSE_SYNC_TOKEN"
batch_size = 50

[embeddings]
provider = "huggingface_onnx"
model = "sentence-transformers/all-MiniLM-L6-v2"
dimensions = 384
```

`endpoint` is the complete HTTPS URL. Pulse batches accepted outbox records as
`{ "records": [...] }` and marks the entire batch delivered only after a 2xx
response. Other responses remain queued with exponential retry (up to five
minutes). This preserves local task and reminder behavior while offline.

## Local embeddings

Pulse uses the Hugging Face `sentence-transformers/all-MiniLM-L6-v2` model with
local ONNX inference. It downloads the model once, to
`%LOCALAPPDATA%\\Pulse\\models` by default, then runs offline without a Hugging
Face API token. MiniLM produces 384-dimensional vectors, so the CockroachDB
schema uses `VECTOR(384)`. Configure `embeddings.cache_dir` if the model cache
should live elsewhere.

The existing cloud table must use the same dimension. On the current empty
development database, run this in CockroachDB's SQL shell before inserting any
embeddings:

```sql
ALTER TABLE pulse_embeddings
  ALTER COLUMN embedding SET DATA TYPE VECTOR(384);
```

## Still required

The Workstream 5 Lambda API will validate the sync token, apply these records
idempotently to CockroachDB, and return semantic search results. After that
endpoint is live, configure CockroachDB Managed MCP Server with the separate
read-only identity.
