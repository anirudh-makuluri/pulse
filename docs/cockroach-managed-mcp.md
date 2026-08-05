# CockroachDB Cloud Managed MCP

Pulse uses CockroachDB Cloud Managed MCP as its second CockroachDB tool. It is
separate from the Pulse sync API: the app writes durable activity memory through
its authenticated AWS Lambda, while an approved coding agent can inspect that
memory through the managed MCP server.

## Safe configuration

1. In CockroachDB Cloud, open the Pulse cluster, choose **Connect**, then the
   **Model Context Protocol (MCP)** tab.
2. Copy the current configuration snippet for your MCP client. For VS Code with
   GitHub Copilot, start with
   [`.vscode/mcp.json.example`](../.vscode/mcp.json.example), replace the
   cluster-id placeholder, and save it as `.vscode/mcp.json` (which is ignored
   by Git).
3. Authenticate with OAuth and grant **read-only** (`mcp:read`) consent only.
   Do not grant write consent and do not put a Cockroach SQL connection string
   or the Lambda sync token in an MCP configuration.
4. Restart the MCP client and confirm that the `cockroachdb-cloud` server is
   connected.

CockroachDB Cloud provides the current client-specific snippet and OAuth flow;
use that Console-generated form if it differs from this example. For unattended
automation, use a dedicated service account/API key scoped to the minimum Cloud
role needed, never a desktop or Lambda credential.

## Demo verification

Ask the connected agent to use the Managed MCP tools to:

1. List the `pulse_*` tables in `defaultdb`.
2. Inspect the schema for `pulse_embeddings`.
3. Run this read-only query:

```sql
SELECT id, title, updated_at
FROM pulse_activities
ORDER BY updated_at DESC
LIMIT 5;
```

4. Show the nearest-neighbor evidence already retrieved by Pulse's Copilot or
   Inbox semantic search; then use MCP to inspect the corresponding activity.

The expected MCP surface is read-only: schema inspection and `SELECT`/`EXPLAIN`
queries. Pulse's application writes remain in the Lambda sync path, so the
Managed MCP credential cannot modify activity memory.

## Submission evidence

Record the MCP server's connected status, the `pulse_embeddings` schema, and a
read-only activity query in the demo video. Identify it as **CockroachDB Cloud
Managed MCP Server** in the submission alongside **Distributed Vector Indexing**.
