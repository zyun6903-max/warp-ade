use reqwest::Client;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

pub const WEB_SEARCH_KEY_ACCOUNT: &str = "web-search";

#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    pub enabled: bool,
    pub provider: String,
    pub max_results: usize,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "brave".to_string(),
            max_results: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub fn search_blocking(
    http: &Client,
    config: &WebSearchConfig,
    api_key: &str,
    query: &str,
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::from("Web 搜索未启用"));
    }
    if api_key.trim().is_empty() {
        return Err(AppError::from("未配置 Web 搜索 API Key"));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AppError::from(format!("无法启动搜索运行时: {e}")))?;
    rt.block_on(search_async(http, config, api_key, query))
}

pub async fn search_async(
    http: &Client,
    config: &WebSearchConfig,
    api_key: &str,
    query: &str,
) -> AppResult<String> {
    let hits = match config.provider.as_str() {
        "tavily" => search_tavily(http, api_key, query, config.max_results).await?,
        _ => search_brave(http, api_key, query, config.max_results).await?,
    };
    Ok(format_hits(&hits))
}

async fn search_brave(
    http: &Client,
    api_key: &str,
    query: &str,
    max_results: usize,
) -> AppResult<Vec<SearchHit>> {
    let count = max_results.min(20).max(1);
    let response = http
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .query(&[("q", query), ("count", &count.to_string())])
        .send()
        .await
        .map_err(|e| AppError::from(format!("Brave 搜索请求失败: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| AppError::from(format!("读取 Brave 响应失败: {e}")))?;

    if !status.is_success() {
        return Err(AppError::from(format!(
            "Brave 搜索错误 ({status}): {}",
            truncate(&body, 300)
        )));
    }

    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| AppError::from(format!("解析 Brave 响应失败: {e}")))?;

    let items = parsed
        .pointer("/web/results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(items
        .into_iter()
        .filter_map(|item| {
            Some(SearchHit {
                title: item.get("title")?.as_str()?.to_string(),
                url: item.get("url")?.as_str()?.to_string(),
                snippet: item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .take(count)
        .collect())
}

async fn search_tavily(
    http: &Client,
    api_key: &str,
    query: &str,
    max_results: usize,
) -> AppResult<Vec<SearchHit>> {
    let max_results = max_results.min(20).max(1);
    let response = http
        .post("https://api.tavily.com/search")
        .json(&json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "include_answer": false,
        }))
        .send()
        .await
        .map_err(|e| AppError::from(format!("Tavily 搜索请求失败: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| AppError::from(format!("读取 Tavily 响应失败: {e}")))?;

    if !status.is_success() {
        return Err(AppError::from(format!(
            "Tavily 搜索错误 ({status}): {}",
            truncate(&body, 300)
        )));
    }

    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| AppError::from(format!("解析 Tavily 响应失败: {e}")))?;

    let items = parsed
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(items
        .into_iter()
        .filter_map(|item| {
            Some(SearchHit {
                title: item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("无标题")
                    .to_string(),
                url: item.get("url")?.as_str()?.to_string(),
                snippet: item
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect())
}

fn format_hits(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "未找到相关结果".into();
    }
    hits.iter()
        .enumerate()
        .map(|(i, h)| {
            format!(
                "{}. {}\n   URL: {}\n   {}",
                i + 1,
                h.title,
                h.url,
                h.snippet.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
    fn formats_search_hits() {
        let out = format_hits(&[SearchHit {
            title: "Rust".into(),
            url: "https://rust-lang.org".into(),
            snippet: "A language".into(),
        }]);
        assert!(out.contains("Rust"));
        assert!(out.contains("rust-lang.org"));
    }
}
