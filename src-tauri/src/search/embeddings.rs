use reqwest::Client;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::storage::db::Provider;

pub fn build_openai_embeddings_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/embeddings") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.ends_with("/v2") || base.ends_with("/v3") || base.ends_with("/v4") {
        return format!("{base}/embeddings");
    }
    format!("{base}/v1/embeddings")
}

pub async fn embed_texts(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    if provider.api_format == "anthropic_messages" {
        return Err(AppError::from(
            "Anthropic 提供商不支持 embedding，请在设置中指定 OpenAI 兼容提供商",
        ));
    }

    let url = build_openai_embeddings_url(&provider.base_url);
    let response = http
        .post(&url)
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "input": texts,
        }))
        .send()
        .await
        .map_err(|e| AppError::from(format!("Embedding 请求失败: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| AppError::from(format!("读取 Embedding 响应失败: {e}")))?;

    if !status.is_success() {
        return Err(AppError::from(format!(
            "Embedding API 错误 ({status}): {}",
            truncate(&body, 300)
        )));
    }

    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| AppError::from(format!("解析 Embedding 响应失败: {e}")))?;

    let items = parsed
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::from("Embedding 响应缺少 data"))?;

    let mut indexed: Vec<(usize, Vec<f32>)> = items
        .iter()
        .filter_map(|item| {
            let idx = item.get("index")?.as_u64()? as usize;
            let arr = item.get("embedding")?.as_array()?;
            let vec: Vec<f32> = arr.iter().filter_map(|v| v.as_f64().map(|n| n as f32)).collect();
            if vec.is_empty() {
                return None;
            }
            Some((idx, vec))
        })
        .collect();
    indexed.sort_by_key(|(i, _)| *i);

    if indexed.len() != texts.len() {
        return Err(AppError::from(format!(
            "Embedding 数量不匹配：期望 {}，收到 {}",
            texts.len(),
            indexed.len()
        )));
    }

    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

pub fn embedding_to_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn embedding_from_bytes(bytes: &[u8]) -> AppResult<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return Err(AppError::from("Embedding 数据损坏"));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_bytes_roundtrip() {
        let original = vec![0.1f32, -0.5, 1.25];
        let bytes = embedding_to_bytes(&original);
        let back = embedding_from_bytes(&bytes).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }
}
