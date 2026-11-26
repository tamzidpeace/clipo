<script lang="ts">
  import { tick } from 'svelte';
  import { saveItemAsSnippet, upsertSnippet } from '../lib/ipc';
  import { detectLanguage } from '../lib/syntax';
  import type { HistoryItemDto, SnippetDto } from '../lib/types';

  interface Props {
    open: boolean;
    sourceItem: HistoryItemDto | null;
    sourceText: string;
    onClose: () => void;
    onSaved: (snippet: SnippetDto) => void;
  }

  let { open, sourceItem, sourceText, onClose, onSaved }: Props = $props();

  let title = $state('');
  let content = $state('');
  let tagsRaw = $state('');
  let language = $state('auto');
  let shortcutKey = $state('');
  let isPinned = $state(false);
  let saving = $state(false);
  let errorMsg = $state<string | null>(null);
  let titleInputEl: HTMLInputElement | null = $state(null);

  let isFromHistory = $derived(sourceItem !== null && sourceItem.kind !== 'image');

  $effect(() => {
    if (!open) {
      return;
    }
    errorMsg = null;
    saving = false;
    if (sourceItem && sourceItem.kind !== 'image') {
      const base = sourceText || sourceItem.title_preview || '';
      const firstLine = base.split('\n')[0]?.trim() ?? '';
      title = firstLine.slice(0, 48) || 'Saved Snippet';
      content = base;
      const detected = detectLanguage(base);
      language = detected === 'plaintext' ? 'auto' : detected;
    } else {
      title = '';
      content = '';
      language = 'auto';
    }
    tagsRaw = '';
    shortcutKey = '';
    isPinned = false;

    void tick().then(() => {
      titleInputEl?.focus();
      titleInputEl?.select();
    });
  });

  let variablePreview = $derived.by(() => {
    const sample = isFromHistory ? sourceText : content;
    const now = new Date();
    const yyyy = now.getFullYear();
    const mm = String(now.getMonth() + 1).padStart(2, '0');
    const dd = String(now.getDate()).padStart(2, '0');
    return sample
      .replace(/\{\{date\}\}/g, `${yyyy}-${mm}-${dd}`)
      .replace(/\{\{uuid\}\}/g, '550e8400-e29b-41d4-a716-446655440000')
      .replace(/\{\{clipboard\}\}/g, '<latest clipboard>')
      .replace(/\{\{cursor\}\}/g, '|');
  });

  function hasTemplateVariables(text: string): boolean {
    return /\{\{(date|clipboard|uuid|cursor)\}\}/.test(text);
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      errorMsg = 'Please enter a snippet title.';
      return;
    }

    const tags = tagsRaw
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
    const normalizedLang = language === 'auto' ? null : language;
    const normalizedShortcut = shortcutKey.trim() || null;

    saving = true;
    errorMsg = null;

    try {
      let saved: SnippetDto;
      if (isFromHistory && sourceItem) {
        saved = await saveItemAsSnippet({
          itemId: sourceItem.id,
          title: trimmedTitle,
          tags,
          language: normalizedLang,
          shortcutKey: normalizedShortcut,
          isPinned,
        });
      } else {
        if (!content.trim()) {
          errorMsg = 'Snippet content cannot be empty.';
          saving = false;
          return;
        }
        saved = await upsertSnippet(null, {
          title: trimmedTitle,
          content,
          tags,
          language: normalizedLang,
          shortcut_key: normalizedShortcut,
          is_pinned: isPinned,
        });
      }
      onSaved(saved);
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

{#if open}
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby="quick-snippet-title"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-3 backdrop-blur-xs"
  >
    <form
      onsubmit={handleSubmit}
      class="flex max-h-full w-full max-w-[390px] flex-col gap-2.5 overflow-y-auto rounded-xl border p-3.5 shadow-2xl"
      style:background-color="var(--clipo-surface-elevated)"
      style:border-color="var(--clipo-border)"
      style:color="var(--clipo-text)"
    >
      <div class="flex items-center justify-between">
        <h2 id="quick-snippet-title" class="text-[13px] font-semibold">
          {isFromHistory ? 'Save Selection as Snippet' : 'New Snippet'}
        </h2>
        <button
          type="button"
          onclick={onClose}
          class="rounded px-1.5 py-0.5 text-[12px] hover:opacity-80"
          style:color="var(--clipo-text-muted)"
        >
          ×
        </button>
      </div>

      {#if errorMsg}
        <div
          class="rounded-md px-2 py-1 text-[11px]"
          style:background-color="rgba(220, 38, 38, 0.14)"
          style:color="var(--clipo-danger)"
        >
          {errorMsg}
        </div>
      {/if}

      <label class="flex flex-col gap-1 text-[11px]">
        <span class="font-medium" style:color="var(--clipo-text-muted)">Title</span>
        <input
          bind:this={titleInputEl}
          bind:value={title}
          type="text"
          placeholder="e.g. Deploy production script"
          class="rounded-md border px-2 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        />
      </label>

      <label class="flex flex-col gap-1 text-[11px]">
        <span class="font-medium" style:color="var(--clipo-text-muted)">
          {isFromHistory ? 'Captured Content' : 'Content'}
        </span>
        {#if isFromHistory}
          <pre
            class="max-h-24 overflow-y-auto whitespace-pre-wrap break-words rounded-md border p-2 font-mono text-[10.5px]"
            style:background-color="var(--clipo-surface-muted)"
            style:border-color="var(--clipo-border-subtle)"
          >{sourceText}</pre>
        {:else}
          <textarea
            bind:value={content}
            rows="4"
            placeholder="Paste or type snippet text… supports {{date}}, {{clipboard}}, {{uuid}}, {{cursor}}"
            class="resize-y rounded-md border p-2 font-mono text-[11px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          ></textarea>
        {/if}
      </label>

      <div class="grid grid-cols-2 gap-2">
        <label class="flex flex-col gap-1 text-[11px]">
          <span class="font-medium" style:color="var(--clipo-text-muted)">
            Tags (comma-separated)
          </span>
          <input
            bind:value={tagsRaw}
            type="text"
            placeholder="cli, work"
            class="rounded-md border px-2 py-1 text-[11.5px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          />
        </label>

        <label class="flex flex-col gap-1 text-[11px]">
          <span class="font-medium" style:color="var(--clipo-text-muted)">Language</span>
          <select
            bind:value={language}
            class="rounded-md border px-2 py-1 text-[11.5px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            <option value="auto">Auto</option>
            <option value="plaintext">Plain Text</option>
            <option value="json">JSON</option>
            <option value="bash">Bash</option>
            <option value="sql">SQL</option>
            <option value="typescript">TypeScript / JS</option>
          </select>
        </label>
      </div>

      <div class="flex items-end justify-between gap-2">
        <label class="flex flex-1 flex-col gap-1 text-[11px]">
          <span class="font-medium" style:color="var(--clipo-text-muted)">
            In-popup Shortcut
          </span>
          <input
            bind:value={shortcutKey}
            type="text"
            placeholder="Optional (e.g. Alt+1)"
            class="rounded-md border px-2 py-1 font-mono text-[11px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          />
        </label>

        <label class="flex cursor-pointer items-center gap-1.5 pb-1.5 text-[11.5px]">
          <input type="checkbox" bind:checked={isPinned} />
          <span>Pin snippet</span>
        </label>
      </div>

      <div
        class="rounded-md border px-2 py-1.5 text-[10px]"
        style:background-color="var(--clipo-surface-muted)"
        style:border-color="var(--clipo-border-subtle)"
        style:color="var(--clipo-text-muted)"
      >
        <div class="font-medium">
          Variables:
          <code class="font-mono">{'{{date}}'}</code>,
          <code class="font-mono">{'{{clipboard}}'}</code>,
          <code class="font-mono">{'{{uuid}}'}</code>,
          <code class="font-mono">{'{{cursor}}'}</code>
        </div>
        {#if hasTemplateVariables(isFromHistory ? sourceText : content)}
          <div class="mt-1 truncate font-mono text-[10px]" style:color="var(--clipo-text)">
            Preview: {variablePreview}
          </div>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 pt-0.5">
        <button
          type="button"
          onclick={onClose}
          class="rounded-md border px-2.5 py-1 text-[11.5px] font-medium hover:opacity-85"
          style:border-color="var(--clipo-border)"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={saving}
          class="rounded-md px-3 py-1 text-[11.5px] font-semibold text-white hover:opacity-90 disabled:opacity-50"
          style:background-color="var(--clipo-accent)"
        >
          {saving ? 'Saving…' : 'Save Snippet'}
        </button>
      </div>
    </form>
  </div>
{/if}
