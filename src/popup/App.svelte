<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { matchesKeyboardEvent } from '../lib/hotkey';
  import {
    clearUnpinned,
    deleteItem,
    deleteSnippet,
    getItemDetail,
    getRuntimeStatus,
    getSettings,
    hidePopup,
    onHistoryChanged,
    onPopupOpened,
    onPopupReset,
    onRuntimeStatus,
    onSettingsChanged,
    openSettingsWindow,
    searchItems,
    selectItem,
    setPaused,
    setPreviewOpen,
    togglePin,
    upsertSnippet,
  } from '../lib/ipc';
  import {
    DEFAULT_APP_SETTINGS,
    DEFAULT_FILTER_TABS,
    type AppSettingsDto,
    type FilterTab,
    type HistoryItemDto,
    type ItemDetailDto,
    type PasteActionMode,
    type RuntimeStatusDto,
    type SearchResultDto,
    type SearchRowDto,
  } from '../lib/types';
  import FilterTabs from './FilterTabs.svelte';
  import FooterBar from './FooterBar.svelte';
  import ItemList from './ItemList.svelte';
  import PreviewPane from './PreviewPane.svelte';
  import QuickSnippetModal from './QuickSnippetModal.svelte';
  import SearchHeader from './SearchHeader.svelte';

  const isMac =
    typeof navigator !== 'undefined' &&
    /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent);

  let settings = $state<AppSettingsDto>(DEFAULT_APP_SETTINGS);
  let runtimeStatus = $state<RuntimeStatusDto | null>(null);
  let activeTab = $state<FilterTab>('all');
  let query = $state('');
  let searchResult = $state<SearchResultDto>({
    tab: 'all',
    query: '',
    rows: [],
    total_history_count: 0,
    total_snippet_count: 0,
    regex_error: null,
  });
  let selectedIndex = $state(0);

  let previewOpen = $state(false);
  let previewSide = $state<'left' | 'right'>('right');
  let previewDetail = $state<ItemDetailDto | null>(null);
  let previewLoading = $state(false);

  let quickSnippetOpen = $state(false);
  let quickSnippetBlank = $state(false);
  let quickSnippetSourceItem = $state<HistoryItemDto | null>(null);
  let quickSnippetSourceText = $state('');

  let systemPrefersDark = $state(
    typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches,
  );
  let searchInputEl: HTMLInputElement | null = $state(null);

  let searchSeq = 0;
  let detailSeq = 0;
  let previewLayoutSeq = 0;
  let popupSessionSeq = 0;
  let previewWasOpen = false;
  let autoPreviewTimer: ReturnType<typeof setTimeout> | null = null;

  let resolvedDark = $derived(
    settings.appearance.theme === 'dark' ||
      (settings.appearance.theme === 'system' && systemPrefersDark),
  );

  let selectedRow = $derived<SearchRowDto | null>(
    searchResult.rows[selectedIndex] ?? null,
  );

  let paused = $derived(
    runtimeStatus?.paused ?? settings.advanced.turn_off,
  );

  function clearAutoPreviewTimer(): void {
    if (autoPreviewTimer !== null) {
      clearTimeout(autoPreviewTimer);
      autoPreviewTimer = null;
    }
  }

  function scheduleAutoPreview(): void {
    clearAutoPreviewTimer();
    if (
      !settings.appearance.open_preview_automatically ||
      previewOpen ||
      !selectedRow
    ) {
      return;
    }
    const delay = Math.max(0, settings.appearance.preview_delay_ms);
    autoPreviewTimer = setTimeout(() => {
      if (selectedRow && !quickSnippetOpen) {
        previewOpen = true;
      }
    }, delay);
  }

  async function refreshSearch(preserveSelection = false): Promise<void> {
    const seq = ++searchSeq;
    const currentQuery = query;
    const currentTab = activeTab;
    try {
      const result = await searchItems(currentQuery, currentTab);
      if (seq !== searchSeq) {
        return;
      }
      searchResult = result;
      if (result.rows.length === 0) {
        selectedIndex = 0;
      } else if (preserveSelection) {
        selectedIndex = Math.min(selectedIndex, result.rows.length - 1);
      } else {
        selectedIndex = 0;
      }
    } catch {
      // Ignore transient IPC errors when running outside Tauri
    }
  }

  async function loadDetailForRow(row: SearchRowDto | null): Promise<void> {
    const seq = ++detailSeq;
    if (!row || row.row_kind !== 'history' || !row.history_item) {
      previewDetail = null;
      previewLoading = false;
      return;
    }
    const itemId = row.history_item.id;
    previewLoading = true;
    try {
      const detail = await getItemDetail(itemId);
      if (seq !== detailSeq) {
        return;
      }
      previewDetail = detail;
    } catch {
      if (seq === detailSeq) {
        previewDetail = null;
      }
    } finally {
      if (seq === detailSeq) {
        previewLoading = false;
      }
    }
  }

  function resetPopupState(): void {
    popupSessionSeq += 1;
    clearAutoPreviewTimer();
    query = '';
    activeTab = 'all';
    selectedIndex = 0;
    previewOpen = false;
    previewSide = 'right';
    previewDetail = null;
    previewLoading = false;
    quickSnippetOpen = false;
    quickSnippetBlank = false;
    quickSnippetSourceItem = null;
    quickSnippetSourceText = '';
    void refreshSearch(false);
  }

  function focusSearchInput(): void {
    void tick().then(() => {
      searchInputEl?.focus();
    });
  }

  function resolveEffectivePasteMode(
    explicit: PasteActionMode | null,
  ): PasteActionMode | null {
    if (explicit !== null) {
      return explicit;
    }
    if (!settings.general.paste_automatically) {
      return 'copy_only';
    }
    return settings.general.paste_without_formatting
      ? 'copy_and_paste_plain'
      : 'copy_and_paste_rich';
  }

  async function activateRow(
    row: SearchRowDto,
    explicitMode: PasteActionMode | null = null,
  ): Promise<void> {
    const id =
      row.row_kind === 'snippet' ? row.snippet?.id : row.history_item?.id;
    if (id === undefined) {
      return;
    }
    const mode = resolveEffectivePasteMode(explicitMode);
    await selectItem(row.row_kind, id, mode);
  }

  async function handlePinSelected(): Promise<void> {
    if (!selectedRow) {
      return;
    }
    if (selectedRow.row_kind === 'history' && selectedRow.history_item) {
      await togglePin(selectedRow.history_item.id, selectedRow.history_item.pin_shortcut);
      await refreshSearch(true);
    } else if (selectedRow.row_kind === 'snippet' && selectedRow.snippet) {
      const snippet = selectedRow.snippet;
      await upsertSnippet(snippet.id, {
        title: snippet.title,
        content: snippet.content,
        tags: snippet.tags,
        language: snippet.language,
        shortcut_key: snippet.shortcut_key,
        is_pinned: !snippet.is_pinned,
      });
      await refreshSearch(true);
    }
  }

  async function handleDeleteSelected(): Promise<void> {
    if (!selectedRow) {
      return;
    }
    if (selectedRow.row_kind === 'snippet' && selectedRow.snippet) {
      await deleteSnippet(selectedRow.snippet.id);
    } else if (selectedRow.history_item) {
      await deleteItem(selectedRow.history_item.id);
    }
    await refreshSearch(true);
  }

  async function openQuickSnippet(blank = false): Promise<void> {
    if (
      blank ||
      !selectedRow ||
      selectedRow.row_kind !== 'history' ||
      !selectedRow.history_item ||
      selectedRow.history_item.kind === 'image'
    ) {
      quickSnippetBlank = true;
      quickSnippetSourceItem = null;
      quickSnippetSourceText = '';
      quickSnippetOpen = true;
      return;
    }

    const session = popupSessionSeq;
    const item = selectedRow.history_item;
    quickSnippetBlank = false;
    quickSnippetSourceItem = item;
    let text = item.full_text ?? item.plain_text ?? item.title_preview;
    try {
      const detail = await getItemDetail(item.id);
      if (session !== popupSessionSeq) {
        return;
      }
      if (item.kind === 'file') {
        text = detail.file_paths.join('\n');
      } else {
        text =
          detail.full_text ??
          detail.item.full_text ??
          detail.item.plain_text ??
          item.title_preview;
      }
    } catch {
      if (session !== popupSessionSeq) {
        return;
      }
      // Fallback to item preview text
    }
    quickSnippetSourceText = text;
    quickSnippetOpen = true;
  }

  function cycleTab(direction: 1 | -1): void {
    const idx = DEFAULT_FILTER_TABS.indexOf(activeTab);
    const nextIdx =
      (idx + direction + DEFAULT_FILTER_TABS.length) % DEFAULT_FILTER_TABS.length;
    activeTab = DEFAULT_FILTER_TABS[nextIdx];
    void refreshSearch(false);
  }

  function handleKeyDown(e: KeyboardEvent): void {
    if (quickSnippetOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        quickSnippetOpen = false;
      }
      return;
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      if (previewOpen) {
        previewOpen = false;
      } else {
        void hidePopup();
      }
      return;
    }

    if (e.key === 'Tab') {
      e.preventDefault();
      cycleTab(e.shiftKey ? -1 : 1);
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (searchResult.rows.length > 0) {
        selectedIndex = (selectedIndex + 1) % searchResult.rows.length;
        scheduleAutoPreview();
      }
      return;
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (searchResult.rows.length > 0) {
        selectedIndex =
          (selectedIndex - 1 + searchResult.rows.length) %
          searchResult.rows.length;
        scheduleAutoPreview();
      }
      return;
    }

    const primaryMod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
    if (matchesKeyboardEvent(e, 'CommandOrControl+,', isMac)) {
      e.preventDefault();
      void openSettingsWindow('general');
      return;
    }

    if (matchesKeyboardEvent(e, 'Alt+Shift+Backspace', isMac)) {
      e.preventDefault();
      void clearUnpinned().then(() => refreshSearch(false));
      return;
    }

    if (matchesKeyboardEvent(e, settings.general.preview_shortcut, isMac)) {
      e.preventDefault();
      previewOpen = !previewOpen;
      clearAutoPreviewTimer();
      return;
    }

    if (matchesKeyboardEvent(e, settings.general.pin_shortcut, isMac)) {
      e.preventDefault();
      void handlePinSelected();
      return;
    }

    if (matchesKeyboardEvent(e, settings.general.save_snippet_shortcut, isMac)) {
      e.preventDefault();
      void openQuickSnippet(false);
      return;
    }

    if (matchesKeyboardEvent(e, settings.general.delete_shortcut, isMac)) {
      e.preventDefault();
      void handleDeleteSelected();
      return;
    }

    // Cmd+1..9 (macOS) / Ctrl+1..9 (Win/Linux) quick-pick
    if (primaryMod && !e.altKey && !e.shiftKey && /^[1-9]$/.test(e.key)) {
      const targetPick = Number(e.key);
      const match = searchResult.rows.find(
        (r) => r.quick_pick_index === targetPick,
      );
      if (match) {
        e.preventDefault();
        void activateRow(match, null);
      }
      return;
    }

    // Enter / Alt+Enter / Cmd+Enter / Cmd+Shift+Enter
    if (e.key === 'Enter' && selectedRow) {
      e.preventDefault();
      const cmdOrCtrl = isMac ? e.metaKey : e.ctrlKey;
      let explicitMode: PasteActionMode | null = null;
      if (e.altKey && !cmdOrCtrl) {
        explicitMode = 'copy_only';
      } else if (cmdOrCtrl && e.shiftKey) {
        explicitMode = 'copy_and_paste_plain';
      } else if (cmdOrCtrl) {
        explicitMode = 'copy_and_paste_rich';
      }
      void activateRow(selectedRow, explicitMode);
      return;
    }

    // Custom per-item pin_shortcut or snippet shortcut_key
    for (const row of searchResult.rows) {
      const custom =
        row.row_kind === 'snippet'
          ? row.snippet?.shortcut_key
          : row.history_item?.pin_shortcut;
      if (!custom) {
        continue;
      }
      const isSingleKey = !custom.includes('+');
      const searchFocused = document.activeElement === searchInputEl;
      if (isSingleKey && (searchFocused || query.length > 0)) {
        continue;
      }
      if (matchesKeyboardEvent(e, custom, isMac)) {
        e.preventDefault();
        void activateRow(row, null);
        return;
      }
    }

    // In `when_typing` mode, typing printable characters starts search immediately.
    if (
      settings.appearance.search_field === 'when_typing' &&
      !e.isComposing &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      e.key.length === 1 &&
      document.activeElement !== searchInputEl
    ) {
      e.preventDefault();
      query = query + e.key;
      void refreshSearch(false);
      focusSearchInput();
    }
  }

  $effect(() => {
    if (previewOpen && selectedRow) {
      void loadDetailForRow(selectedRow);
    } else {
      detailSeq += 1;
      previewDetail = null;
      previewLoading = false;
    }
  });

  $effect(() => {
    const open = previewOpen;
    if (!open && !previewWasOpen) {
      return;
    }
    previewWasOpen = open;
    const seq = ++previewLayoutSeq;
    void setPreviewOpen(open)
      .then((side) => {
        if (seq === previewLayoutSeq) {
          previewSide = side;
        }
      })
      .catch(() => {
        if (seq === previewLayoutSeq && !open) {
          previewSide = 'right';
        }
      });
  });

  onMount(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onMediaChange = (ev: MediaQueryListEvent) => {
      systemPrefersDark = ev.matches;
    };
    mq.addEventListener('change', onMediaChange);

    let unlisteners: Array<() => void> = [];
    let disposed = false;

    void (async () => {
      try {
        const [loadedSettings, loadedStatus] = await Promise.all([
          getSettings(),
          getRuntimeStatus(),
        ]);
        if (!disposed) {
          settings = loadedSettings;
          runtimeStatus = loadedStatus;
        }
      } catch {
        // Fallback to defaults outside Tauri
      }
      if (!disposed) {
        await refreshSearch(false);
        focusSearchInput();
      }

      const fns = await Promise.all([
        onPopupOpened(() => {
          void refreshSearch(false);
          focusSearchInput();
          scheduleAutoPreview();
        }),
        onPopupReset(() => {
          resetPopupState();
        }),
        onHistoryChanged(() => {
          void refreshSearch(true);
        }),
        onSettingsChanged((nextSettings) => {
          settings = nextSettings;
          void refreshSearch(true);
        }),
        onRuntimeStatus((nextStatus) => {
          runtimeStatus = nextStatus;
        }),
      ]);
      if (disposed) {
        fns.forEach((fn) => fn());
      } else {
        unlisteners = fns;
      }
    })();

    return () => {
      disposed = true;
      clearAutoPreviewTimer();
      mq.removeEventListener('change', onMediaChange);
      unlisteners.forEach((fn) => fn());
    };
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

<main
  class={`flex h-screen w-screen flex-col overflow-hidden rounded-xl border backdrop-blur-2xl ${resolvedDark ? 'theme-dark' : 'theme-light'}`}
  style:background-color="var(--clipo-surface)"
  style:border-color="var(--clipo-border)"
  style:color="var(--clipo-text)"
>
  <SearchHeader
    {query}
    searchMode={settings.general.search_mode}
    searchFieldMode={settings.appearance.search_field}
    showTitleBeforeSearchField={settings.appearance.show_title_before_search_field}
    regexError={searchResult.regex_error}
    {paused}
    bind:inputEl={searchInputEl}
    onQueryChange={(val) => {
      query = val;
      void refreshSearch(false);
      scheduleAutoPreview();
    }}
    onClearQuery={() => {
      query = '';
      void refreshSearch(false);
      focusSearchInput();
    }}
  />

  <FilterTabs
    {activeTab}
    tabs={DEFAULT_FILTER_TABS}
    totalHistoryCount={searchResult.total_history_count}
    totalSnippetCount={searchResult.total_snippet_count}
    onSelectTab={(tab) => {
      activeTab = tab;
      void refreshSearch(false);
    }}
  />

  {#snippet previewSidecar()}
    <PreviewPane
      row={selectedRow}
      detail={previewDetail}
      loading={previewLoading}
      {isMac}
      side={previewSide}
      pinShortcut={settings.general.pin_shortcut}
      saveSnippetShortcut={settings.general.save_snippet_shortcut}
      deleteShortcut={settings.general.delete_shortcut}
      onClose={() => {
        previewOpen = false;
      }}
    />
  {/snippet}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if previewOpen && previewSide === 'left'}
      {@render previewSidecar()}
    {/if}

    <div class="flex min-h-0 flex-1 min-w-0 flex-col">
      <ItemList
        rows={searchResult.rows}
        {selectedIndex}
        highlightStyle={settings.appearance.highlight_matches}
        showSpecialSymbols={settings.appearance.show_special_symbols}
        showApplicationIcons={settings.appearance.show_application_icons}
        showHexColorSwatch={settings.appearance.show_hex_color_swatch}
        imageHeight={settings.appearance.image_height}
        {isMac}
        {query}
        onSelectIndex={(idx) => {
          selectedIndex = idx;
          scheduleAutoPreview();
        }}
        onActivateRow={(row, ev) => {
          const cmdOrCtrl = isMac ? ev.metaKey : ev.ctrlKey;
          let mode: PasteActionMode | null = null;
          if (ev.altKey && !cmdOrCtrl) {
            mode = 'copy_only';
          } else if (cmdOrCtrl && ev.shiftKey) {
            mode = 'copy_and_paste_plain';
          } else if (cmdOrCtrl) {
            mode = 'copy_and_paste_rich';
          }
          void activateRow(row, mode);
        }}
      />
    </div>

    {#if previewOpen && previewSide === 'right'}
      {@render previewSidecar()}
    {/if}
  </div>

  {#if settings.appearance.show_footer}
    <FooterBar
      {paused}
      itemCount={searchResult.rows.length}
      {previewOpen}
      onTogglePause={async () => {
        runtimeStatus = await setPaused(!paused);
      }}
      onNewSnippet={() => {
        void openQuickSnippet(true);
      }}
      onClearUnpinned={async () => {
        await clearUnpinned();
        await refreshSearch(false);
      }}
      onOpenPreferences={() => {
        void openSettingsWindow('general');
      }}
      onTogglePreview={() => {
        previewOpen = !previewOpen;
        clearAutoPreviewTimer();
      }}
    />
  {/if}

  <QuickSnippetModal
    open={quickSnippetOpen}
    sourceItem={quickSnippetBlank ? null : quickSnippetSourceItem}
    sourceText={quickSnippetSourceText}
    onClose={() => {
      quickSnippetOpen = false;
      focusSearchInput();
    }}
    onSaved={() => {
      quickSnippetOpen = false;
      void refreshSearch(true);
      focusSearchInput();
    }}
  />
</main>
