use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::models::{
    AppSettings, CapturedPayload, ClipoError, HistoryItem, IgnoreRule, IgnoreRuleType, ItemKind,
    Snippet, SnippetInput, SortBy,
};
use crate::storage::blobs::{
    self, DEFAULT_THUMB_MAX_HEIGHT, INLINE_TEXT_LIMIT,
};
use crate::storage::{settings_store, ResolvedItemPayload, StorageStats};

const PREVIEW_CHAR_LIMIT: usize = 300;
const SEARCH_TEXT_BYTE_LIMIT: usize = 16 * 1024;

/// Thread-safe SQLite WAL + content-addressed blob storage engine.
pub struct StorageEngine {
    conn: Mutex<Connection>,
    db_path: PathBuf,
    blobs_dir: PathBuf,
}

struct ExistingHistoryRow {
    id: i64,
    blob_path: Option<String>,
    rich_blob_path: Option<String>,
    thumb_path: Option<String>,
}

struct PreparedCapture {
    kind: ItemKind,
    content_hash: String,
    title_preview: String,
    full_text: Option<String>,
    plain_text: Option<String>,
    rtf_payload: Option<String>,
    html_payload: Option<String>,
    rich_blob_path: Option<String>,
    blob_path: Option<String>,
    thumb_path: Option<String>,
    image_width: Option<u32>,
    image_height: Option<u32>,
    byte_size: i64,
    source_app_name: Option<String>,
    source_app_id: Option<String>,
}

