//! Pick heuristic vs CLI backend from config + privacy ack.

use pulse_core::config::{LlmConfig, PrivacyConfig};

use crate::cli_backend::CliLlmClient;
use crate::discover::{discover_cli_backend, find_backend, CliBackendKind};
use crate::heuristic::HeuristicClient;
use crate::types::LlmClient;

#[derive(Debug, Clone)]
pub struct LlmStatus {
    pub backend_id: String,
    pub path: Option<String>,
    pub privacy_ack: bool,
    pub provider_setting: String,
    pub reason: String,
}

/// Resolve client: no ack / none / missing bin → heuristic.
pub fn resolve_llm_client(llm: &LlmConfig, privacy: &PrivacyConfig) -> Box<dyn LlmClient> {
    if llm.provider == "none" {
        return Box::new(HeuristicClient::default());
    }
    if !privacy.acknowledge_remote_llm {
        return Box::new(HeuristicClient::default());
    }
    if let Some(d) = discover_cli_backend(llm) {
        return Box::new(CliLlmClient::new(
            d.kind,
            d.path,
            llm.timeout_secs.max(10),
            llm.model.clone(),
        ));
    }
    Box::new(HeuristicClient::default())
}

pub fn llm_status(llm: &LlmConfig, privacy: &PrivacyConfig) -> LlmStatus {
    let privacy_ack = privacy.acknowledge_remote_llm;
    if llm.provider == "none" {
        return LlmStatus {
            backend_id: "heuristic".into(),
            path: None,
            privacy_ack,
            provider_setting: llm.provider.clone(),
            reason: "provider=none".into(),
        };
    }
    if !privacy_ack {
        return LlmStatus {
            backend_id: "heuristic".into(),
            path: None,
            privacy_ack,
            provider_setting: llm.provider.clone(),
            reason: "privacy.acknowledge_remote_llm=false; run `pulse privacy acknowledge`".into(),
        };
    }
    if let Some(d) = discover_cli_backend(llm) {
        return LlmStatus {
            backend_id: d.kind.as_str().into(),
            path: Some(d.path.display().to_string()),
            privacy_ack,
            provider_setting: llm.provider.clone(),
            reason: "cli backend resolved".into(),
        };
    }
    LlmStatus {
        backend_id: "heuristic".into(),
        path: None,
        privacy_ack,
        provider_setting: llm.provider.clone(),
        reason: "no agent CLI found on PATH for preference order".into(),
    }
}

pub fn probe_preference(llm: &LlmConfig) -> Vec<(String, Option<String>)> {
    let kinds = if llm.provider == "auto" {
        llm.preference.clone()
    } else {
        vec![llm.provider.clone()]
    };
    kinds
        .into_iter()
        .map(|name| {
            let path = CliBackendKind::parse(&name)
                .and_then(|k| find_backend(k, llm))
                .map(|p| p.display().to_string());
            (name, path)
        })
        .collect()
}

