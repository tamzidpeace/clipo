use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unified error type for all Clipo backend operations.
#[derive(Debug, Error)]
pub enum ClipoError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("clipboard error: {0}")]
    Clipboard(String),

    #[error("paste error: {0}")]
    Paste(String),

    #[error("search error: {0}")]
    Search(String),

    #[error("snippet error: {0}")]
    Snippet(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl Serialize for ClipoError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Classification of a captured clipboard item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Text,
    Image,
    File,
}

impl ItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::File => "file",
        }
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ItemKind {
    type Err = ClipoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "file" => Ok(Self::File),
            other => Err(ClipoError::InvalidInput(format!(
                "unknown item kind: {other}"
            ))),
        }
    }
}

/// Search matching strategy in the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    Fuzzy,
    Exact,
    Regex,
    Mixed,
}

/// Sort ordering for unpinned clipboard history items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    LastCopied,
    FirstCopied,
    CopyCount,
}

/// Screen placement strategy when opening the popup window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PopupPosition {
    #[default]
    Cursor,
    MenuIcon,
    ScreenCenter,
    ActiveWindow,
}

/// Where pinned history items appear relative to unpinned items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PinPosition {
    #[default]
    Top,
    Bottom,
}

/// Visual treatment for matched search characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchHighlightStyle {
    #[default]
    Bold,
    Underline,
    Highlight,
}

/// Visibility behavior for the popup search input field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchFieldMode {
    #[default]
    Always,
    WhenTyping,
    Never,
}

/// Application color theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Kind of clipboard ignore rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreRuleType {
    App,
    PasteboardType,
    Regex,
}

impl IgnoreRuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::App => "app",
            Self::PasteboardType => "pasteboard_type",
            Self::Regex => "regex",
        }
    }
}

impl fmt::Display for IgnoreRuleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for IgnoreRuleType {
    type Err = ClipoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "app" => Ok(Self::App),
            "pasteboard_type" => Ok(Self::PasteboardType),
            "regex" => Ok(Self::Regex),
            other => Err(ClipoError::InvalidInput(format!(
                "unknown ignore rule type: {other}"
            ))),
        }
    }
}

/// Popup filter strip tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterTab {
    #[default]
    All,
    Snippets,
    Images,
    Files,
}

/// Stored clipboard history item DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: i64,
    pub kind: ItemKind,
    pub content_hash: String,
    pub title_preview: String,
    pub full_text: Option<String>,
    pub plain_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub rich_blob_path: Option<String>,
    pub blob_path: Option<String>,
    pub thumb_path: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub byte_size: i64,
    pub source_app_name: Option<String>,
    pub source_app_id: Option<String>,
    pub first_copied_at: i64,
    pub last_copied_at: i64,
    pub copy_count: i64,
    pub is_pinned: bool,
    pub pin_shortcut: Option<String>,
    pub pinned_at: Option<i64>,
}

/// Stored reusable snippet DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub shortcut_key: Option<String>,
    pub is_pinned: bool,
    pub use_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Payload for creating or updating a snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetInput {
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub shortcut_key: Option<String>,
    pub is_pinned: bool,
}

/// Stored ignore rule DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoreRule {
    pub id: i64,
    pub rule_type: IgnoreRuleType,
    pub pattern: String,
    pub enabled: bool,
}

/// General behavior and shortcut settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub check_updates_automatically: bool,
    pub open_shortcut: String,
    pub pin_shortcut: String,
    pub delete_shortcut: String,
    pub preview_shortcut: String,
    pub save_snippet_shortcut: String,
    pub search_mode: SearchMode,
    pub paste_automatically: bool,
    pub paste_without_formatting: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            check_updates_automatically: true,
            open_shortcut: "CommandOrControl+Shift+C".to_string(),
            pin_shortcut: "Alt+P".to_string(),
            delete_shortcut: "Alt+Backspace".to_string(),
            preview_shortcut: "Control+Space".to_string(),
            save_snippet_shortcut: "Alt+S".to_string(),
            search_mode: SearchMode::Fuzzy,
            paste_automatically: true,
            paste_without_formatting: false,
        }
    }
}

/// Storage and retention settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageSettings {
    pub save_files: bool,
    pub save_images: bool,
    pub save_text: bool,
    pub max_items: usize,
    pub sort_by: SortBy,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            save_files: true,
            save_images: true,
            save_text: true,
            max_items: 200,
            sort_by: SortBy::LastCopied,
        }
    }
}

