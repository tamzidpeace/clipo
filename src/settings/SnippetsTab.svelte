<script lang="ts">
  import { onMount } from 'svelte';
  import { detectHotkeyPlatform, formatHotkey } from '../lib/hotkey';
  import {
    deleteSnippet,
    exportSnippets,
    importSnippets,
    isTauriRuntime,
    listSnippets,
    upsertSnippet,
  } from '../lib/ipc';
  import { detectLanguage } from '../lib/syntax';
  import type { SnippetDto } from '../lib/types';
  import HotkeyRecorder from './HotkeyRecorder.svelte';

  interface Props {
    onRecordingChange: (recording: boolean) => void;
    onModalOpenChange: (open: boolean) => void;
    onSnippetsChanged?: () => void;
  }

  let { onRecordingChange, onModalOpenChange, onSnippetsChanged }: Props =
    $props();

  const platform = detectHotkeyPlatform();

  const LANGUAGE_OPTIONS = [
    { value: 'auto', label: 'Auto-detect' },
    { value: 'json', label: 'JSON' },
    { value: 'typescript', label: 'TypeScript' },
    { value: 'javascript', label: 'JavaScript' },
    { value: 'rust', label: 'Rust' },
    { value: 'python', label: 'Python' },
    { value: 'sql', label: 'SQL' },
    { value: 'html', label: 'HTML' },
    { value: 'css', label: 'CSS' },
    { value: 'shell', label: 'Shell' },
    { value: 'plaintext', label: 'Plain Text' },
  ];

  let snippets = $state<SnippetDto[]>([]);
  let loading = $state(false);
  let searchQuery = $state('');
  let selectedId = $state<number | null>(null);

  let title = $state('');
  let content = $state('');
  let tagsRaw = $state('');
  let language = $state('auto');
  let shortcutKey = $state('');
  let isPinned = $state(false);

  let saving = $state(false);
  let deleting = $state(false);
  let statusMsg = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  let exportedJson = $state<string | null>(null);
  let copiedExport = $state(false);
  let importModalOpen = $state(false);
  let importRawJson = $state('');
  let importError = $state<string | null>(null);
  let importing = $state(false);
  let importTextareaEl: HTMLTextAreaElement | null = $state(null);

  let filteredSnippets = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) {
      return snippets;
    }
    return snippets.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.content.toLowerCase().includes(q) ||
        s.tags.some((t) => t.toLowerCase().includes(q)),
    );
  });

  let detectedLangHint = $derived(
    language === 'auto' ? detectLanguage(content, null) : language,
  );

  let variablePreview = $derived.by(() => {
    const now = new Date();
    const yyyy = now.getFullYear();
    const mm = String(now.getMonth() + 1).padStart(2, '0');
    const dd = String(now.getDate()).padStart(2, '0');
    const hh = String(now.getHours()).padStart(2, '0');
    const min = String(now.getMinutes()).padStart(2, '0');
    const sec = String(now.getSeconds()).padStart(2, '0');

    return content
      .replace(/\{\{date\}\}/g, `${yyyy}-${mm}-${dd}`)
      .replace(/\{\{time\}\}/g, `${hh}:${min}:${sec}`)
      .replace(/\{\{datetime\}\}/g, `${yyyy}-${mm}-${dd} ${hh}:${min}:${sec}`)
      .replace(/\{\{uuid\}\}/g, '550e8400-e29b-41d4-a716-446655440000')
      .replace(/\{\{clipboard\}\}/g, '<latest clipboard>')
      .replace(/\{\{cursor\}\}/g, '|');
  });

  let hasVariables = $derived(
    /\{\{(date|time|datetime|clipboard|uuid|cursor)\}\}/.test(content),
  );

  function loadSnippetIntoEditor(snippet: SnippetDto): void {
    selectedId = snippet.id;
    title = snippet.title;
    content = snippet.content;
    tagsRaw = snippet.tags.join(', ');
    language = snippet.language ?? 'auto';
    shortcutKey = snippet.shortcut_key ?? '';
    isPinned = snippet.is_pinned;
    errorMsg = null;
    statusMsg = null;
  }

  function startNewSnippet(): void {
    selectedId = null;
    title = '';
    content = '';
    tagsRaw = '';
    language = 'auto';
    shortcutKey = '';
    isPinned = false;
    errorMsg = null;
    statusMsg = null;
  }

  export async function refreshSnippets(selectTargetId?: number | null): Promise<void> {
    if (!isTauriRuntime()) {
      return;
    }
    loading = true;
    try {
      const list = await listSnippets();
      snippets = list;
      if (selectTargetId !== undefined) {
        const match =
          selectTargetId === null
            ? null
            : list.find((s) => s.id === selectTargetId) ?? null;
        if (match) {
          loadSnippetIntoEditor(match);
        } else if (list.length > 0 && selectedId === null) {
          loadSnippetIntoEditor(list[0]);
        }
      } else if (selectedId !== null) {
        const current = list.find((s) => s.id === selectedId);
        if (current) {
          loadSnippetIntoEditor(current);
        } else if (list.length > 0) {
          loadSnippetIntoEditor(list[0]);
        } else {
          startNewSnippet();
        }
      } else if (list.length > 0) {
        loadSnippetIntoEditor(list[0]);
      }
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refreshSnippets();
  });

  async function handleSave(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      errorMsg = 'Please enter a snippet title.';
      return;
    }
    if (!content.trim()) {
      errorMsg = 'Snippet content cannot be empty.';
      return;
    }

    const tags = tagsRaw
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);

    saving = true;
    errorMsg = null;
    statusMsg = null;

    try {
      const saved = await upsertSnippet(selectedId, {
        title: trimmedTitle,
        content,
        tags,
        language: language === 'auto' ? null : language,
        shortcut_key: shortcutKey.trim() || null,
        is_pinned: isPinned,
      });
      await refreshSnippets(saved.id);
      statusMsg = selectedId === null ? 'Snippet created.' : 'Snippet saved.';
      onSnippetsChanged?.();
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function handleDelete(): Promise<void> {
    if (selectedId === null) {
      return;
    }
    deleting = true;
    errorMsg = null;
    statusMsg = null;
    try {
      await deleteSnippet(selectedId);
      selectedId = null;
      await refreshSnippets();
      if (snippets.length === 0) {
        startNewSnippet();
      }
      statusMsg = 'Snippet deleted.';
      onSnippetsChanged?.();
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    } finally {
      deleting = false;
    }
  }

  async function handleExportJson(): Promise<void> {
    errorMsg = null;
    statusMsg = null;
    copiedExport = false;
    try {
      const raw = await exportSnippets();
      exportedJson = raw;
      const blob = new Blob([raw], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = 'clipo-snippets.json';
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      statusMsg = 'Exported snippets to clipo-snippets.json.';
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    }
  }

  async function copyExportedJson(): Promise<void> {
    if (!exportedJson) {
      return;
    }
    try {
      await navigator.clipboard.writeText(exportedJson);
      copiedExport = true;
    } catch {
      // Clipboard write unavailable in headless mode
    }
  }

  function openImportModal(): void {
    importRawJson = '';
    importError = null;
    importModalOpen = true;
    onModalOpenChange(true);
    queueMicrotask(() => importTextareaEl?.focus());
  }

  function closeImportModal(): void {
    importModalOpen = false;
    importError = null;
    onModalOpenChange(false);
  }

  async function handleImportFile(e: Event): Promise<void> {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) {
      return;
    }
    try {
      importRawJson = await file.text();
      importError = null;
    } catch (err) {
      importError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleImportSubmit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (!importRawJson.trim()) {
      importError = 'Paste JSON or select a snippets JSON file to import.';
      return;
    }
    importing = true;
    importError = null;
    try {
      const created = await importSnippets(importRawJson);
      closeImportModal();
      const firstId = created[0]?.id ?? null;
      await refreshSnippets(firstId ?? undefined);
      statusMsg = `Imported ${created.length} snippet${created.length === 1 ? '' : 's'}.`;
      onSnippetsChanged?.();
    } catch (err) {
      importError = err instanceof Error ? err.message : String(err);
    } finally {
      importing = false;
    }
  }
</script>

<div class="grid h-full min-h-[420px] grid-cols-[240px_1fr] gap-4">
  <!-- Left: Snippet List + Import/Export -->
  <aside
    class="flex min-h-0 flex-col rounded-xl border"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="flex flex-col gap-2 border-b p-2.5" style:border-color="var(--clipo-border-subtle)">
      <div class="flex items-center justify-between gap-2">
        <span class="text-[12px] font-semibold">Snippets ({snippets.length})</span>
        <button
          type="button"
          onclick={startNewSnippet}
          class="rounded-md px-2 py-1 text-[11px] font-semibold text-white hover:opacity-90"
          style:background-color="var(--clipo-accent)"
        >
          + New Snippet
        </button>
      </div>

      <input
        type="search"
        bind:value={searchQuery}
        placeholder="Filter snippets…"
        class="w-full rounded-md border px-2 py-1 text-[11.5px] outline-none"
        style:background-color="var(--clipo-surface)"
        style:border-color="var(--clipo-border)"
      />
    </div>

    <div class="clipo-scroll flex-1 overflow-y-auto p-1.5">
      {#if loading}
        <div class="p-4 text-center text-[11.5px]" style:color="var(--clipo-text-muted)">
          Loading snippets…
        </div>
      {:else if filteredSnippets.length === 0}
        <div class="p-4 text-center text-[11.5px]" style:color="var(--clipo-text-muted)">
          {snippets.length === 0
            ? 'No snippets yet. Create one to get started.'
            : 'No snippets match your search.'}
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          {#each filteredSnippets as snippet (snippet.id)}
            {@const active = selectedId === snippet.id}
            <button
              type="button"
              onclick={() => loadSnippetIntoEditor(snippet)}
              class="flex w-full flex-col gap-1 rounded-lg px-2.5 py-2 text-left transition-colors"
              style:background-color={active
                ? 'var(--clipo-accent-soft)'
                : 'transparent'}
              style:color="var(--clipo-text)"
            >
              <div class="flex items-center justify-between gap-1.5">
                <span class="truncate text-[12px] font-medium">
                  {#if snippet.is_pinned}
                    <span style:color="var(--clipo-accent)">★ </span>
                  {/if}
                  {snippet.title}
                </span>
                {#if snippet.shortcut_key}
                  <span
                    class="shrink-0 rounded px-1 py-0.5 font-mono text-[10px]"
                    style:background-color="var(--clipo-badge-bg)"
                    style:color="var(--clipo-badge-text)"
                  >
                    {formatHotkey(snippet.shortcut_key, platform)}
                  </span>
                {/if}
              </div>

              <div class="flex items-center justify-between gap-2 text-[10px]" style:color="var(--clipo-text-faint)">
                <span class="truncate">
                  {snippet.tags.length > 0 ? snippet.tags.join(', ') : 'No tags'}
                </span>
                <span class="shrink-0 uppercase">
                  {snippet.language ?? 'auto'}
                </span>
              </div>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div
      class="flex items-center justify-between gap-1.5 border-t p-2"
      style:border-color="var(--clipo-border-subtle)"
    >
      <button
        type="button"
        onclick={() => void handleExportJson()}
        class="flex-1 rounded-md border px-2 py-1 text-[11px] font-medium hover:opacity-85"
        style:background-color="var(--clipo-surface)"
        style:border-color="var(--clipo-border)"
      >
        Export JSON
      </button>
      <button
        type="button"
        onclick={openImportModal}
        class="flex-1 rounded-md border px-2 py-1 text-[11px] font-medium hover:opacity-85"
        style:background-color="var(--clipo-surface)"
        style:border-color="var(--clipo-border)"
      >
        Import JSON
      </button>
    </div>
  </aside>

  <!-- Right: Snippet Editor -->
  <form
    onsubmit={handleSave}
    class="flex min-h-0 flex-col gap-3 rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-[13px] font-semibold">
        {selectedId === null ? 'New Snippet' : 'Edit Snippet'}
      </h2>

      <div class="flex items-center gap-2">
        {#if exportedJson}
          <button
            type="button"
            onclick={() => void copyExportedJson()}
            class="rounded-md border px-2 py-1 text-[11px] font-medium hover:opacity-85"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            {copiedExport ? 'Copied JSON' : 'Copy Exported JSON'}
          </button>
        {/if}

        {#if selectedId !== null}
          <button
            type="button"
            onclick={() => void handleDelete()}
            disabled={deleting}
            class="rounded-md border px-2.5 py-1 text-[11.5px] font-medium hover:opacity-85 disabled:opacity-50"
            style:border-color="var(--clipo-border)"
            style:color="var(--clipo-danger)"
          >
            {deleting ? 'Deleting…' : 'Delete'}
          </button>
        {/if}

        <button
          type="submit"
          disabled={saving}
          class="rounded-md px-3 py-1 text-[12px] font-semibold text-white hover:opacity-90 disabled:opacity-50"
          style:background-color="var(--clipo-accent)"
        >
          {saving ? 'Saving…' : 'Save Snippet'}
        </button>
      </div>
    </div>

    {#if errorMsg}
      <div
        role="alert"
        class="rounded-md px-2.5 py-1.5 text-[11.5px]"
        style:background-color="rgba(220, 38, 38, 0.12)"
        style:color="var(--clipo-danger)"
      >
        {errorMsg}
      </div>
    {/if}

    {#if statusMsg}
      <div
        class="rounded-md px-2.5 py-1.5 text-[11.5px]"
        style:background-color="var(--clipo-accent-soft)"
        style:color="var(--clipo-accent)"
      >
        {statusMsg}
      </div>
    {/if}

    <div class="grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="text-[11.5px] font-medium" style:color="var(--clipo-text-muted)">
          Title
        </span>
        <input
          type="text"
          bind:value={title}
          placeholder="e.g. SQL recent orders"
          class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-[11.5px] font-medium" style:color="var(--clipo-text-muted)">
          Tags (comma-separated)
        </span>
        <input
          type="text"
          bind:value={tagsRaw}
          placeholder="work, sql, template"
          class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        />
      </label>
    </div>

    <div class="flex flex-wrap items-center justify-between gap-3">
      <label class="flex items-center gap-2">
        <span class="text-[11.5px] font-medium" style:color="var(--clipo-text-muted)">
          Language
        </span>
        <select
          bind:value={language}
          class="rounded-md border px-2 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          {#each LANGUAGE_OPTIONS as opt (opt.value)}
            <option value={opt.value}>{opt.label}</option>
          {/each}
          {#if language !== 'auto' && !LANGUAGE_OPTIONS.some((o) => o.value === language)}
            <option value={language}>{language}</option>
          {/if}
        </select>
        {#if language === 'auto' && content.trim()}
          <span
            class="rounded px-1.5 py-0.5 font-mono text-[10px] uppercase"
            style:background-color="var(--clipo-badge-bg)"
            style:color="var(--clipo-badge-text)"
          >
            {detectedLangHint}
          </span>
        {/if}
      </label>

      <div class="flex items-center gap-3">
        <label class="flex cursor-pointer items-center gap-1.5 text-[12px]">
          <input
            type="checkbox"
            bind:checked={isPinned}
            class="h-4 w-4 accent-[var(--clipo-accent)]"
          />
          <span>Pin to top</span>
        </label>

        <div class="flex items-center gap-1.5">
          <span class="text-[11.5px] font-medium" style:color="var(--clipo-text-muted)">
            Hotkey
          </span>
          <HotkeyRecorder
            value={shortcutKey}
            requireModifier={false}
            allowEmpty={true}
            placeholder="Optional"
            ariaLabel="In-popup snippet shortcut"
            onChange={(next) => {
              shortcutKey = next;
            }}
            {onRecordingChange}
          />
        </div>
      </div>
    </div>

    <label class="flex min-h-0 flex-1 flex-col gap-1">
      <span class="text-[11.5px] font-medium" style:color="var(--clipo-text-muted)">
        Content
      </span>
      <textarea
        bind:value={content}
        rows="6"
        placeholder="Snippet template text… supports {{date}}, {{uuid}}, {{clipboard}}, {{cursor}}"
        class="clipo-scroll min-h-[120px] flex-1 resize-y rounded-lg border p-2.5 font-mono text-[12px] leading-relaxed outline-none"
        style:background-color="var(--clipo-surface)"
        style:border-color="var(--clipo-border)"
      ></textarea>
    </label>

    <!-- Live variable preview -->
    <div
      class="rounded-lg border p-2.5 text-[11px]"
      style:background-color="var(--clipo-surface-muted)"
      style:border-color="var(--clipo-border-subtle)"
    >
      <div class="flex flex-wrap items-center justify-between gap-2" style:color="var(--clipo-text-muted)">
        <span class="font-medium">
          Variables:
          <code class="font-mono">{'{{date}}'}</code>,
          <code class="font-mono">{'{{uuid}}'}</code>,
          <code class="font-mono">{'{{clipboard}}'}</code>,
          <code class="font-mono">{'{{cursor}}'}</code>
        </span>
      </div>
      {#if hasVariables}
        <pre
          class="clipo-scroll mt-1.5 max-h-20 overflow-y-auto whitespace-pre-wrap break-words rounded border p-2 font-mono text-[11px]"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border-subtle)"
        >{variablePreview}</pre>
      {/if}
    </div>
  </form>
</div>

{#if importModalOpen}
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby="import-snippets-title"
    tabindex="-1"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 backdrop-blur-xs"
    onkeydown={(e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        closeImportModal();
      }
    }}
  >
    <form
      onsubmit={handleImportSubmit}
      class="flex w-full max-w-[460px] flex-col gap-3 rounded-xl border p-4 shadow-2xl"
      style:background-color="var(--clipo-surface-elevated)"
      style:border-color="var(--clipo-border)"
      style:color="var(--clipo-text)"
    >
      <div class="flex items-center justify-between">
        <h3 id="import-snippets-title" class="text-[13px] font-semibold">
          Import Snippets JSON
        </h3>
        <button
          type="button"
          onclick={closeImportModal}
          class="rounded px-1.5 py-0.5 text-[13px] hover:opacity-80"
          style:color="var(--clipo-text-muted)"
        >
          ×
        </button>
      </div>

      <label class="flex flex-col gap-1 text-[11.5px]">
        <span style:color="var(--clipo-text-muted)">Choose JSON file</span>
        <input
          type="file"
          accept=".json,application/json"
          onchange={(e) => void handleImportFile(e)}
          class="text-[11.5px]"
        />
      </label>

      <label class="flex flex-col gap-1 text-[11.5px]">
        <span style:color="var(--clipo-text-muted)">Or paste JSON payload</span>
        <textarea
          bind:this={importTextareaEl}
          bind:value={importRawJson}
          rows="7"
          placeholder={'{"version": 1, "snippets": [...]}'}
          class="clipo-scroll resize-y rounded-lg border p-2 font-mono text-[11px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        ></textarea>
      </label>

      {#if importError}
        <div
          role="alert"
          class="rounded-md px-2.5 py-1.5 text-[11px]"
          style:background-color="rgba(220, 38, 38, 0.12)"
          style:color="var(--clipo-danger)"
        >
          {importError}
        </div>
      {/if}

      <div class="flex items-center justify-end gap-2">
        <button
          type="button"
          onclick={closeImportModal}
          class="rounded-md border px-3 py-1.5 text-[12px] font-medium hover:opacity-85"
          style:border-color="var(--clipo-border)"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={importing}
          class="rounded-md px-3 py-1.5 text-[12px] font-semibold text-white hover:opacity-90 disabled:opacity-50"
          style:background-color="var(--clipo-accent)"
        >
          {importing ? 'Importing…' : 'Import'}
        </button>
      </div>
    </form>
  </div>
{/if}
