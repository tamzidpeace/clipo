use std::time::Duration;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

use crate::clipboard::{validate_ignore_regex, PastePayload, PlatformCapabilities};
use crate::models::{
    AppSettings, ClipoError, FilterTab, HistoryItem, IgnoreRule, IgnoreRuleType, ItemKind,
    SearchResultDto, SearchRowKind, Snippet, SnippetInput,
};
use crate::paste::{PasteActionMode, PasteEngine, PasteOutcome};
use crate::search::SearchEngine;
use crate::snippets::{
    export_snippets_json, import_snippets_json, resolve_snippet_template, TemplateContext,
};
use crate::storage::StorageStats;
use crate::{tray, windows_mgr, AppState};

const CLIPO_RELEASES_URL: &str = "https://github.com/tamzidpeace/clipo/releases";

/// Full history item payload returned to the Popup Preview Pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDetailDto {
    pub item: HistoryItem,
    pub full_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub file_paths: Vec<String>,
}

/// Live runtime capture & platform capability status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStatusDto {
    pub paused: bool,
    pub ignore_next: bool,
    pub capabilities: PlatformCapabilities,
}

/// Result of checking for Clipo application updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatusDto {
    pub status: String,
    pub current_version: String,
    pub available_version: Option<String>,
    pub release_notes: Option<String>,
    pub release_url: String,
    pub message: Option<String>,
}

#[tauri::command]
pub fn search_items(
    state: State<'_, AppState>,
    query: String,
    tab: FilterTab,
) -> Result<SearchResultDto, ClipoError> {
    let settings = state.storage.load_settings()?;
    let limit = settings.storage.max_items.saturating_add(500);
    let history = state.storage.list_history(settings.storage.sort_by, limit)?;
    let snippets = state.storage.list_snippets()?;
    Ok(SearchEngine::query(
        &history,
        &snippets,
        &query,
        settings.general.search_mode,
        tab,
        settings.appearance.pin_to,
    ))
}

#[tauri::command]
pub fn select_item(
    app: AppHandle,
    state: State<'_, AppState>,
    row_kind: SearchRowKind,
    id: i64,
    mode: Option<PasteActionMode>,
) -> Result<PasteOutcome, ClipoError> {
    let settings = state.storage.load_settings()?;
    let effective_mode = mode.unwrap_or(if !settings.general.paste_automatically {
        PasteActionMode::CopyOnly
    } else if settings.general.paste_without_formatting {
        PasteActionMode::CopyAndPastePlain
    } else {
        PasteActionMode::CopyAndPasteRich
    });

    let (payload, cursor_back_chars, is_snippet) = match row_kind {
        SearchRowKind::History => {
            let resolved = state.storage.read_item_payload(id)?;
            let plain_text = match resolved.item.kind {
                ItemKind::File => None,
                _ => resolved.full_text.or(resolved.item.plain_text),
            };
            let payload = PastePayload {
                kind: resolved.item.kind,
                plain_text,
                rtf_payload: resolved.rtf_payload,
                html_payload: resolved.html_payload,
                image_bytes: resolved.image_bytes,
                file_paths: resolved.file_paths,
            };
            (payload, None, false)
        }
        SearchRowKind::Snippet => {
            let snippet = state
                .storage
                .list_snippets()?
                .into_iter()
                .find(|s| s.id == id)
                .ok_or_else(|| ClipoError::NotFound(format!("snippet {id} not found")))?;

            let clipboard_text = latest_text_from_history(&state);
            let resolved = resolve_snippet_template(
                &snippet.content,
                &TemplateContext {
                    now: Local::now(),
                    clipboard_text,
                    uuid_override: None,
                },
            );
            let cursor_back = if settings.advanced.move_cursor_on_snippet_placeholder {
                resolved.cursor_back_chars
            } else {
                None
            };
            let payload = PastePayload {
                kind: ItemKind::Text,
                plain_text: Some(resolved.text),
                rtf_payload: None,
                html_payload: None,
                image_bytes: None,
                file_paths: Vec::new(),
            };
            (payload, cursor_back, true)
        }
    };

    let _ = windows_mgr::hide_popup(&app);

    let prev_app = state.prev_app.lock().ok().and_then(|g| g.clone());
    let outcome = PasteEngine::execute(
        state.backend.as_ref(),
        &state.write_guard,
        &payload,
        effective_mode,
        prev_app.as_ref(),
        cursor_back_chars,
    )?;

    if is_snippet {
        let _ = state.storage.increment_snippet_use(id);
    }

    Ok(outcome)
}

