import { SecretsManagerClient, GetSecretValueCommand } from "@aws-sdk/client-secrets-manager";
import { S3Client, PutObjectCommand } from "@aws-sdk/client-s3";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
import { timingSafeEqual } from "node:crypto";
import pg from "pg";

const secrets = new SecretsManagerClient({});
const s3 = new S3Client({});
let settings;

const json = (statusCode, body) => ({
  statusCode,
  headers: { "content-type": "application/json", "cache-control": "no-store" },
  body: JSON.stringify(body),
});

async function loadSettings() {
  if (settings) return settings;
  const result = await secrets.send(new GetSecretValueCommand({ SecretId: process.env.PULSE_SECRET_ARN }));
  settings = JSON.parse(result.SecretString);
  if (!settings.cockroach_connection_string || !settings.sync_token) throw new Error("Pulse secret is incomplete");
  return settings;
}

function authorised(event, token) {
  const value = event.headers?.authorization ?? event.headers?.Authorization ?? "";
  const expected = Buffer.from(`Bearer ${token}`);
  const supplied = Buffer.from(value);
  return supplied.length === expected.length && timingSafeEqual(supplied, expected);
}

function parseBody(event) {
  try { return JSON.parse(event.body || "{}"); }
  catch { throw new Error("request body must be valid JSON"); }
}

function requireString(value, name, max = 10000) {
  if (typeof value !== "string" || !value.trim() || value.length > max) throw new Error(`${name} is invalid`);
  return value;
}

function vectorText(values) {
  if (!Array.isArray(values) || values.length !== 384 || values.some((v) => !Number.isFinite(v))) {
    throw new Error("embedding must contain 384 finite values");
  }
  return `[${values.join(",")}]`;
}