/// Popup appearance and preview settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub popup_at: PopupPosition,
    pub pin_to: PinPosition,
    pub image_height: u32,
    pub open_preview_automatically: bool,
    pub preview_delay_ms: u64,
    pub highlight_matches: MatchHighlightStyle,
    pub show_special_symbols: bool,
    pub show_menu_icon: bool,
    pub menu_icon: String,
    pub show_recent_copy_next_to_menu_icon: bool,
    pub search_field: SearchFieldMode,
    pub show_title_before_search_field: bool,
    pub show_application_icons: bool,
    pub show_hex_color_swatch: bool,
    pub show_footer: bool,
    pub theme: ThemeMode,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            popup_at: PopupPosition::Cursor,
            pin_to: PinPosition::Top,
            image_height: 40,
            open_preview_automatically: true,
            preview_delay_ms: 1500,
            highlight_matches: MatchHighlightStyle::Bold,
            show_special_symbols: false,
            show_menu_icon: true,
            menu_icon: "clipo".to_string(),
            show_recent_copy_next_to_menu_icon: false,
            search_field: SearchFieldMode::Always,
            show_title_before_search_field: false,
            show_application_icons: true,
            show_hex_color_swatch: true,
            show_footer: true,
            theme: ThemeMode::System,
        }
    }
}

/// Advanced capture, privacy, and IPC settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedSettings {
    pub turn_off: bool,
    pub clear_history_on_quit: bool,
    pub clear_system_clipboard_on_quit: bool,
    pub ignore_all_apps_except_listed: bool,
    pub poll_interval_ms: u64,
    pub move_cursor_on_snippet_placeholder: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            turn_off: false,
            clear_history_on_quit: false,
            clear_system_clipboard_on_quit: false,
            ignore_all_apps_except_listed: false,
            poll_interval_ms: 250,
            move_cursor_on_snippet_placeholder: false,
        }
    }
}

/// Complete persisted application settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppSettings {
    pub general: GeneralSettings,
    pub storage: StorageSettings,
    pub appearance: AppearanceSettings,
    pub advanced: AdvancedSettings,
}

/// Raw clipboard capture normalized by the platform watcher before storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedPayload {
    pub kind: ItemKind,
    pub plain_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub file_paths: Vec<String>,
    pub source_app_name: Option<String>,
    pub source_app_id: Option<String>,
    pub pasteboard_types: Vec<String>,
    pub is_concealed_or_transient: bool,
}

/// Kind of entry in a popup search result row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchRowKind {
    History,
    Snippet,
}

/// Half-open byte/character range for match highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRange {
    pub start: usize,
    pub end: usize,
}

/// Single row returned to the popup search UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRowDto {
    pub row_kind: SearchRowKind,
    pub history_item: Option<HistoryItem>,
    pub snippet: Option<Snippet>,
    pub match_ranges: Vec<MatchRange>,
    pub quick_pick_index: Option<u8>,
    pub score: i64,
}

/// Search response payload for a popup query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultDto {
    pub tab: FilterTab,
    pub query: String,
    pub rows: Vec<SearchRowDto>,
    pub total_history_count: usize,
    pub total_snippet_count: usize,
    pub regex_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_match_maccy_defaults() {
        let settings = AppSettings::default();
        assert_eq!(settings.storage.max_items, 200);
        assert!(settings.storage.save_text);
        assert!(settings.storage.save_images);
        assert!(settings.storage.save_files);
        assert_eq!(settings.storage.sort_by, SortBy::LastCopied);
        assert_eq!(settings.appearance.image_height, 40);
        assert!(settings.appearance.open_preview_automatically);
        assert_eq!(settings.appearance.preview_delay_ms, 1500);
        assert_eq!(settings.appearance.pin_to, PinPosition::Top);
        assert_eq!(settings.appearance.popup_at, PopupPosition::Cursor);
        assert_eq!(
            settings.appearance.highlight_matches,
            MatchHighlightStyle::Bold
        );
        assert_eq!(settings.appearance.search_field, SearchFieldMode::Always);
        assert_eq!(settings.appearance.theme, ThemeMode::System);
        assert_eq!(settings.general.search_mode, SearchMode::Fuzzy);
        assert!(settings.general.paste_automatically);
        assert!(!settings.general.paste_without_formatting);
    }

    #[test]
    fn test_app_settings_json_roundtrip() {
        let mut settings = AppSettings::default();
        settings.general.search_mode = SearchMode::Mixed;
        settings.storage.sort_by = SortBy::CopyCount;
        settings.appearance.popup_at = PopupPosition::ScreenCenter;
        settings.appearance.pin_to = PinPosition::Bottom;
        settings.appearance.highlight_matches = MatchHighlightStyle::Highlight;
        settings.appearance.search_field = SearchFieldMode::WhenTyping;
        settings.appearance.theme = ThemeMode::Dark;

        let json = serde_json::to_string(&settings).expect("serialize AppSettings");
        let parsed: AppSettings = serde_json::from_str(&json).expect("deserialize AppSettings");
        assert_eq!(parsed, settings);
    }

    #[test]
    fn test_enum_serde_snake_case_strings() {
        assert_eq!(serde_json::to_string(&ItemKind::Text).unwrap(), "\"text\"");
        assert_eq!(
            serde_json::to_string(&FilterTab::Snippets).unwrap(),
            "\"snippets\""
        );
        assert_eq!(
            serde_json::to_string(&IgnoreRuleType::PasteboardType).unwrap(),
            "\"pasteboard_type\""
        );
        assert_eq!(
            serde_json::to_string(&PopupPosition::ActiveWindow).unwrap(),
            "\"active_window\""
        );
    }
}