impl StorageEngine {
    /// Opens or initializes the SQLite WAL database and `blobs/` directory under `data_dir`.
    pub fn open(data_dir: &Path) -> Result<Self, ClipoError> {
        fs::create_dir_all(data_dir)?;
        let blobs_dir = data_dir.join("blobs");
        fs::create_dir_all(&blobs_dir)?;
        let db_path = data_dir.join("clipo.db");

        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;

        migrate(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            db_path,
            blobs_dir,
        })
    }

    /// Returns the path to the content-addressed `blobs/` directory.
    pub fn blobs_dir(&self) -> &Path {
        &self.blobs_dir
    }

    /// Inserts a captured clipboard payload or bumps an existing duplicate, then prunes unpinned excess.
    pub fn insert_or_bump_capture(
        &self,
        payload: CapturedPayload,
        max_items: usize,
        sort_by: SortBy,
    ) -> Result<HistoryItem, ClipoError> {
        let prepared = self.prepare_capture(payload)?;
        let mut cleanup_candidates: Vec<String> = Vec::new();

        let item = {
            let mut conn = self.lock_conn()?;
            let tx = conn.transaction()?;
            let ts = next_monotonic_ms(&tx)?;

            let existing: Option<ExistingHistoryRow> = tx
                .query_row(
                    "SELECT id, blob_path, rich_blob_path, thumb_path
                     FROM history_items
                     WHERE kind = ?1 AND content_hash = ?2",
                    params![prepared.kind.as_str(), prepared.content_hash],
                    |row| {
                        Ok(ExistingHistoryRow {
                            id: row.get(0)?,
                            blob_path: row.get(1)?,
                            rich_blob_path: row.get(2)?,
                            thumb_path: row.get(3)?,
                        })
                    },
                )
                .optional()?;

            let target_id = if let Some(prev) = existing {
                tx.execute(
                    "UPDATE history_items
                     SET title_preview = ?1,
                         full_text = ?2,
                         plain_text = ?3,
                         rtf_payload = ?4,
                         html_payload = ?5,
                         rich_blob_path = ?6,
                         blob_path = ?7,
                         thumb_path = ?8,
                         image_width = ?9,
                         image_height = ?10,
                         byte_size = ?11,
                         source_app_name = ?12,
                         source_app_id = ?13,
                         last_copied_at = ?14,
                         copy_count = copy_count + 1
                     WHERE id = ?15",
                    params![
                        prepared.title_preview,
                        prepared.full_text,
                        prepared.plain_text,
                        prepared.rtf_payload,
                        prepared.html_payload,
                        prepared.rich_blob_path,
                        prepared.blob_path,
                        prepared.thumb_path,
                        prepared.image_width,
                        prepared.image_height,
                        prepared.byte_size,
                        prepared.source_app_name,
                        prepared.source_app_id,
                        ts,
                        prev.id,
                    ],
                )?;

                collect_replaced_blob(prev.blob_path, &prepared.blob_path, &mut cleanup_candidates);
                collect_replaced_blob(
                    prev.rich_blob_path,
                    &prepared.rich_blob_path,
                    &mut cleanup_candidates,
                );
                collect_replaced_blob(
                    prev.thumb_path,
                    &prepared.thumb_path,
                    &mut cleanup_candidates,
                );

                prev.id
            } else {
                tx.execute(
                    "INSERT INTO history_items (
                        kind, content_hash, title_preview, full_text, plain_text,
                        rtf_payload, html_payload, rich_blob_path, blob_path, thumb_path,
                        image_width, image_height, byte_size, source_app_name, source_app_id,
                        first_copied_at, last_copied_at, copy_count, is_pinned, pin_shortcut, pinned_at
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5,
                        ?6, ?7, ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14, ?15,
                        ?16, ?16, 1, 0, NULL, NULL
                     )",
                    params![
                        prepared.kind.as_str(),
                        prepared.content_hash,
                        prepared.title_preview,
                        prepared.full_text,
                        prepared.plain_text,
                        prepared.rtf_payload,
                        prepared.html_payload,
                        prepared.rich_blob_path,
                        prepared.blob_path,
                        prepared.thumb_path,
                        prepared.image_width,
                        prepared.image_height,
                        prepared.byte_size,
                        prepared.source_app_name,
                        prepared.source_app_id,
                        ts,
                    ],
                )?;
                tx.last_insert_rowid()
            };

            let item = fetch_history_item(&tx, target_id)?;
            prune_excess_unpinned(&tx, max_items, sort_by, &mut cleanup_candidates)?;
            tx.commit()?;

            self.cleanup_unreferenced_blobs_with_conn(&conn, cleanup_candidates)?;
            item
        };

        Ok(item)
    }

    /// Lists history items ordered with pinned entries first, then unpinned entries by `sort_by`.
    pub fn list_history(&self, sort_by: SortBy, limit: usize) -> Result<Vec<HistoryItem>, ClipoError> {
        let conn = self.lock_conn()?;
        let order_clause = match sort_by {
            SortBy::LastCopied => "is_pinned DESC, last_copied_at DESC, id DESC",
            SortBy::FirstCopied => "is_pinned DESC, first_copied_at DESC, id DESC",
            SortBy::CopyCount => "is_pinned DESC, copy_count DESC, last_copied_at DESC, id DESC",
        };
        let sql = format!(
            "SELECT id, kind, content_hash, title_preview, full_text, plain_text,
                    rtf_payload, html_payload, rich_blob_path, blob_path, thumb_path,
                    image_width, image_height, byte_size, source_app_name, source_app_id,
                    first_copied_at, last_copied_at, copy_count, is_pinned, pin_shortcut, pinned_at
             FROM history_items
             ORDER BY {order_clause}
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], row_to_history_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ClipoError::from)
    }

    /// Fetches a single history item by primary key.
    pub fn get_history_item(&self, id: i64) -> Result<HistoryItem, ClipoError> {
        let conn = self.lock_conn()?;
        fetch_history_item(&conn, id)
    }

    /// Returns the most recently copied history item regardless of pin status.
    pub fn latest_copied_item(&self) -> Result<Option<HistoryItem>, ClipoError> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT id, kind, content_hash, title_preview, full_text, plain_text,
                    rtf_payload, html_payload, rich_blob_path, blob_path, thumb_path,
                    image_width, image_height, byte_size, source_app_name, source_app_id,
                    first_copied_at, last_copied_at, copy_count, is_pinned, pin_shortcut, pinned_at
             FROM history_items
             ORDER BY last_copied_at DESC, id DESC
             LIMIT 1",
            [],
            row_to_history_item,
        )
        .optional()
        .map_err(ClipoError::from)
    }

    /// Returns the most recently copied text history item regardless of pin status.
    pub fn latest_copied_text_item(&self) -> Result<Option<HistoryItem>, ClipoError> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT id, kind, content_hash, title_preview, full_text, plain_text,
                    rtf_payload, html_payload, rich_blob_path, blob_path, thumb_path,
                    image_width, image_height, byte_size, source_app_name, source_app_id,
                    first_copied_at, last_copied_at, copy_count, is_pinned, pin_shortcut, pinned_at
             FROM history_items
             WHERE kind = 'text'
             ORDER BY last_copied_at DESC, id DESC
             LIMIT 1",
            [],
            row_to_history_item,
        )
        .optional()
        .map_err(ClipoError::from)
    }

    /// Toggles pin status on a history item, storing `shortcut` when newly pinned.
    pub fn toggle_pin(&self, id: i64, shortcut: Option<String>) -> Result<HistoryItem, ClipoError> {
        let conn = self.lock_conn()?;
        let current = fetch_history_item(&conn, id)?;
        if current.is_pinned {
            conn.execute(
                "UPDATE history_items
                 SET is_pinned = 0, pin_shortcut = NULL, pinned_at = NULL
                 WHERE id = ?1",
                params![id],
            )?;
        } else {
            let now = current_unix_ms();
            conn.execute(
                "UPDATE history_items
                 SET is_pinned = 1, pin_shortcut = ?1, pinned_at = ?2
                 WHERE id = ?3",
                params![shortcut, now, id],
            )?;
        }
        fetch_history_item(&conn, id)
    }

    /// Deletes a single history item and cleans up any unreferenced blobs.
    pub fn delete_item(&self, id: i64) -> Result<(), ClipoError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;

        let blobs: Option<(Option<String>, Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT blob_path, rich_blob_path, thumb_path FROM history_items WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let Some((blob_path, rich_blob_path, thumb_path)) = blobs else {
            return Err(ClipoError::NotFound(format!("history item {id} not found")));
        };

        tx.execute("DELETE FROM history_items WHERE id = ?1", params![id])?;
        tx.commit()?;

        let candidates: Vec<String> = [blob_path, rich_blob_path, thumb_path]
            .into_iter()
            .flatten()
            .collect();
        self.cleanup_unreferenced_blobs_with_conn(&conn, candidates)
    }

    /// Deletes all unpinned history items while keeping pinned items, snippets, rules, and settings.
    pub fn clear_unpinned_history(&self) -> Result<(), ClipoError> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;

        let mut candidates = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT blob_path, rich_blob_path, thumb_path
                 FROM history_items
                 WHERE is_pinned = 0",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                for idx in 0..3 {
                    if let Some(rel) = row.get::<_, Option<String>>(idx)? {
                        candidates.push(rel);
                    }
                }
            }
        }

        tx.execute("DELETE FROM history_items WHERE is_pinned = 0", [])?;
        tx.commit()?;

        self.cleanup_unreferenced_blobs_with_conn(&conn, candidates)
    }

    /// Reads the complete payload for `id`, resolving any offloaded text, rich, file, or image blobs.
    pub fn read_item_payload(&self, id: i64) -> Result<ResolvedItemPayload, ClipoError> {
        let item = self.get_history_item(id)?;

        let (rtf_payload, html_payload) = if let Some(ref rel) = item.rich_blob_path {
            blobs::read_rich_blob(&self.blobs_dir, rel)?
        } else {
            (item.rtf_payload.clone(), item.html_payload.clone())
        };

        let (full_text, image_bytes, file_paths) = match item.kind {
            ItemKind::Text => {
                let text = if item.full_text.is_some() {
                    item.full_text.clone()
                } else if let Some(ref rel) = item.blob_path {
                    Some(blobs::read_blob_utf8(&self.blobs_dir, rel)?)
                } else {
                    item.plain_text.clone()
                };
                (text, None, Vec::new())
            }
            ItemKind::Image => {
                let rel = item.blob_path.as_deref().ok_or_else(|| {
                    ClipoError::NotFound(format!("image item {} missing blob path", item.id))
                })?;
                let bytes = blobs::read_blob(&self.blobs_dir, rel)?;
                (None, Some(bytes), Vec::new())
            }
            ItemKind::File => {
                let paths = if let Some(ref json) = item.full_text {
                    serde_json::from_str(json)?
                } else if let Some(ref rel) = item.blob_path {
                    let json = blobs::read_blob_utf8(&self.blobs_dir, rel)?;
                    serde_json::from_str(&json)?
                } else if let Some(ref plain) = item.plain_text {
                    plain
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(str::to_string)
                        .collect()
                } else {
                    Vec::new()
                };
                (item.full_text.clone(), None, paths)
            }
        };

        Ok(ResolvedItemPayload {
            item,
            full_text,
            rtf_payload,
            html_payload,
            image_bytes,
            file_paths,
        })
    }

    /// Computes live row counts and on-disk database + blob byte usage.
    pub fn storage_stats(&self) -> Result<StorageStats, ClipoError> {
        let (total_items, pinned_items, unpinned_items, snippet_count) = {
            let conn = self.lock_conn()?;
            let total: i64 =
                conn.query_row("SELECT COUNT(*) FROM history_items", [], |r| r.get(0))?;
            let pinned: i64 = conn.query_row(
                "SELECT COUNT(*) FROM history_items WHERE is_pinned = 1",
                [],
                |r| r.get(0),
            )?;
            let snippets: i64 =
                conn.query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))?;
            let total_usize = total.max(0) as usize;
            let pinned_usize = pinned.max(0) as usize;
            (
                total_usize,
                pinned_usize,
                total_usize.saturating_sub(pinned_usize),
                snippets.max(0) as usize,
            )
        };

        let db_bytes = file_len_or_zero(&self.db_path)
            .saturating_add(file_len_or_zero(&wal_sidecar_path(&self.db_path, "-wal")))
            .saturating_add(file_len_or_zero(&wal_sidecar_path(&self.db_path, "-shm")));
        let blob_bytes = blobs::total_blobs_bytes(&self.blobs_dir)?;
        let total_bytes = db_bytes.saturating_add(blob_bytes);

        Ok(StorageStats {
            total_items,
            pinned_items,
            unpinned_items,
            snippet_count,
            db_bytes,
            blob_bytes,
            total_bytes,
        })
    }

    /// Creates a new snippet (`id = None`) or updates an existing snippet (`id = Some(id)`).
    pub fn upsert_snippet(
        &self,
        id: Option<i64>,
        input: SnippetInput,
    ) -> Result<Snippet, ClipoError> {
        if input.title.trim().is_empty() {
            return Err(ClipoError::InvalidInput(
                "snippet title cannot be empty".to_string(),
            ));
        }

        let tags_json = serde_json::to_string(&input.tags)?;
        let now = current_unix_ms();
        let conn = self.lock_conn()?;

        let snippet_id = if let Some(existing_id) = id {
            let changed = conn.execute(
                "UPDATE snippets
                 SET title = ?1,
                     content = ?2,
                     tags_json = ?3,
                     language = ?4,
                     shortcut_key = ?5,
                     is_pinned = ?6,
                     updated_at = ?7
                 WHERE id = ?8",
                params![
                    input.title,
                    input.content,
                    tags_json,
                    input.language,
                    input.shortcut_key,
                    i64::from(input.is_pinned),
                    now,
                    existing_id,
                ],
            )?;
            if changed == 0 {
                return Err(ClipoError::NotFound(format!(
                    "snippet {existing_id} not found"
                )));
            }
            existing_id
        } else {
            conn.execute(
                "INSERT INTO snippets (
                    title, content, tags_json, language, shortcut_key,
                    is_pinned, use_count, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
                params![
                    input.title,
                    input.content,
                    tags_json,
                    input.language,
                    input.shortcut_key,
                    i64::from(input.is_pinned),
                    now,
                ],
            )?;
            conn.last_insert_rowid()
        };

        fetch_snippet(&conn, snippet_id)
    }

    /// Lists all saved snippets, pinned first, then most recently updated.
    pub fn list_snippets(&self) -> Result<Vec<Snippet>, ClipoError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, content, tags_json, language, shortcut_key,
                    is_pinned, use_count, created_at, updated_at
             FROM snippets
             ORDER BY is_pinned DESC, updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], row_to_snippet)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ClipoError::from)
    }

    /// Deletes a snippet by primary key.
    pub fn delete_snippet(&self, id: i64) -> Result<(), ClipoError> {
        let conn = self.lock_conn()?;
        let changed = conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(ClipoError::NotFound(format!("snippet {id} not found")));
        }
        Ok(())
    }

    /// Increments the usage counter on a snippet and refreshes its `updated_at` timestamp.
    pub fn increment_snippet_use(&self, id: i64) -> Result<Snippet, ClipoError> {
        let conn = self.lock_conn()?;
        let now = current_unix_ms();
        let changed = conn.execute(
            "UPDATE snippets
             SET use_count = use_count + 1, updated_at = ?1
             WHERE id = ?2",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(ClipoError::NotFound(format!("snippet {id} not found")));
        }
        fetch_snippet(&conn, id)
    }

    /// Creates or updates an ignore rule.
    pub fn upsert_ignore_rule(
        &self,
        id: Option<i64>,
        rule_type: IgnoreRuleType,
        pattern: String,
        enabled: bool,
    ) -> Result<IgnoreRule, ClipoError> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Err(ClipoError::InvalidInput(
                "ignore rule pattern cannot be empty".to_string(),
            ));
        }

        let conn = self.lock_conn()?;
        let rule_id = if let Some(existing_id) = id {
            let changed = conn.execute(
                "UPDATE ignore_rules
                 SET rule_type = ?1, pattern = ?2, enabled = ?3
                 WHERE id = ?4",
                params![rule_type.as_str(), trimmed, i64::from(enabled), existing_id],
            )?;
            if changed == 0 {
                return Err(ClipoError::NotFound(format!(
                    "ignore rule {existing_id} not found"
                )));
            }
            existing_id
        } else {
            conn.execute(
                "INSERT INTO ignore_rules (rule_type, pattern, enabled)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(rule_type, pattern) DO UPDATE SET enabled = excluded.enabled",
                params![rule_type.as_str(), trimmed, i64::from(enabled)],
            )?;
            conn.query_row(
                "SELECT id FROM ignore_rules WHERE rule_type = ?1 AND pattern = ?2",
                params![rule_type.as_str(), trimmed],
                |row| row.get(0),
            )?
        };

        conn.query_row(
            "SELECT id, rule_type, pattern, enabled FROM ignore_rules WHERE id = ?1",
            params![rule_id],
            row_to_ignore_rule,
        )
        .map_err(ClipoError::from)
    }

    /// Lists all configured ignore rules ordered by type and ID.
    pub fn list_ignore_rules(&self) -> Result<Vec<IgnoreRule>, ClipoError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, rule_type, pattern, enabled
             FROM ignore_rules
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_ignore_rule)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ClipoError::from)
    }

    /// Deletes an ignore rule by primary key.
    pub fn delete_ignore_rule(&self, id: i64) -> Result<(), ClipoError> {
        let conn = self.lock_conn()?;
        let changed = conn.execute("DELETE FROM ignore_rules WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(ClipoError::NotFound(format!("ignore rule {id} not found")));
        }
        Ok(())
    }

    /// Loads persisted application settings, or defaults if none have been saved yet.
    pub fn load_settings(&self) -> Result<AppSettings, ClipoError> {
        let conn = self.lock_conn()?;
        settings_store::load_settings(&conn)
    }

    /// Persists application settings in SQLite.
    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), ClipoError> {
        let conn = self.lock_conn()?;
        settings_store::save_settings(&conn, settings)
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, ClipoError> {
        self.conn
            .lock()
            .map_err(|_| ClipoError::Internal("storage mutex poisoned".to_string()))
    }

    fn prepare_capture(&self, payload: CapturedPayload) -> Result<PreparedCapture, ClipoError> {
        match payload.kind {
            ItemKind::Text => {
                let text = payload.plain_text.unwrap_or_default();
                let content_hash = blobs::sha256_hex(text.as_bytes());
                let title_preview = make_title_preview(&text);

                let (full_text, plain_text, blob_path) = if text.len() > INLINE_TEXT_LIMIT {
                    let rel = format!("{content_hash}.txt");
                    blobs::write_blob_atomic(&self.blobs_dir, &rel, text.as_bytes())?;
                    (
                        None,
                        Some(truncate_utf8_bytes(&text, SEARCH_TEXT_BYTE_LIMIT)),
                        Some(rel),
                    )
                } else {
                    (Some(text.clone()), Some(text.clone()), None)
                };

                let rich_len = payload.rtf_payload.as_ref().map_or(0, String::len)
                    + payload.html_payload.as_ref().map_or(0, String::len);
                let (rtf_payload, html_payload, rich_blob_path, rich_byte_size) =
                    if rich_len > INLINE_TEXT_LIMIT {
                        let (rel, written_len) = blobs::write_rich_blob(
                            &self.blobs_dir,
                            payload.rtf_payload.as_deref(),
                            payload.html_payload.as_deref(),
                        )?;
                        (None, None, Some(rel), written_len)
                    } else {
                        (payload.rtf_payload, payload.html_payload, None, rich_len)
                    };

                let byte_size = (text.len() + rich_byte_size) as i64;

                Ok(PreparedCapture {
                    kind: ItemKind::Text,
                    content_hash,
                    title_preview,
                    full_text,
                    plain_text,
                    rtf_payload,
                    html_payload,
                    rich_blob_path,
                    blob_path,
                    thumb_path: None,
                    image_width: None,
                    image_height: None,
                    byte_size,
                    source_app_name: payload.source_app_name,
                    source_app_id: payload.source_app_id,
                })
            }
            ItemKind::Image => {
                let bytes = payload.image_bytes.ok_or_else(|| {
                    ClipoError::InvalidInput("image capture missing image_bytes".to_string())
                })?;
                if bytes.is_empty() {
                    return Err(ClipoError::InvalidInput(
                        "image capture has empty bytes".to_string(),
                    ));
                }

                let content_hash = blobs::sha256_hex(&bytes);
                let stored = blobs::store_image_and_thumbnail(
                    &self.blobs_dir,
                    &bytes,
                    payload.image_width,
                    payload.image_height,
                    DEFAULT_THUMB_MAX_HEIGHT,
                )?;
                let title_preview = match (stored.width, stored.height) {
                    (Some(w), Some(h)) => format!("Image ({w}x{h})"),
                    _ => "Image".to_string(),
                };

                Ok(PreparedCapture {
                    kind: ItemKind::Image,
                    content_hash,
                    title_preview,
                    full_text: None,
                    plain_text: None,
                    rtf_payload: None,
                    html_payload: None,
                    rich_blob_path: None,
                    blob_path: Some(stored.blob_rel_path),
                    thumb_path: stored.thumb_rel_path,
                    image_width: stored.width,
                    image_height: stored.height,
                    byte_size: stored.byte_size,
                    source_app_name: payload.source_app_name,
                    source_app_id: payload.source_app_id,
                })
            }
            ItemKind::File => {
                if payload.file_paths.is_empty() {
                    return Err(ClipoError::InvalidInput(
                        "file capture missing file_paths".to_string(),
                    ));
                }

                let mut canonical = payload.file_paths.clone();
                canonical.sort();
                let content_hash = blobs::sha256_hex(canonical.join("\n").as_bytes());
                let title_preview = make_file_title_preview(&payload.file_paths);
                let joined_plain = payload.file_paths.join("\n");
                let files_json = serde_json::to_string(&payload.file_paths)?;

                let (full_text, blob_path) = if files_json.len() > INLINE_TEXT_LIMIT {
                    let rel = format!("{content_hash}.files.json");
                    blobs::write_blob_atomic(&self.blobs_dir, &rel, files_json.as_bytes())?;
                    (None, Some(rel))
                } else {
                    (Some(files_json), None)
                };

                Ok(PreparedCapture {
                    kind: ItemKind::File,
                    content_hash,
                    title_preview,
                    full_text,
                    plain_text: Some(truncate_utf8_bytes(&joined_plain, SEARCH_TEXT_BYTE_LIMIT)),
                    rtf_payload: None,
                    html_payload: None,
                    rich_blob_path: None,
                    blob_path,
                    thumb_path: None,
                    image_width: None,
                    image_height: None,
                    byte_size: joined_plain.len() as i64,
                    source_app_name: payload.source_app_name,
                    source_app_id: payload.source_app_id,
                })
            }
        }
    }

    fn cleanup_unreferenced_blobs_with_conn(
        &self,
        conn: &Connection,
        candidates: Vec<String>,
    ) -> Result<(), ClipoError> {
        let unique: HashSet<String> = candidates.into_iter().collect();
        for rel_path in unique {
            let refs: i64 = conn.query_row(
                "SELECT COUNT(*) FROM history_items
                 WHERE blob_path = ?1 OR rich_blob_path = ?1 OR thumb_path = ?1",
                params![rel_path],
                |row| row.get(0),
            )?;
            if refs == 0 {
                blobs::delete_blob_if_exists(&self.blobs_dir, &rel_path)?;
            }
        }
        Ok(())
    }
}

