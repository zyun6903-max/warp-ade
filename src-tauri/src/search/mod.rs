pub mod web;
pub mod embeddings;
pub mod code_index;

pub use web::{
    search_async, search_blocking, WebSearchConfig, WEB_SEARCH_KEY_ACCOUNT,
};
pub use code_index::{
    index_status, rebuild_index_async, search_blocking as semantic_search_blocking,
    CodeIndexStatus, SemanticSearchConfig,
};
