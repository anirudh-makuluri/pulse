//! Best-effort opt-in cloud synchronization.
//!
//! This worker is deliberately independent of the local activity path: it
//! reads the durable SQLite outbox and retries failed deliveries in the
//! background. A network failure never reaches task, reminder, or IPC callers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use pulse_core::{Config, Store, SyncOutboxItem};
use pulse_llm::HuggingFaceEmbeddingClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct SyncWorker {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SyncWorker {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Serialize)]
struct SyncRequest<'a> {
    records: &'a [SyncOutboxItem],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeddings: Vec<SyncEmbedding>,
}

/// A locally generated, approved MiniLM vector. The API never receives model
/// credentials or asks Lambda to infer from raw transcripts.
#[derive(Debug, Serialize)]
struct SyncEmbedding {
    source_type: String,
    source_id: String,
    activity_id: String,
    content: String,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct UploadUrlResponse {
    key: String,
    upload_url: String,
}

pub fn start_sync_worker(store: Arc<Mutex<Store>>, config: Arc<Mutex<Config>>) -> SyncWorker {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        while !worker_stop.load(Ordering::SeqCst) {
            if let Err(error) = sync_once(&store, &config) {
                eprintln!("pulse sync: {error}");
            }
            for _ in 0..5 {
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
    SyncWorker {
        stop,
        handle: Some(handle),
    }
}

fn sync_once(store: &Arc<Mutex<Store>>, config: &Arc<Mutex<Config>>) -> Result<(), String> {
    let cfg = config
        .lock()
        .map_err(|_| "config lock unavailable".to_string())?
        .clone();
    if !cfg.sync.enabled {
        return Ok(());
    }
    let endpoint = cfg
        .sync
        .endpoint
        .as_deref()
        .ok_or_else(|| "sync is enabled without an endpoint".to_string())?;
    let token = std::env::var(&cfg.sync.token_env).map_err(|_| {
        format!(
            "sync is enabled but environment variable {} is not set",
            cfg.sync.token_env
        )
    })?;

    let mut records = store
        .lock()
        .map_err(|_| "store lock unavailable".to_string())?
        .list_pending_sync(cfg.sync.batch_size, Utc::now())
        .map_err(|e| e.to_string())?;
    if records.is_empty() {
        return Ok(());
    }

    if let Err(error) = upload_approved_artifacts(endpoint, &token, &mut records) {
        mark_records_failed(store, records, &error)?;
        return Err(format!(
            "artifact archival failed; queued for retry: {error}"
        ));
    }

    // Embeddings deliberately happen after the outbox has been selected and
    // before an HTTP request. Any model failure leaves the structured records
    // syncable; task and reminder operations remain entirely local and never
    // wait for this work.
    let embeddings = match build_embeddings(&cfg, &records) {
        Ok(embeddings) => embeddings,
        Err(error) => {
            eprintln!("pulse sync embeddings: {error}; sending records without vectors");
            Vec::new()
        }
    };
    let body = serde_json::to_string(&SyncRequest {
        records: &records,
        embeddings,
    })
    .map_err(|e| format!("encode sync request: {e}"))?;
    let response = ureq::post(endpoint)
        .set("content-type", "application/json")
        .set("authorization", &format!("Bearer {token}"))
        .send_string(&body);

    match response {
        Ok(_) => {
            let ids: Vec<_> = records.iter().map(|record| record.id).collect();
            store
                .lock()
                .map_err(|_| "store lock unavailable".to_string())?
                .mark_sync_delivered(&ids, Utc::now())
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            mark_records_failed(store, records, &message)?;
            Err(format!("delivery failed; queued for retry: {message}"))
        }
    }
}

fn mark_records_failed(
    store: &Arc<Mutex<Store>>,
    records: Vec<SyncOutboxItem>,
    message: &str,
) -> Result<(), String> {
    let guard = store
        .lock()
        .map_err(|_| "store lock unavailable".to_string())?;
    for record in records {
        guard
            .mark_sync_failed(
                record.id,
                message,
                Utc::now() + retry_delay(record.attempt_count),
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Archive only an explicit local artifact file. A missing local path simply
/// leaves the artifact as structured metadata; the file is never guessed or
/// scanned. The temporary object URL is one-time scoped by S3 and expires in
/// 15 minutes.
fn upload_approved_artifacts(
    sync_endpoint: &str,
    token: &str,
    records: &mut [SyncOutboxItem],
) -> Result<(), String> {
    let endpoint = artifact_upload_endpoint(sync_endpoint)?;
    for record in records
        .iter_mut()
        .filter(|record| record.record_type == "artifact" && record.operation == "upsert")
    {
        let mut payload: Value = serde_json::from_str(&record.payload_json)
            .map_err(|e| format!("decode artifact payload: {e}"))?;
        let Some(local_path) = payload.get("local_path").and_then(Value::as_str) else {
            continue;
        };
        let file = std::fs::read(local_path)
            .map_err(|e| format!("read approved artifact {local_path}: {e}"))?;
        let content_type = payload
            .get("content_type")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream");
        let request = serde_json::json!({
            "activity_id": payload.get("task_id").and_then(Value::as_str),
            "artifact_id": payload.get("id").and_then(Value::as_str),
            "name": payload.get("name").and_then(Value::as_str),
            "content_type": content_type,
        });
        let response: UploadUrlResponse = ureq::post(&endpoint)
            .set("content-type", "application/json")
            .set("authorization", &format!("Bearer {token}"))
            .send_json(request)
            .map_err(|e| format!("request artifact upload URL: {e}"))?
            .into_json()
            .map_err(|e| format!("decode artifact upload URL: {e}"))?;
        ureq::put(&response.upload_url)
            .set("content-type", content_type)
            .send_bytes(&file)
            .map_err(|e| format!("upload approved artifact: {e}"))?;
        payload["object_key"] = Value::String(response.key);
        // The cloud payload has an object key rather than a local path. The
        // local SQLite artifact continues to retain its original local path.
        payload["local_path"] = Value::Null;
        record.payload_json = serde_json::to_string(&payload)
            .map_err(|e| format!("encode archived artifact payload: {e}"))?;
    }
    Ok(())
}

fn artifact_upload_endpoint(sync_endpoint: &str) -> Result<String, String> {
    let base = sync_endpoint
        .strip_suffix("/v1/pulse/sync")
        .ok_or_else(|| {
            "sync.endpoint must end with /v1/pulse/sync for artifact archival".to_string()
        })?;
    Ok(format!("{base}/v1/pulse/artifacts/upload-url"))
}

fn build_embeddings(
    config: &Config,
    records: &[SyncOutboxItem],
) -> Result<Vec<SyncEmbedding>, String> {
    if config.embeddings.provider == "none" {
        return Ok(Vec::new());
    }

    let candidates: Vec<_> = records.iter().filter_map(embedding_candidate).collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut client =
        HuggingFaceEmbeddingClient::from_config(&config.embeddings).map_err(|e| e.to_string())?;
    let vectors = client
        .embed(
            candidates
                .iter()
                .map(|candidate| candidate.content.clone())
                .collect(),
        )
        .map_err(|e| e.to_string())?;

    Ok(candidates
        .into_iter()
        .zip(vectors)
        .map(|(candidate, embedding)| SyncEmbedding {
            embedding,
            ..candidate
        })
        .collect())
}

fn embedding_candidate(record: &SyncOutboxItem) -> Option<SyncEmbedding> {
    if record.operation != "upsert" {
        return None;
    }
    let payload: Value = serde_json::from_str(&record.payload_json).ok()?;
    let source_type = record.record_type.as_str();
    let (activity_id, content) = match source_type {
        "activity" => (
            payload.get("id")?.as_str()?.to_string(),
            join_text(&[
                payload.get("title")?.as_str()?,
                payload.get("notes").and_then(Value::as_str).unwrap_or(""),
                payload
                    .get("suggested_next_action")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ]),
        ),
        "checkpoint" => (
            payload.get("task_id")?.as_str()?.to_string(),
            join_text(&[
                payload.get("summary")?.as_str()?,
                &json_strings(&payload, "decisions"),
                &json_strings(&payload, "failures"),
                &json_strings(&payload, "next_actions"),
            ]),
        ),
        "memory" => (
            payload.get("task_id")?.as_str()?.to_string(),
            join_text(&[
                payload.get("kind")?.as_str()?,
                payload.get("content")?.as_str()?,
            ]),
        ),
        "reminder" => (
            payload.get("task_id")?.as_str()?.to_string(),
            join_text(&[
                payload.get("title")?.as_str()?,
                payload
                    .get("context_json")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ]),
        ),
        _ => return None,
    };
    if content.trim().is_empty() {
        return None;
    }
    Some(SyncEmbedding {
        source_type: source_type.to_string(),
        source_id: payload.get("id")?.as_str()?.to_string(),
        activity_id,
        content: truncate_chars(content, 10_000),
        embedding: Vec::new(),
    })
}

fn json_strings(payload: &Value, field: &str) -> String {
    payload
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default()
}

fn join_text(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_chars(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text
    } else {
        text.chars().take(max_chars).collect()
    }
}

fn retry_delay(attempt_count: i64) -> ChronoDuration {
    let exponent = attempt_count.clamp(0, 8) as u32;
    ChronoDuration::seconds((2_i64.pow(exponent + 1)).min(300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_is_bounded() {
        assert_eq!(retry_delay(0), ChronoDuration::seconds(2));
        assert_eq!(retry_delay(4), ChronoDuration::seconds(32));
        assert_eq!(retry_delay(99), ChronoDuration::seconds(300));
    }

    #[test]
    fn creates_embedding_candidate_for_checkpoint() {
        let record = SyncOutboxItem {
            id: uuid::Uuid::new_v4(), record_type: "checkpoint".into(), record_id: uuid::Uuid::new_v4(), operation: "upsert".into(),
            payload_json: r#"{"id":"bbf7d150-2e8b-4c06-86c8-759e632ddcf8","task_id":"c737ccd5-fd37-435d-8183-6ad1a6bbf78d","summary":"API is deployed","decisions":["use Lambda"],"failures":[],"next_actions":["verify sync"]}"#.into(),
            created_at: Utc::now(), attempt_count: 0, next_attempt_at: Utc::now(), last_error: None,
        };
        let candidate = embedding_candidate(&record).unwrap();
        assert_eq!(candidate.source_type, "checkpoint");
        assert_eq!(
            candidate.activity_id,
            "c737ccd5-fd37-435d-8183-6ad1a6bbf78d"
        );
        assert!(candidate.content.contains("use Lambda"));
    }

    #[test]
    fn ignores_non_searchable_and_deleted_records() {
        let mut record = SyncOutboxItem {
            id: uuid::Uuid::new_v4(),
            record_type: "event".into(),
            record_id: uuid::Uuid::new_v4(),
            operation: "upsert".into(),
            payload_json: "{}".into(),
            created_at: Utc::now(),
            attempt_count: 0,
            next_attempt_at: Utc::now(),
            last_error: None,
        };
        assert!(embedding_candidate(&record).is_none());
        record.operation = "delete".into();
        record.record_type = "activity".into();
        assert!(embedding_candidate(&record).is_none());
    }

    #[test]
    fn derives_artifact_endpoint_from_sync_endpoint() {
        assert_eq!(
            artifact_upload_endpoint("https://sync.example.com/v1/pulse/sync").unwrap(),
            "https://sync.example.com/v1/pulse/artifacts/upload-url"
        );
        assert!(artifact_upload_endpoint("https://sync.example.com/sync").is_err());
    }
}