const upserts = {
  activity: {
    sql: `INSERT INTO pulse_activities (id,title,status,source,project,notes,suggested_next_action,created_at,updated_at,completed_at)
      VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
      ON CONFLICT (id) DO UPDATE SET title=excluded.title,status=excluded.status,source=excluded.source,project=excluded.project,notes=excluded.notes,suggested_next_action=excluded.suggested_next_action,updated_at=excluded.updated_at,completed_at=excluded.completed_at,synced_at=now()`,
    params: (p) => [p.id,p.title,p.status,p.source,p.project,p.notes,p.suggested_next_action,p.created_at,p.updated_at,p.completed_at],
  },
  session: {
    sql: `INSERT INTO pulse_sessions (id,activity_id,agent,application,repository_path,external_id,source_ref,started_at,ended_at,metadata)
      VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::JSONB)
      ON CONFLICT (id) DO UPDATE SET agent=excluded.agent,application=excluded.application,repository_path=excluded.repository_path,external_id=excluded.external_id,source_ref=excluded.source_ref,ended_at=excluded.ended_at,metadata=excluded.metadata,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.agent,p.application,p.repository_path,p.external_id,p.source_ref,p.started_at,p.ended_at,p.metadata_json || "{}"],
  },
  event: {
    sql: `INSERT INTO pulse_events (id,activity_id,session_id,kind,summary,payload,source_ref,occurred_at)
      VALUES ($1,$2,$3,$4,$5,$6::JSONB,$7,$8)
      ON CONFLICT (id) DO UPDATE SET session_id=excluded.session_id,kind=excluded.kind,summary=excluded.summary,payload=excluded.payload,source_ref=excluded.source_ref,occurred_at=excluded.occurred_at,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.session_id,p.kind,p.summary,p.payload_json || null,p.source_ref,p.occurred_at],
  },
  checkpoint: {
    sql: `INSERT INTO pulse_checkpoints (id,activity_id,session_id,summary,decisions,failures,next_actions,source_ref,created_at)
      VALUES ($1,$2,$3,$4,$5::JSONB,$6::JSONB,$7::JSONB,$8,$9)
      ON CONFLICT (id) DO UPDATE SET session_id=excluded.session_id,summary=excluded.summary,decisions=excluded.decisions,failures=excluded.failures,next_actions=excluded.next_actions,source_ref=excluded.source_ref,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.session_id,p.summary,JSON.stringify(p.decisions || []),JSON.stringify(p.failures || []),JSON.stringify(p.next_actions || []),p.source_ref,p.created_at],
  },
  reminder: {
    sql: `INSERT INTO pulse_reminders (id,activity_id,title,due_at,status,context,created_at,updated_at,completed_at)
      VALUES ($1,$2,$3,$4,$5,$6::JSONB,$7,$8,$9)
      ON CONFLICT (id) DO UPDATE SET title=excluded.title,due_at=excluded.due_at,status=excluded.status,context=excluded.context,updated_at=excluded.updated_at,completed_at=excluded.completed_at,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.title,p.due_at,p.status,p.context_json || "{}",p.created_at,p.updated_at,p.completed_at],
  },
  memory: {
    sql: `INSERT INTO pulse_memories (id,activity_id,checkpoint_id,kind,content,provenance,created_at,updated_at)
      VALUES ($1,$2,$3,$4,$5,$6::JSONB,$7,$8)
      ON CONFLICT (id) DO UPDATE SET checkpoint_id=excluded.checkpoint_id,kind=excluded.kind,content=excluded.content,provenance=excluded.provenance,updated_at=excluded.updated_at,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.checkpoint_id,p.kind,p.content,p.provenance_json || "{}",p.created_at,p.updated_at],
  },
  artifact: {
    sql: `INSERT INTO pulse_artifacts (id,activity_id,session_id,kind,name,object_key,content_type,size_bytes,checksum,metadata,created_at)
      VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::JSONB,$11)
      ON CONFLICT (id) DO UPDATE SET session_id=excluded.session_id,kind=excluded.kind,name=excluded.name,object_key=excluded.object_key,content_type=excluded.content_type,size_bytes=excluded.size_bytes,checksum=excluded.checksum,metadata=excluded.metadata,synced_at=now()`,
    params: (p) => [p.id,p.task_id,p.session_id,p.kind,p.name,p.object_key || null,p.content_type,p.size_bytes,p.checksum,p.metadata_json || "{}",p.created_at],
  },
};

async function archiveCheckpoint(payload) {
  const key = `checkpoints/${payload.task_id}/${payload.id}.json`;
  await s3.send(new PutObjectCommand({ Bucket: process.env.PULSE_ARCHIVE_BUCKET, Key: key, Body: JSON.stringify(payload), ContentType: "application/json" }));
  return key;
}

async function sync(body, connectionString) {
  if (!Array.isArray(body.records) || body.records.length === 0 || body.records.length > 50) throw new Error("records must contain 1-50 items");
  const client = new pg.Client({ connectionString, ssl: { rejectUnauthorized: true } });
  await client.connect();
  try {
    await client.query("BEGIN");
    // Parent activities first ensures safely retried batches satisfy foreign keys.
    const records = [...body.records].sort((a, b) => (a.record_type === "activity" ? -1 : 0) - (b.record_type === "activity" ? -1 : 0));
    for (const record of records) {
      requireString(record.record_type, "record_type", 32);
      requireString(record.record_id, "record_id", 36);
      if (record.operation === "delete") {
        if (record.record_type !== "activity") throw new Error("only activity deletion is supported");
        await client.query("DELETE FROM pulse_activities WHERE id=$1", [record.record_id]);
        continue;
      }
      if (record.operation !== "upsert" || typeof record.payload_json !== "string") throw new Error("record operation is invalid");
      const payload = JSON.parse(record.payload_json);
      const handler = upserts[record.record_type];
      if (!handler || payload.id !== record.record_id) throw new Error("record payload is invalid");
      await client.query(handler.sql, handler.params(payload));
      if (record.record_type === "checkpoint") await archiveCheckpoint(payload);
    }
    for (const embedding of body.embeddings || []) {
      const sourceType = requireString(embedding.source_type, "source_type", 32);
      if (!new Set(["activity", "checkpoint", "memory", "reminder"]).has(sourceType)) throw new Error("unsupported embedding source_type");
      await client.query(`INSERT INTO pulse_embeddings (id,activity_id,source_type,source_id,content,embedding,updated_at)
        VALUES (gen_random_uuid(),$1,$2,$3,$4,$5::VECTOR,now())
        ON CONFLICT (source_type,source_id) DO UPDATE SET activity_id=excluded.activity_id,content=excluded.content,embedding=excluded.embedding,updated_at=now()`,
        [requireString(embedding.activity_id, "activity_id", 36),sourceType,requireString(embedding.source_id, "source_id", 36),requireString(embedding.content, "content"),vectorText(embedding.embedding)]);
    }
    await client.query("COMMIT");
    return { accepted: body.records.length, embeddings: (body.embeddings || []).length };
  } catch (error) {
    await client.query("ROLLBACK").catch(() => {});
    throw error;
  } finally { await client.end(); }
}

async function search(body, connectionString) {
  const limit = Math.min(Math.max(Number(body.limit) || 10, 1), 50);
  const client = new pg.Client({ connectionString, ssl: { rejectUnauthorized: true } });
  await client.connect();
  try {
    const result = await client.query(`SELECT source_type,source_id,activity_id,content,embedding <=> $1::VECTOR AS cosine_distance
      FROM pulse_embeddings ORDER BY embedding <=> $1::VECTOR LIMIT $2`, [vectorText(body.embedding), limit]);
    return { results: result.rows };
  } finally { await client.end(); }
}

async function uploadUrl(body) {
  const activityId = requireString(body.activity_id, "activity_id", 36);
  const artifactId = requireString(body.artifact_id, "artifact_id", 36);
  const name = requireString(body.name, "name", 256).replace(/[^a-zA-Z0-9._-]/g, "_");
  const key = `artifacts/${activityId}/${artifactId}/${name}`;
  const url = await getSignedUrl(s3, new PutObjectCommand({ Bucket: process.env.PULSE_ARCHIVE_BUCKET, Key: key, ContentType: body.content_type || "application/octet-stream" }), { expiresIn: 900 });
  return { key, upload_url: url, expires_in_seconds: 900 };
}

export const handler = async (event) => {
  try {
    const configuration = await loadSettings();
    if (!authorised(event, configuration.sync_token)) return json(401, { error: "unauthorized" });
    const method = event.requestContext?.http?.method;
    const path = event.rawPath;
    if (method === "POST" && path === "/v1/pulse/sync") return json(200, await sync(parseBody(event), configuration.cockroach_connection_string));
    if (method === "POST" && path === "/v1/pulse/search") return json(200, await search(parseBody(event), configuration.cockroach_connection_string));
    if (method === "POST" && path === "/v1/pulse/artifacts/upload-url") return json(200, await uploadUrl(parseBody(event)));
    return json(404, { error: "not found" });
  } catch (error) {
    console.error(error);
    return json(error.message?.includes("invalid") || error.message?.includes("must") ? 400 : 500, { error: "request failed" });
  }
};
