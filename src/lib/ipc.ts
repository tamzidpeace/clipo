import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppSettingsDto,
  FilterTab,
  HistoryItemDto,
  IgnoreRuleDto,
  IgnoreRuleType,
  ItemDetailDto,
  PasteActionMode,
  PasteOutcome,
  PlatformCapabilities,
  RuntimeStatusDto,
  SearchResultDto,
  SearchRowKind,
  SnippetDto,
  SnippetInputDto,
  StorageStats,
  UpdateStatusDto,
} from './types';

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function listenSafe<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<T>(event, (ev) => handler(ev.payload));
}

export async function searchItems(
  query: string,
  tab: FilterTab,
): Promise<SearchResultDto> {
  return invoke<SearchResultDto>('search_items', { query, tab });
}

export async function selectItem(
  rowKind: SearchRowKind,
  id: number,
  mode: PasteActionMode | null = null,
): Promise<PasteOutcome> {
  return invoke<PasteOutcome>('select_item', { rowKind, id, mode });
}

export async function getItemDetail(id: number): Promise<ItemDetailDto> {
  return invoke<ItemDetailDto>('get_item_detail', { id });
}

export async function togglePin(
  id: number,
  shortcut: string | null = null,
): Promise<HistoryItemDto> {
  return invoke<HistoryItemDto>('toggle_pin', { id, shortcut });
}

export async function deleteItem(id: number): Promise<void> {
  return invoke<void>('delete_item', { id });
}

export async function clearUnpinned(): Promise<void> {
  return invoke<void>('clear_unpinned');
}

export interface SaveItemAsSnippetParams {
  itemId: number;
  title: string;
  tags: string[];
  language: string | null;
  shortcutKey: string | null;
  isPinned?: boolean;
}

export async function saveItemAsSnippet(
  params: SaveItemAsSnippetParams,
): Promise<SnippetDto> {
  const created = await invoke<SnippetDto>('save_item_as_snippet', {
    itemId: params.itemId,
    title: params.title,
    tags: params.tags,
    language: params.language,
    shortcutKey: params.shortcutKey,
  });

  if (params.isPinned && !created.is_pinned) {
    return upsertSnippet(created.id, {
      title: created.title,
      content: created.content,
      tags: created.tags,
      language: created.language,
      shortcut_key: created.shortcut_key,
      is_pinned: true,
    });
  }

  return created;
}

export async function listSnippets(): Promise<SnippetDto[]> {
  return invoke<SnippetDto[]>('list_snippets');
}

export async function upsertSnippet(
  id: number | null,
  input: SnippetInputDto,
): Promise<SnippetDto> {
  return invoke<SnippetDto>('upsert_snippet', { id, input });
}

export async function deleteSnippet(id: number): Promise<void> {
  return invoke<void>('delete_snippet', { id });
}

export async function exportSnippets(): Promise<string> {
  return invoke<string>('export_snippets');
}

export async function importSnippets(rawJson: string): Promise<SnippetDto[]> {
  return invoke<SnippetDto[]>('import_snippets', { rawJson });
}

export async function getSettings(): Promise<AppSettingsDto> {
  return invoke<AppSettingsDto>('get_settings');
}

export async function updateSettings(
  settings: AppSettingsDto,
): Promise<AppSettingsDto> {
  return invoke<AppSettingsDto>('update_settings', { settings });
}

export async function getIgnoreRules(): Promise<IgnoreRuleDto[]> {
  return invoke<IgnoreRuleDto[]>('get_ignore_rules');
}

export async function upsertIgnoreRule(
  id: number | null,
  ruleType: IgnoreRuleType,
  pattern: string,
  enabled: boolean,
): Promise<IgnoreRuleDto> {
  return invoke<IgnoreRuleDto>('upsert_ignore_rule', {
    id,
    ruleType,
    pattern,
    enabled,
  });
}

export async function deleteIgnoreRule(id: number): Promise<void> {
  return invoke<void>('delete_ignore_rule', { id });
}

export async function getStorageStats(): Promise<StorageStats> {
  return invoke<StorageStats>('get_storage_stats');
}

export async function setPaused(paused: boolean): Promise<RuntimeStatusDto> {
  return invoke<RuntimeStatusDto>('set_paused', { paused });
}

export async function setIgnoreNext(): Promise<RuntimeStatusDto> {
  return invoke<RuntimeStatusDto>('set_ignore_next');
}

export async function getRuntimeStatus(): Promise<RuntimeStatusDto> {
  return invoke<RuntimeStatusDto>('get_runtime_status');
}

export async function openSettingsWindow(
  tab: string | null = null,
): Promise<void> {
  return invoke<void>('open_settings_window', { tab });
}

export async function hidePopup(): Promise<void> {
  return invoke<void>('hide_popup');
}

export async function setPreviewOpen(open: boolean): Promise<'left' | 'right'> {
  if (!isTauriRuntime()) {
    return 'right';
  }
  const side = await invoke<string>('set_preview_open', { open });
  return side === 'left' ? 'left' : 'right';
}

export async function getPlatformCapabilities(): Promise<PlatformCapabilities> {
  return invoke<PlatformCapabilities>('get_platform_capabilities');
}

export async function openAccessibilitySettings(): Promise<boolean> {
  return invoke<boolean>('open_accessibility_settings');
}

export async function checkForUpdates(): Promise<UpdateStatusDto> {
  return invoke<UpdateStatusDto>('check_for_updates');
}

export async function onPopupOpened(handler: () => void): Promise<UnlistenFn> {
  return listenSafe<void>('clipo://popup-opened', () => handler());
}

export async function onPopupReset(handler: () => void): Promise<UnlistenFn> {
  return listenSafe<void>('clipo://popup-reset', () => handler());
}

export async function onHistoryChanged(handler: () => void): Promise<UnlistenFn> {
  return listenSafe<void>('clipo://history-changed', () => handler());
}

export async function onSettingsChanged(
  handler: (settings: AppSettingsDto) => void,
): Promise<UnlistenFn> {
  const unlistenUpdated = await listenSafe<AppSettingsDto>(
    'clipo://settings-updated',
    handler,
  );
  const unlistenChanged = await listenSafe<AppSettingsDto>(
    'clipo://settings-changed',
    handler,
  );
  return () => {
    unlistenUpdated();
    unlistenChanged();
  };
}

export async function onRuntimeStatus(
  handler: (status: RuntimeStatusDto) => void,
): Promise<UnlistenFn> {
  return listenSafe<RuntimeStatusDto>('clipo://runtime-status', handler);
}

export async function onSettingsTab(
  handler: (tab: string) => void,
): Promise<UnlistenFn> {
  return listenSafe<string>('clipo://settings-tab', handler);
}

export async function onCheckUpdates(handler: () => void): Promise<UnlistenFn> {
  return listenSafe<void>('clipo://check-updates', () => handler());
}