fn latest_text_from_history(state: &AppState) -> Option<String> {
    let item = state.storage.latest_copied_text_item().ok().flatten()?;
    if let Ok(resolved) = state.storage.read_item_payload(item.id) {
        return resolved.full_text.or(resolved.item.plain_text);
    }
    item.full_text.or(item.plain_text)
}

#[tauri::command]
pub fn get_item_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<ItemDetailDto, ClipoError> {
    let resolved = state.storage.read_item_payload(id)?;
    Ok(ItemDetailDto {
        item: resolved.item,
        full_text: resolved.full_text,
        rtf_payload: resolved.rtf_payload,
        html_payload: resolved.html_payload,
        file_paths: resolved.file_paths,
    })
}

#[tauri::command]
pub fn toggle_pin(
    state: State<'_, AppState>,
    id: i64,
    shortcut: Option<String>,
) -> Result<HistoryItem, ClipoError> {
    state.storage.toggle_pin(id, shortcut)
}

#[tauri::command]
pub fn delete_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), ClipoError> {
    state.storage.delete_item(id)?;
    tray::sync_tray_state(&app);
    Ok(())
}

#[tauri::command]
pub fn clear_unpinned(app: AppHandle, state: State<'_, AppState>) -> Result<(), ClipoError> {
    state.storage.clear_unpinned_history()?;
    tray::sync_tray_state(&app);
    Ok(())
}

#[tauri::command]
pub fn save_item_as_snippet(
    state: State<'_, AppState>,
    item_id: i64,
    title: String,
    tags: Vec<String>,
    language: Option<String>,
    shortcut_key: Option<String>,
) -> Result<Snippet, ClipoError> {
    let resolved = state.storage.read_item_payload(item_id)?;
    let content = match resolved.item.kind {
        ItemKind::Text => resolved
            .full_text
            .or(resolved.item.plain_text)
            .unwrap_or_default(),
        ItemKind::File => resolved.file_paths.join("\n"),
        ItemKind::Image => {
            return Err(ClipoError::InvalidInput(
                "cannot save an image history item as a text snippet".to_string(),
            ));
        }
    };

    state.storage.upsert_snippet(
        None,
        SnippetInput {
            title,
            content,
            tags,
            language,
            shortcut_key,
            is_pinned: false,
        },
    )
}

#[tauri::command]
pub fn list_snippets(state: State<'_, AppState>) -> Result<Vec<Snippet>, ClipoError> {
    state.storage.list_snippets()
}

#[tauri::command]
pub fn upsert_snippet(
    state: State<'_, AppState>,
    id: Option<i64>,
    input: SnippetInput,
) -> Result<Snippet, ClipoError> {
    state.storage.upsert_snippet(id, input)
}

#[tauri::command]
pub fn delete_snippet(state: State<'_, AppState>, id: i64) -> Result<(), ClipoError> {
    state.storage.delete_snippet(id)
}

#[tauri::command]
pub fn export_snippets(state: State<'_, AppState>) -> Result<String, ClipoError> {
    let snippets = state.storage.list_snippets()?;
    export_snippets_json(&snippets)
}

