<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import {
    checkForUpdates,
    clearUnpinned,
    deleteIgnoreRule,
    getIgnoreRules,
    getPlatformCapabilities,
    getRuntimeStatus,
    getSettings,
    getStorageStats,
    isTauriRuntime,
    onCheckUpdates,
    onHistoryChanged,
    onRuntimeStatus,
    onSettingsChanged,
    onSettingsTab,
    openAccessibilitySettings,
    setIgnoreNext,
    setPaused,
    updateSettings,
    upsertIgnoreRule,
  } from '../lib/ipc';
  import {
    DEFAULT_APP_SETTINGS,
    type AdvancedSettingsDto,
    type AppearanceSettingsDto,
    type AppSettingsDto,
    type GeneralSettingsDto,
    type IgnoreRuleDto,
    type IgnoreRuleType,
    type PlatformCapabilities,
    type RuntimeStatusDto,
    type StorageSettingsDto,
    type StorageStats,
    type UpdateStatusDto,
  } from '../lib/types';
  import AdvancedTab from './AdvancedTab.svelte';
  import AppearanceTab from './AppearanceTab.svelte';
  import GeneralTab from './GeneralTab.svelte';
  import IgnoreTab from './IgnoreTab.svelte';
  import SnippetsTab from './SnippetsTab.svelte';
  import StorageTab from './StorageTab.svelte';

  type SettingsTabId =
    | 'general'
    | 'storage'
    | 'appearance'
    | 'ignore'
    | 'snippets'
    | 'advanced';

  const TABS: { id: SettingsTabId; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'storage', label: 'Storage' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'ignore', label: 'Ignore' },
    { id: 'snippets', label: 'Snippets' },
    { id: 'advanced', label: 'Advanced' },
  ];

  const isMac =
    typeof navigator !== 'undefined' &&
    /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent);

  let activeTab = $state<SettingsTabId>('general');
  let settings = $state<AppSettingsDto>(DEFAULT_APP_SETTINGS);
  let runtimeStatus = $state<RuntimeStatusDto | null>(null);
  let capabilities = $state<PlatformCapabilities | null>(null);
  let ignoreRules = $state<IgnoreRuleDto[]>([]);
  let loadingIgnoreRules = $state(false);
  let storageStats = $state<StorageStats | null>(null);
  let loadingStats = $state(false);
  let updateStatus = $state<UpdateStatusDto | null>(null);
  let checkingUpdates = $state(false);

  let bannerError = $state<string | null>(null);
  let bannerNotice = $state<string | null>(null);
  let activeRecorders = $state(0);
  let activeModals = $state(0);
  let snippetsVisited = $state(false);
  let snippetsDirty = $state(false);
  let persistSeq = 0;
  let inFlightPersists = 0;

  let systemPrefersDark = $state(
    typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches,
  );

  let resolvedDark = $derived(
    settings.appearance.theme === 'dark' ||
      (settings.appearance.theme === 'system' && systemPrefersDark),
  );

  function normalizeTab(raw: string | null | undefined): SettingsTabId | null {
    if (!raw) {
      return null;
    }
    const lower = raw.trim().toLowerCase();
    const match = TABS.find((t) => t.id === lower);
    return match ? match.id : null;
  }

  function handleRecordingChange(recording: boolean): void {
    activeRecorders = Math.max(0, activeRecorders + (recording ? 1 : -1));
  }

  function handleModalOpenChange(open: boolean): void {
    activeModals = Math.max(0, activeModals + (open ? 1 : -1));
  }

  async function refreshStorageStats(): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    loadingStats = true;
    try {
      storageStats = await getStorageStats();
    } catch {
      // Non-fatal if stats fail transiently
    } finally {
      loadingStats = false;
    }
  }

  async function refreshIgnoreRules(): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    loadingIgnoreRules = true;
    try {
      ignoreRules = await getIgnoreRules();
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    } finally {
      loadingIgnoreRules = false;
    }
  }

  async function persistSettings(next: AppSettingsDto): Promise<void> {
    const previous = settings;
    settings = next;
    bannerError = null;
    bannerNotice = null;
    if (!isTauriRuntime()) {
      return;
    }
    const seq = ++persistSeq;
    inFlightPersists += 1;
    try {
      const saved = await updateSettings(next);
      if (seq === persistSeq) {
        settings = saved;
      }
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
      if (seq === persistSeq) {
        try {
          const latest = await getSettings();
          if (seq === persistSeq) {
            settings = latest;
          }
        } catch {
          if (seq === persistSeq) {
            settings = previous;
          }
        }
      }
    } finally {
      inFlightPersists = Math.max(0, inFlightPersists - 1);
    }
  }

  function updateGeneral(patch: Partial<GeneralSettingsDto>): void {
    void persistSettings({
      ...settings,
      general: { ...settings.general, ...patch },
    });
  }

  function updateStorage(patch: Partial<StorageSettingsDto>): void {
    void persistSettings({
      ...settings,
      storage: { ...settings.storage, ...patch },
    });
  }

  function updateAppearance(patch: Partial<AppearanceSettingsDto>): void {
    void persistSettings({
      ...settings,
      appearance: { ...settings.appearance, ...patch },
    });
  }

  function updateAdvanced(patch: Partial<AdvancedSettingsDto>): void {
    void persistSettings({
      ...settings,
      advanced: { ...settings.advanced, ...patch },
    });
  }

  async function handleCheckUpdates(): Promise<void> {
    activeTab = 'general';
    bannerError = null;
    if (!isTauriRuntime()) {
      updateStatus = {
        status: 'fallback',
        current_version: '0.1.0',
        available_version: null,
        release_notes: null,
        release_url: 'https://github.com/tamzidpeace/clipo/releases',
        message: 'Update checks require the desktop Tauri runtime.',
      };
      return;
    }
    checkingUpdates = true;
    try {
      updateStatus = await checkForUpdates();
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    } finally {
      checkingUpdates = false;
    }
  }

  async function handleOpenAccessibility(): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    try {
      const opened = await openAccessibilitySettings();
      if (!opened) {
        bannerNotice = 'Open System Settings → Privacy & Security → Accessibility to grant permission.';
      }
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleUpsertIgnoreRule(
    id: number | null,
    ruleType: IgnoreRuleType,
    pattern: string,
    enabled: boolean,
  ): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    await upsertIgnoreRule(id, ruleType, pattern, enabled);
    await refreshIgnoreRules();
  }

  async function handleDeleteIgnoreRule(id: number): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    await deleteIgnoreRule(id);
    await refreshIgnoreRules();
  }

  async function handleTogglePaused(paused: boolean): Promise<void> {
    bannerError = null;
    if (!isTauriRuntime()) {
      settings = {
        ...settings,
        advanced: { ...settings.advanced, turn_off: paused },
      };
      return;
    }
    try {
      const status = await setPaused(paused);
      runtimeStatus = status;
      settings = {
        ...settings,
        advanced: { ...settings.advanced, turn_off: status.paused },
      };
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleIgnoreNext(): Promise<void> {
    bannerError = null;
    if (!isTauriRuntime()) {
      return;
    }
    try {
      runtimeStatus = await setIgnoreNext();
      bannerNotice = 'Next clipboard change will be ignored.';
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleClearUnpinned(): Promise<void> {
    bannerError = null;
    if (!isTauriRuntime()) {
      return;
    }
    try {
      await clearUnpinned();
      await refreshStorageStats();
    } catch (err) {
      bannerError = err instanceof Error ? err.message : String(err);
    }
  }

  function handleWindowKeyDown(e: KeyboardEvent): void {
    if (e.key !== 'Escape' || e.defaultPrevented) {
      return;
    }
    if (activeRecorders > 0 || activeModals > 0) {
      return;
    }
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
      target?.blur();
      return;
    }
    if (isTauriRuntime()) {
      e.preventDefault();
      void getCurrentWindow().close();
    }
  }

  $effect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    systemPrefersDark = mq.matches;
    const onChange = (ev: MediaQueryListEvent) => {
      systemPrefersDark = ev.matches;
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

  $effect(() => {
    if (typeof document === 'undefined') {
      return;
    }
    document.documentElement.classList.toggle('theme-dark', resolvedDark);
    document.documentElement.classList.toggle('theme-light', !resolvedDark);
  });

  $effect(() => {
    if (activeTab === 'snippets') {
      snippetsVisited = true;
    }
  });

  onMount(() => {
    const params =
      typeof window !== 'undefined'
        ? new URLSearchParams(window.location.search)
        : new URLSearchParams();

    const initialTab = normalizeTab(params.get('tab'));
    if (initialTab) {
      activeTab = initialTab;
    }
    const initialAction = params.get('action')?.trim().toLowerCase();

    let unlisteners: Array<() => void> = [];
    let disposed = false;

    void (async () => {
      if (isTauriRuntime()) {
        try {
          const [loadedSettings, status, caps] = await Promise.all([
            getSettings(),
            getRuntimeStatus(),
            getPlatformCapabilities(),
          ]);
          if (!disposed) {
            if (persistSeq === 0) {
              settings = loadedSettings;
            }
            runtimeStatus = status;
            capabilities = caps;
          }
        } catch (err) {
          if (!disposed) {
            bannerError = err instanceof Error ? err.message : String(err);
          }
        }

        if (!disposed) {
          await Promise.all([refreshIgnoreRules(), refreshStorageStats()]);
        }
      }

      if (disposed) {
        return;
      }

      if (initialAction === 'check-updates') {
        void handleCheckUpdates();
      }

      const fns = await Promise.all([
        onSettingsChanged((updated) => {
          if (!disposed && inFlightPersists === 0) {
            settings = updated;
          }
        }),
        onRuntimeStatus((status) => {
          if (!disposed) {
            runtimeStatus = status;
          }
        }),
        onHistoryChanged(() => {
          if (!disposed) {
            void refreshStorageStats();
          }
        }),
        onSettingsTab((tab) => {
          if (disposed) {
            return;
          }
          const target = normalizeTab(tab);
          if (target) {
            activeTab = target;
          }
        }),
        onCheckUpdates(() => {
          if (!disposed) {
            void handleCheckUpdates();
          }
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
      unlisteners.forEach((fn) => fn());
    };
  });
</script>

<svelte:window onkeydown={handleWindowKeyDown} />

<main
  class={`flex h-screen w-screen flex-col overflow-hidden ${resolvedDark ? 'theme-dark' : 'theme-light'}`}
  style:background-color="var(--clipo-surface)"
  style:color="var(--clipo-text)"
>
  <!-- Top Tab Navigation -->
  <header
    class="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2.5"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="flex items-center gap-2">
      <span
        class="inline-flex h-6 w-6 items-center justify-center rounded-md text-[12px] font-bold"
        style:background-color="var(--clipo-accent-soft)"
        style:color="var(--clipo-accent)"
      >
        C
      </span>
      <h1 class="text-[13.5px] font-semibold tracking-tight">Clipo Settings</h1>
    </div>

    <nav aria-label="Settings sections" class="flex items-center gap-1">
      {#each TABS as tab (tab.id)}
        {@const active = activeTab === tab.id}
        <button
          type="button"
          role="tab"
          aria-selected={active}
          onclick={() => {
            activeTab = tab.id;
            bannerError = null;
            bannerNotice = null;
            if (tab.id === 'storage') {
              void refreshStorageStats();
            } else if (tab.id === 'ignore') {
              void refreshIgnoreRules();
            }
          }}
          class="rounded-md px-2.5 py-1 text-[12px] font-medium transition-colors"
          style:background-color={active
            ? 'var(--clipo-accent-soft)'
            : 'transparent'}
          style:color={active
            ? 'var(--clipo-accent)'
            : 'var(--clipo-text-muted)'}
        >
          {tab.label}
          {#if tab.id === 'snippets' && snippetsDirty}
            <span
              class="ml-1 inline-block h-1.5 w-1.5 rounded-full align-middle"
              style:background-color="var(--clipo-accent)"
              title="Unsaved snippet draft"
            ></span>
          {/if}
        </button>
      {/each}
    </nav>
  </header>

  <!-- Inline Status / Error Banners -->
  {#if bannerError}
    <div
      role="alert"
      class="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2 text-[12px]"
      style:background-color="rgba(220, 38, 38, 0.14)"
      style:border-color="rgba(220, 38, 38, 0.35)"
      style:color="var(--clipo-danger)"
    >
      <span class="min-w-0 flex-1 break-words">{bannerError}</span>
      <button
        type="button"
        onclick={() => {
          bannerError = null;
        }}
        class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold hover:opacity-80"
      >
        Dismiss
      </button>
    </div>
  {:else if bannerNotice}
    <div
      class="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2 text-[12px]"
      style:background-color="var(--clipo-accent-soft)"
      style:border-color="var(--clipo-border-subtle)"
      style:color="var(--clipo-accent)"
    >
      <span class="min-w-0 flex-1 break-words">{bannerNotice}</span>
      <button
        type="button"
        onclick={() => {
          bannerNotice = null;
        }}
        class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold hover:opacity-80"
      >
        Dismiss
      </button>
    </div>
  {/if}

  <!-- Active Tab Content -->
  <section class="clipo-scroll min-h-0 flex-1 overflow-y-auto p-5">
    <div class="mx-auto max-w-[720px] h-full">
      {#if activeTab === 'general'}
        <GeneralTab
          {settings}
          {capabilities}
          {updateStatus}
          {checkingUpdates}
          {isMac}
          onUpdateGeneral={updateGeneral}
          onCheckUpdates={() => void handleCheckUpdates()}
          onOpenAccessibility={() => void handleOpenAccessibility()}
          onRecordingChange={handleRecordingChange}
        />
      {:else if activeTab === 'storage'}
        <StorageTab
          {settings}
          stats={storageStats}
          {loadingStats}
          onUpdateStorage={updateStorage}
          onRefreshStats={() => void refreshStorageStats()}
        />
      {:else if activeTab === 'appearance'}
        <AppearanceTab
          {settings}
          onUpdateAppearance={updateAppearance}
        />
      {:else if activeTab === 'ignore'}
        <IgnoreTab
          {settings}
          rules={ignoreRules}
          loading={loadingIgnoreRules}
          onToggleAllowlist={(enabled) =>
            updateAdvanced({ ignore_all_apps_except_listed: enabled })}
          onUpsertRule={handleUpsertIgnoreRule}
          onDeleteRule={handleDeleteIgnoreRule}
        />
      {:else if activeTab === 'advanced'}
        <AdvancedTab
          {settings}
          {runtimeStatus}
          onUpdateAdvanced={updateAdvanced}
          onTogglePaused={handleTogglePaused}
          onIgnoreNext={() => void handleIgnoreNext()}
          onClearUnpinned={handleClearUnpinned}
        />
      {/if}

      {#if snippetsVisited}
        <div class={activeTab === 'snippets' ? 'h-full' : 'hidden'}>
          <SnippetsTab
            onRecordingChange={handleRecordingChange}
            onModalOpenChange={handleModalOpenChange}
            onSnippetsChanged={() => void refreshStorageStats()}
            onDirtyChange={(dirty) => {
              snippetsDirty = dirty;
            }}
          />
        </div>
      {/if}
    </div>
  </section>
</main>
