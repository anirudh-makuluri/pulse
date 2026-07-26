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
use serde::Serialize;

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

    let records = store
        .lock()
        .map_err(|_| "store lock unavailable".to_string())?
        .list_pending_sync(cfg.sync.batch_size, Utc::now())
        .map_err(|e| e.to_string())?;
    if records.is_empty() {
        return Ok(());
    }

    let body = serde_json::to_string(&SyncRequest { records: &records })
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
            let guard = store
                .lock()
                .map_err(|_| "store lock unavailable".to_string())?;
            for record in records {
                guard
                    .mark_sync_failed(
                        record.id,
                        &message,
                        Utc::now() + retry_delay(record.attempt_count),
                    )
                    .map_err(|e| e.to_string())?;
            }
            Err(format!("delivery failed; queued for retry: {message}"))
        }
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
}