#[tauri::command]
pub fn import_snippets(
    state: State<'_, AppState>,
    raw_json: String,
) -> Result<Vec<Snippet>, ClipoError> {
    let inputs = import_snippets_json(&raw_json)?;
    let mut created = Vec::with_capacity(inputs.len());
    for input in inputs {
        created.push(state.storage.upsert_snippet(None, input)?);
    }
    Ok(created)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, ClipoError> {
    state.storage.load_settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, ClipoError> {
    let prev = state.storage.load_settings()?;

    if prev.general.open_shortcut != settings.general.open_shortcut {
        state.sync_open_shortcut(&app, &settings.general.open_shortcut)?;
    }

    state.storage.save_settings(&settings)?;
    state.set_paused_flag(settings.advanced.turn_off);

    #[cfg(desktop)]
    {
        let autolaunch = app.autolaunch();
        if let Ok(currently_enabled) = autolaunch.is_enabled() {
            if settings.general.launch_at_login && !currently_enabled {
                let _ = autolaunch.enable();
            } else if !settings.general.launch_at_login && currently_enabled {
                let _ = autolaunch.disable();
            }
        }
    }

    if prev.advanced.poll_interval_ms != settings.advanced.poll_interval_ms {
        let interval = Duration::from_millis(settings.advanced.poll_interval_ms.max(50));
        let _ = state.restart_watcher(&app, interval);
    }

    tray::sync_tray_state(&app);
    let _ = app.emit("clipo://settings-updated", &settings);
    let _ = app.emit("clipo://runtime-status", state.runtime_status());

    Ok(settings)
}

#[tauri::command]
pub fn get_ignore_rules(state: State<'_, AppState>) -> Result<Vec<IgnoreRule>, ClipoError> {
    state.storage.list_ignore_rules()
}

#[tauri::command]
pub fn upsert_ignore_rule(
    state: State<'_, AppState>,
    id: Option<i64>,
    rule_type: IgnoreRuleType,
    pattern: String,
    enabled: bool,
) -> Result<IgnoreRule, ClipoError> {
    if rule_type == IgnoreRuleType::Regex {
        validate_ignore_regex(&pattern)?;
    }
    state
        .storage
        .upsert_ignore_rule(id, rule_type, pattern, enabled)
}

#[tauri::command]
pub fn delete_ignore_rule(state: State<'_, AppState>, id: i64) -> Result<(), ClipoError> {
    state.storage.delete_ignore_rule(id)
}

#[tauri::command]
pub fn get_storage_stats(state: State<'_, AppState>) -> Result<StorageStats, ClipoError> {
    state.storage.storage_stats()
}

#[tauri::command]
pub fn set_paused(
    app: AppHandle,
    state: State<'_, AppState>,
    paused: bool,
) -> Result<RuntimeStatusDto, ClipoError> {
    state.apply_paused(&app, paused)
}

#[tauri::command]
pub fn set_ignore_next(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RuntimeStatusDto, ClipoError> {
    Ok(state.trigger_ignore_next(&app))
}

#[tauri::command]
pub fn get_runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatusDto, ClipoError> {
    Ok(state.runtime_status())
}

#[tauri::command]
pub async fn open_settings_window(app: AppHandle, tab: Option<String>) -> Result<(), ClipoError> {
    windows_mgr::open_settings_window(&app, tab)
}

#[tauri::command]
pub fn hide_popup(app: AppHandle) -> Result<(), ClipoError> {
    windows_mgr::hide_popup(&app)
}

#[tauri::command]
pub fn get_platform_capabilities(
    state: State<'_, AppState>,
) -> Result<PlatformCapabilities, ClipoError> {
    Ok(state.backend.capabilities())
}

#[tauri::command]
pub fn open_accessibility_settings() -> Result<bool, ClipoError> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
        Ok(status.is_ok())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateStatusDto, ClipoError> {
    let current_version = app.package_info().version.to_string();

    #[cfg(not(desktop))]
    {
        return Ok(UpdateStatusDto {
            status: "fallback".to_string(),
            current_version,
            available_version: None,
            release_notes: None,
            release_url: CLIPO_RELEASES_URL.to_string(),
            message: Some(
                "Automatic updates are only supported on desktop builds. Visit GitHub Releases to check for updates."
                    .to_string(),
            ),
        });
    }

    #[cfg(desktop)]
    {
        let updater = match app.updater_builder().build() {
            Ok(updater) => updater,
            Err(_) => {
                return Ok(UpdateStatusDto {
                    status: "fallback".to_string(),
                    current_version,
                    available_version: None,
                    release_notes: None,
                    release_url: CLIPO_RELEASES_URL.to_string(),
                    message: Some(
                        "Automatic updater endpoint is not configured for this build. Visit GitHub Releases to check for updates."
                            .to_string(),
                    ),
                });
            }
        };

        match updater.check().await {
            Ok(Some(update)) => Ok(UpdateStatusDto {
                status: "update_available".to_string(),
                current_version,
                available_version: Some(update.version),
                release_notes: update.body,
                release_url: CLIPO_RELEASES_URL.to_string(),
                message: Some("A newer version of Clipo is available.".to_string()),
            }),
            Ok(None) => Ok(UpdateStatusDto {
                status: "up_to_date".to_string(),
                current_version,
                available_version: None,
                release_notes: None,
                release_url: CLIPO_RELEASES_URL.to_string(),
                message: Some("You are running the latest version of Clipo.".to_string()),
            }),
            Err(err) => Ok(UpdateStatusDto {
                status: "fallback".to_string(),
                current_version,
                available_version: None,
                release_notes: None,
                release_url: CLIPO_RELEASES_URL.to_string(),
                message: Some(format!("Unable to check updates automatically: {err}")),
            }),
        }
    }
}
