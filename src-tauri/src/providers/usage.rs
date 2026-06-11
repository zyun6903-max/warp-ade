use crate::agent::context::estimate_tokens;
use crate::error::AppResult;
use crate::storage::db::{Database, ProviderUsageRow};

pub fn record_chat_usage(
    db: &Database,
    provider_id: &str,
    model: &str,
    input_text: &str,
    output_text: &str,
) -> AppResult<()> {
    let input = estimate_tokens(input_text) as i64;
    let output = estimate_tokens(output_text) as i64;
    db.record_provider_usage(provider_id, model, input, output, false)
}

pub fn record_test_usage(db: &Database, provider_id: &str, model: &str) -> AppResult<()> {
    db.record_provider_usage(provider_id, model, 1, 1, true)
}

pub fn list_usage_stats(db: &Database) -> AppResult<Vec<ProviderUsageRow>> {
    db.list_provider_usage_stats()
}