fn migrate(conn: &Connection) -> Result<(), ClipoError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            title_preview TEXT NOT NULL,
            full_text TEXT,
            plain_text TEXT,
            rtf_payload TEXT,
            html_payload TEXT,
            rich_blob_path TEXT,
            blob_path TEXT,
            thumb_path TEXT,
            image_width INTEGER,
            image_height INTEGER,
            byte_size INTEGER NOT NULL,
            source_app_name TEXT,
            source_app_id TEXT,
            first_copied_at INTEGER NOT NULL,
            last_copied_at INTEGER NOT NULL,
            copy_count INTEGER NOT NULL DEFAULT 1,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            pin_shortcut TEXT,
            pinned_at INTEGER,
            UNIQUE(kind, content_hash)
        );
        CREATE INDEX IF NOT EXISTS idx_history_pinned_last_copied
            ON history_items (is_pinned, last_copied_at DESC);
        CREATE INDEX IF NOT EXISTS idx_history_kind_hash
            ON history_items (kind, content_hash);

        CREATE TABLE IF NOT EXISTS snippets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            language TEXT,
            shortcut_key TEXT,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            use_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ignore_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            UNIQUE(rule_type, pattern)
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn prune_excess_unpinned(
    tx: &Transaction<'_>,
    max_items: usize,
    sort_by: SortBy,
    cleanup_candidates: &mut Vec<String>,
) -> Result<(), ClipoError> {
    let unpinned_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM history_items WHERE is_pinned = 0",
        [],
        |row| row.get(0),
    )?;
    let unpinned_usize = unpinned_count.max(0) as usize;
    if unpinned_usize <= max_items {
        return Ok(());
    }

    let excess = unpinned_usize - max_items;
    let oldest_order = match sort_by {
        SortBy::LastCopied => "last_copied_at ASC, id ASC",
        SortBy::FirstCopied => "first_copied_at ASC, id ASC",
        SortBy::CopyCount => "copy_count ASC, last_copied_at ASC, id ASC",
    };
    let select_sql = format!(
        "SELECT id, blob_path, rich_blob_path, thumb_path
         FROM history_items
         WHERE is_pinned = 0
         ORDER BY {oldest_order}
         LIMIT ?1"
    );

    let mut doomed_ids = Vec::with_capacity(excess);
    {
        let mut stmt = tx.prepare(&select_sql)?;
        let mut rows = stmt.query(params![excess as i64])?;
        while let Some(row) = rows.next()? {
            doomed_ids.push(row.get::<_, i64>(0)?);
            for idx in 1..=3 {
                if let Some(rel) = row.get::<_, Option<String>>(idx)? {
                    cleanup_candidates.push(rel);
                }
            }
        }
    }

    for id in doomed_ids {
        tx.execute("DELETE FROM history_items WHERE id = ?1", params![id])?;
    }

    Ok(())
}

