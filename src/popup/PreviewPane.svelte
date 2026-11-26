<script lang="ts">
  import { buildClipoImgUrl, detectColorSwatch } from '../lib/highlight';
  import {
    detectLanguage,
    tokenizeCode,
    tryPrettyPrintJson,
    type TokenKind,
  } from '../lib/syntax';
  import type { ItemDetailDto, SearchRowDto } from '../lib/types';

  interface Props {
    row: SearchRowDto | null;
    detail: ItemDetailDto | null;
    loading: boolean;
    isMac: boolean;
    onClose: () => void;
  }

  let { row, detail, loading, isMac, onClose }: Props = $props();

  let prettyJson = $state(true);

  let historyItem = $derived(detail?.item ?? row?.history_item ?? null);
  let snippet = $derived(row?.row_kind === 'snippet' ? row.snippet : null);

  let rawText = $derived.by(() => {
    if (snippet) {
      return snippet.content;
    }
    if (detail) {
      return (
        detail.full_text ??
        detail.item.full_text ??
        detail.item.plain_text ??
        detail.item.title_preview
      );
    }
    if (historyItem) {
      return (
        historyItem.full_text ??
        historyItem.plain_text ??
        historyItem.title_preview
      );
    }
    return '';
  });

  let jsonCandidate = $derived(tryPrettyPrintJson(rawText));

  let displayedText = $derived(
    prettyJson && jsonCandidate.isValidJson ? jsonCandidate.formatted : rawText,
  );

  let language = $derived(
    detectLanguage(
      displayedText,
      snippet?.language ?? (jsonCandidate.isValidJson ? 'json' : null),
    ),
  );

  let tokenized = $derived(tokenizeCode(displayedText, language));

  let colorSwatch = $derived.by(() => {
    if (snippet || historyItem?.kind === 'text') {
      return detectColorSwatch(rawText);
    }
    return null;
  });

  let imageUrl = $derived.by(() => {
    if (historyItem?.kind !== 'image') {
      return null;
    }
    return buildClipoImgUrl(historyItem.blob_path ?? historyItem.thumb_path);
  });

  let filePaths = $derived.by(() => {
    if (detail?.file_paths?.length) {
      return detail.file_paths;
    }
    if (historyItem?.kind === 'file' && historyItem.title_preview) {
      return historyItem.title_preview.split('\n').filter(Boolean);
    }
    return [];
  });

  function formatBytes(bytes: number): string {
    if (bytes < 1024) {
      return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
      return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  function formatTimestamp(ts: number): string {
    if (!ts) {
      return '—';
    }
    const date = new Date(ts > 1e12 ? ts : ts * 1000);
    return date.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  function tokenColor(kind: TokenKind): string {
    switch (kind) {
      case 'keyword':
        return 'var(--clipo-syn-keyword)';
      case 'string':
        return 'var(--clipo-syn-string)';
      case 'number':
        return 'var(--clipo-syn-number)';
      case 'comment':
        return 'var(--clipo-syn-comment)';
      case 'key':
        return 'var(--clipo-syn-key)';
      case 'boolean':
        return 'var(--clipo-syn-boolean)';
      case 'variable':
        return 'var(--clipo-syn-variable)';
      case 'operator':
        return 'var(--clipo-syn-operator)';
      default:
        return 'var(--clipo-text)';
    }
  }

  let kindBadge = $derived.by(() => {
    if (snippet) {
      return snippet.language ? `SNIPPET · ${snippet.language.toUpperCase()}` : 'SNIPPET';
    }
    if (historyItem?.kind === 'image') {
      return 'IMAGE';
    }
    if (historyItem?.kind === 'file') {
      return 'FILES';
    }
    if (colorSwatch) {
      return 'COLOR';
    }
    if (language !== 'plaintext') {
      return language.toUpperCase();
    }
    return 'TEXT';
  });
</script>

<aside
  aria-label="Item preview"
  class="flex h-full min-h-0 flex-1 flex-col border-l"
  style:background-color="var(--clipo-surface-elevated)"
  style:border-color="var(--clipo-border-subtle)"
>
  <div
    class="flex items-center justify-between gap-1 border-b px-2.5 py-1.5"
    style:border-color="var(--clipo-border-subtle)"
  >
    <span
      class="truncate rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold"
      style:background-color="var(--clipo-accent-soft)"
      style:color="var(--clipo-accent)"
    >
      {kindBadge}
    </span>

    <div class="flex items-center gap-1">
      {#if jsonCandidate.isValidJson}
        <button
          type="button"
          onclick={() => {
            prettyJson = !prettyJson;
          }}
          class="rounded px-1.5 py-0.5 text-[10px] font-medium transition-opacity hover:opacity-80"
          style:background-color={prettyJson ? 'var(--clipo-accent-soft)' : 'var(--clipo-badge-bg)'}
          style:color={prettyJson ? 'var(--clipo-accent)' : 'var(--clipo-text-muted)'}
        >
          {prettyJson ? 'Pretty' : 'Raw'}
        </button>
      {/if}

      <button
        type="button"
        onclick={onClose}
        title="Close preview"
        class="inline-flex h-5 w-5 items-center justify-center rounded text-[12px] hover:opacity-80"
        style:color="var(--clipo-text-muted)"
      >
        ×
      </button>
    </div>
  </div>

  {#if !row}
    <div
      class="flex flex-1 items-center justify-center p-4 text-center text-[11.5px]"
      style:color="var(--clipo-text-faint)"
    >
      Select an item to preview
    </div>
  {:else}
    <div class="clipo-scroll min-h-0 flex-1 overflow-y-auto p-2.5">
      {#if loading}
        <div class="mb-2 text-[10.5px]" style:color="var(--clipo-text-faint)">
          Loading full payload…
        </div>
      {/if}

      {#if colorSwatch}
        <div
          class="mb-2.5 rounded-lg border p-2"
          style:border-color="var(--clipo-border)"
          style:background-color="var(--clipo-surface)"
        >
          <div
            class="mb-2 h-14 w-full rounded-md border shadow-inner"
            style:background-color={colorSwatch.cssColor}
            style:border-color="var(--clipo-border)"
          ></div>
          <dl class="space-y-1 font-mono text-[10.5px]">
            <div class="flex justify-between gap-2">
              <dt style:color="var(--clipo-text-faint)">HEX</dt>
              <dd class="select-text font-semibold">{colorSwatch.hex}</dd>
            </div>
            <div class="flex justify-between gap-2">
              <dt style:color="var(--clipo-text-faint)">RGB</dt>
              <dd class="select-text">{colorSwatch.rgb}</dd>
            </div>
            <div class="flex justify-between gap-2">
              <dt style:color="var(--clipo-text-faint)">HSL</dt>
              <dd class="select-text">{colorSwatch.hsl}</dd>
            </div>
          </dl>
        </div>
      {:else if imageUrl}
        <div class="flex flex-col items-center gap-2">
          <div
            class="flex max-h-52 w-full items-center justify-center overflow-hidden rounded-lg border p-1.5"
            style:border-color="var(--clipo-border)"
            style:background-color="var(--clipo-surface-muted)"
          >
            <img
              src={imageUrl}
              alt={historyItem?.title_preview ?? 'Image preview'}
              class="max-h-48 max-w-full rounded object-contain"
            />
          </div>
          {#if historyItem?.image_width && historyItem?.image_height}
            <div class="font-mono text-[10.5px]" style:color="var(--clipo-text-muted)">
              {historyItem.image_width} × {historyItem.image_height} px · {formatBytes(historyItem.byte_size)}
            </div>
          {/if}
        </div>
      {:else if historyItem?.kind === 'file'}
        <div class="space-y-1">
          {#each filePaths as path, idx (idx)}
            <div
              class="select-text break-all rounded border px-2 py-1 font-mono text-[10.5px]"
              style:border-color="var(--clipo-border-subtle)"
              style:background-color="var(--clipo-surface)"
            >
              {path}
            </div>
          {/each}
        </div>
      {:else}
        {#if snippet}
          <div class="mb-2 space-y-1">
            <div class="text-[12px] font-semibold">{snippet.title}</div>
            {#if snippet.tags.length > 0}
              <div class="flex flex-wrap gap-1">
                {#each snippet.tags as tag (tag)}
                  <span
                    class="rounded px-1.5 py-0.5 text-[9.5px]"
                    style:background-color="var(--clipo-badge-bg)"
                    style:color="var(--clipo-text-muted)"
                  >
                    #{tag}
                  </span>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        <pre
          class="select-text whitespace-pre-wrap break-words rounded-lg border p-2 font-mono text-[11px] leading-relaxed"
          style:border-color="var(--clipo-border-subtle)"
          style:background-color="var(--clipo-surface)"
        ><code>{#each tokenized.tokens as tok, idx (idx)}<span style:color={tokenColor(tok.kind)}>{tok.text}</span>{/each}</code></pre>
      {/if}
    </div>

    <!-- Metadata & Shortcut Legend -->
    <div
      class="space-y-1.5 border-t px-2.5 py-2 text-[10px]"
      style:border-color="var(--clipo-border-subtle)"
      style:color="var(--clipo-text-muted)"
    >
      {#if historyItem}
        <div class="grid grid-cols-2 gap-x-2 gap-y-0.5">
          <div class="truncate">
            <span style:color="var(--clipo-text-faint)">App:</span>
            {historyItem.source_app_name ?? 'Unknown'}
          </div>
          <div class="text-right tabular-nums">
            <span style:color="var(--clipo-text-faint)">Copied:</span>
            {historyItem.copy_count}×
          </div>
          <div class="truncate">
            <span style:color="var(--clipo-text-faint)">Last:</span>
            {formatTimestamp(historyItem.last_copied_at)}
          </div>
          <div class="text-right tabular-nums">
            <span style:color="var(--clipo-text-faint)">Size:</span>
            {formatBytes(historyItem.byte_size)}
          </div>
        </div>
      {:else if snippet}
        <div class="flex items-center justify-between">
          <span>Used {snippet.use_count}×</span>
          <span>Updated {formatTimestamp(snippet.updated_at)}</span>
        </div>
      {/if}

      <div
        class="flex flex-wrap gap-x-2 gap-y-0.5 border-t pt-1 font-mono text-[9.5px]"
        style:border-color="var(--clipo-border-subtle)"
        style:color="var(--clipo-text-faint)"
      >
        <span>↵ Paste</span>
        <span>{isMac ? '⌥↵' : 'Alt+↵'} Copy</span>
        <span>{isMac ? '⌘⇧↵' : 'Ctrl+⇧↵'} Plain</span>
        <span>{isMac ? '⌥P' : 'Alt+P'} Pin</span>
        <span>{isMac ? '⌥S' : 'Alt+S'} Snippet</span>
      </div>
    </div>
  {/if}
</aside>
