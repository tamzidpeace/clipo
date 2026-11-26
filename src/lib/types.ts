export type ItemKind = 'text' | 'image' | 'file';

export type SearchMode = 'fuzzy' | 'exact' | 'regex' | 'mixed';

export type SortBy = 'last_copied' | 'first_copied' | 'copy_count';

export type PopupPosition = 'cursor' | 'menu_icon' | 'screen_center' | 'active_window';

export type PinPosition = 'top' | 'bottom';

export type MatchHighlightStyle = 'bold' | 'underline' | 'highlight';

export type SearchFieldMode = 'always' | 'when_typing' | 'never';

export type ThemeMode = 'system' | 'light' | 'dark';

export type IgnoreRuleType = 'app' | 'pasteboard_type' | 'regex';

export type FilterTab = 'all' | 'snippets' | 'images' | 'files';

export const DEFAULT_FILTER_TABS: FilterTab[] = ['all', 'snippets', 'images', 'files'];

export type SearchRowKind = 'history' | 'snippet';

export interface HistoryItemDto {
  id: number;
  kind: ItemKind;
  content_hash: string;
  title_preview: string;
  full_text: string | null;
  plain_text: string | null;
  rtf_payload: string | null;
  html_payload: string | null;
  rich_blob_path: string | null;
  blob_path: string | null;
  thumb_path: string | null;
  image_width: number | null;
  image_height: number | null;
  byte_size: number;
  source_app_name: string | null;
  source_app_id: string | null;
  first_copied_at: number;
  last_copied_at: number;
  copy_count: number;
  is_pinned: boolean;
  pin_shortcut: string | null;
  pinned_at: number | null;
}

export interface SnippetDto {
  id: number;
  title: string;
  content: string;
  tags: string[];
  language: string | null;
  shortcut_key: string | null;
  is_pinned: boolean;
  use_count: number;
  created_at: number;
  updated_at: number;
}

export interface SnippetInputDto {
  title: string;
  content: string;
  tags: string[];
  language: string | null;
  shortcut_key: string | null;
  is_pinned: boolean;
}

export interface IgnoreRuleDto {
  id: number;
  rule_type: IgnoreRuleType;
  pattern: string;
  enabled: boolean;
}

export interface GeneralSettingsDto {
  launch_at_login: boolean;
  check_updates_automatically: boolean;
  open_shortcut: string;
  pin_shortcut: string;
  delete_shortcut: string;
  preview_shortcut: string;
  save_snippet_shortcut: string;
  search_mode: SearchMode;
  paste_automatically: boolean;
  paste_without_formatting: boolean;
}

export interface StorageSettingsDto {
  save_files: boolean;
  save_images: boolean;
  save_text: boolean;
  max_items: number;
  sort_by: SortBy;
}

export interface AppearanceSettingsDto {
  popup_at: PopupPosition;
  pin_to: PinPosition;
  image_height: number;
  open_preview_automatically: boolean;
  preview_delay_ms: number;
  highlight_matches: MatchHighlightStyle;
  show_special_symbols: boolean;
  show_menu_icon: boolean;
  menu_icon: string;
  show_recent_copy_next_to_menu_icon: boolean;
  search_field: SearchFieldMode;
  show_title_before_search_field: boolean;
  show_application_icons: boolean;
  show_hex_color_swatch: boolean;
  show_footer: boolean;
  theme: ThemeMode;
}

export interface AdvancedSettingsDto {
  turn_off: boolean;
  clear_history_on_quit: boolean;
  clear_system_clipboard_on_quit: boolean;
  ignore_all_apps_except_listed: boolean;
  poll_interval_ms: number;
  move_cursor_on_snippet_placeholder: boolean;
}

export interface AppSettingsDto {
  general: GeneralSettingsDto;
  storage: StorageSettingsDto;
  appearance: AppearanceSettingsDto;
  advanced: AdvancedSettingsDto;
}

export const DEFAULT_APP_SETTINGS: AppSettingsDto = {
  general: {
    launch_at_login: false,
    check_updates_automatically: true,
    open_shortcut: 'CommandOrControl+Shift+C',
    pin_shortcut: 'Alt+P',
    delete_shortcut: 'Alt+Backspace',
    preview_shortcut: 'Control+Space',
    save_snippet_shortcut: 'Alt+S',
    search_mode: 'fuzzy',
    paste_automatically: true,
    paste_without_formatting: false,
  },
  storage: {
    save_files: true,
    save_images: true,
    save_text: true,
    max_items: 200,
    sort_by: 'last_copied',
  },
  appearance: {
    popup_at: 'cursor',
    pin_to: 'top',
    image_height: 40,
    open_preview_automatically: true,
    preview_delay_ms: 1500,
    highlight_matches: 'bold',
    show_special_symbols: false,
    show_menu_icon: true,
    menu_icon: 'clipo',
    show_recent_copy_next_to_menu_icon: false,
    search_field: 'always',
    show_title_before_search_field: false,
    show_application_icons: true,
    show_hex_color_swatch: true,
    show_footer: true,
    theme: 'system',
  },
  advanced: {
    turn_off: false,
    clear_history_on_quit: false,
    clear_system_clipboard_on_quit: false,
    ignore_all_apps_except_listed: false,
    poll_interval_ms: 250,
    move_cursor_on_snippet_placeholder: false,
  },
};

export interface MatchRangeDto {
  start: number;
  end: number;
}

export interface SearchRowDto {
  row_kind: SearchRowKind;
  history_item: HistoryItemDto | null;
  snippet: SnippetDto | null;
  match_ranges: MatchRangeDto[];
  quick_pick_index: number | null;
  score: number;
}

export interface SearchResultDto {
  tab: FilterTab;
  query: string;
  rows: SearchRowDto[];
  total_history_count: number;
  total_snippet_count: number;
  regex_error: string | null;
}

export type PasteActionMode = 'copy_only' | 'copy_and_paste_rich' | 'copy_and_paste_plain';

export interface PasteOutcome {
  pasted: boolean;
  copied: boolean;
  degraded_reason: string | null;
}

export interface PlatformCapabilities {
  os: string;
  session_type: string;
  can_watch_clipboard: boolean;
  can_auto_paste: boolean;
  accessibility_granted: boolean | null;
  notes: string | null;
}

export interface StorageStats {
  total_items: number;
  pinned_items: number;
  unpinned_items: number;
  snippet_count: number;
  db_bytes: number;
  blob_bytes: number;
  total_bytes: number;
}

export interface ItemDetailDto {
  item: HistoryItemDto;
  full_text: string | null;
  rtf_payload: string | null;
  html_payload: string | null;
  file_paths: string[];
}

export interface RuntimeStatusDto {
  paused: boolean;
  ignore_next: boolean;
  capabilities: PlatformCapabilities;
}

export interface UpdateStatusDto {
  status: 'up_to_date' | 'update_available' | 'fallback' | 'error';
  current_version: string;
  available_version: string | null;
  release_notes: string | null;
  release_url: string;
  message: string | null;
}
