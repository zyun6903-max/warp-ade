use std::time::Instant;

use reqwest::Client;
use serde::Serialize;

use super::chat::{complete_anthropic, complete_openai};
use crate::storage::db::Provider;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProviderResult {
    pub ok: bool,
    pub model: String,
    pub latency_ms: u64,
    pub message: String,
}

pub async fn test_provider_connection(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    model: &str,
) -> TestProviderResult {
    let started = Instant::now();
    let mut probe = provider.clone();
    probe.default_model = model.to_string();
    let result = match provider.api_format.as_str() {
        "anthropic_messages" => complete_anthropic(http, &probe, api_key, &[], "ping").await,
        _ => complete_openai(http, &probe, api_key, &[], "ping").await,
    };

    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(_) => TestProviderResult {
            ok: true,
            model: model.to_string(),
            latency_ms,
            message: format!("{model} 连接成功（{latency_ms} ms）"),
        },
        Err(err) => TestProviderResult {
            ok: false,
            model: model.to_string(),
            latency_ms,
            message: format!("{model}：{}", err.message()),
        },
    }
}