fn fetch_history_item(conn: &Connection, id: i64) -> Result<HistoryItem, ClipoError> {
    conn.query_row(
        "SELECT id, kind, content_hash, title_preview, full_text, plain_text,
                rtf_payload, html_payload, rich_blob_path, blob_path, thumb_path,
                image_width, image_height, byte_size, source_app_name, source_app_id,
                first_copied_at, last_copied_at, copy_count, is_pinned, pin_shortcut, pinned_at
         FROM history_items
         WHERE id = ?1",
        params![id],
        row_to_history_item,
    )
    .optional()?
    .ok_or_else(|| ClipoError::NotFound(format!("history item {id} not found")))
}

fn fetch_snippet(conn: &Connection, id: i64) -> Result<Snippet, ClipoError> {
    conn.query_row(
        "SELECT id, title, content, tags_json, language, shortcut_key,
                is_pinned, use_count, created_at, updated_at
         FROM snippets
         WHERE id = ?1",
        params![id],
        row_to_snippet,
    )
    .optional()?
    .ok_or_else(|| ClipoError::NotFound(format!("snippet {id} not found")))
}

fn row_to_history_item(row: &Row<'_>) -> rusqlite::Result<HistoryItem> {
    let kind_raw: String = row.get(1)?;
    let kind = ItemKind::from_str(&kind_raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let is_pinned_int: i64 = row.get(19)?;

    Ok(HistoryItem {
        id: row.get(0)?,
        kind,
        content_hash: row.get(2)?,
        title_preview: row.get(3)?,
        full_text: row.get(4)?,
        plain_text: row.get(5)?,
        rtf_payload: row.get(6)?,
        html_payload: row.get(7)?,
        rich_blob_path: row.get(8)?,
        blob_path: row.get(9)?,
        thumb_path: row.get(10)?,
        image_width: row.get(11)?,
        image_height: row.get(12)?,
        byte_size: row.get(13)?,
        source_app_name: row.get(14)?,
        source_app_id: row.get(15)?,
        first_copied_at: row.get(16)?,
        last_copied_at: row.get(17)?,
        copy_count: row.get(18)?,
        is_pinned: is_pinned_int != 0,
        pin_shortcut: row.get(20)?,
        pinned_at: row.get(21)?,
    })
}

fn row_to_snippet(row: &Row<'_>) -> rusqlite::Result<Snippet> {
    let tags_json: String = row.get(3)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let is_pinned_int: i64 = row.get(6)?;

    Ok(Snippet {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        tags,
        language: row.get(4)?,
        shortcut_key: row.get(5)?,
        is_pinned: is_pinned_int != 0,
        use_count: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn row_to_ignore_rule(row: &Row<'_>) -> rusqlite::Result<IgnoreRule> {
    let type_raw: String = row.get(1)?;
    let rule_type = IgnoreRuleType::from_str(&type_raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let enabled_int: i64 = row.get(3)?;

    Ok(IgnoreRule {
        id: row.get(0)?,
        rule_type,
        pattern: row.get(2)?,
        enabled: enabled_int != 0,
    })
}

fn collect_replaced_blob(
    old_path: Option<String>,
    new_path: &Option<String>,
    candidates: &mut Vec<String>,
) {
    if let Some(old) = old_path {
        if new_path.as_deref() != Some(old.as_str()) {
            candidates.push(old);
        }
    }
}

fn current_unix_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn next_monotonic_ms(tx: &Transaction<'_>) -> Result<i64, ClipoError> {
    let now = current_unix_ms();
    let max_existing: Option<i64> =
        tx.query_row("SELECT MAX(last_copied_at) FROM history_items", [], |row| {
            row.get(0)
        })?;
    Ok(match max_existing {
        Some(prev) if now <= prev => prev.saturating_add(1),
        _ => now,
    })
}

fn make_title_preview(text: &str) -> String {
    let mut normalized = String::new();
    let mut prev_space = false;
    let mut char_count = 0usize;

    for ch in text.trim().chars() {
        let mapped = if ch.is_whitespace() { ' ' } else { ch };
        if mapped == ' ' {
            if prev_space || normalized.is_empty() {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }

        if char_count >= PREVIEW_CHAR_LIMIT {
            normalized.push('…');
            break;
        }
        normalized.push(mapped);
        char_count += 1;
    }

    normalized
}

fn make_file_title_preview(paths: &[String]) -> String {
    let first = paths
        .first()
        .and_then(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| paths.first().cloned().unwrap_or_default());

    if paths.len() <= 1 {
        make_title_preview(&first)
    } else {
        format!("{} (+{} more)", make_title_preview(&first), paths.len() - 1)
    }
}

fn truncate_utf8_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn file_len_or_zero(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn wal_sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut os = db_path.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}
