pub mod blobs;
pub mod db;
pub mod settings_store;

use serde::{Deserialize, Serialize};

use crate::models::HistoryItem;

pub use db::StorageEngine;

/// Full resolved payload for a history item, including any offloaded blobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedItemPayload {
    pub item: HistoryItem,
    pub full_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub file_paths: Vec<String>,
}

/// Disk and row usage metrics for the Clipo storage directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_items: usize,
    pub pinned_items: usize,
    pub unpinned_items: usize,
    pub snippet_count: usize,
    pub db_bytes: u64,
    pub blob_bytes: u64,
    pub total_bytes: u64,
}
