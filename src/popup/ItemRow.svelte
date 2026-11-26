<script lang="ts">
  import {
    buildClipoImgUrl,
    buildDisplaySegments,
    detectColorSwatch,
  } from '../lib/highlight';
  import type { MatchHighlightStyle, SearchRowDto } from '../lib/types';

  interface Props {
    row: SearchRowDto;
    selected: boolean;
    highlightStyle: MatchHighlightStyle;
    showSpecialSymbols: boolean;
    showApplicationIcons: boolean;
    showHexColorSwatch: boolean;
    imageHeight: number;
    isMac: boolean;
    onHover: () => void;
    onActivate: (e: MouseEvent) => void;
  }

  let {
    row,
    selected,
    highlightStyle,
    showSpecialSymbols,
    showApplicationIcons,
    showHexColorSwatch,
    imageHeight,
    isMac,
    onHover,
    onActivate,
  }: Props = $props();

  let displayText = $derived.by(() => {
    if (row.row_kind === 'snippet' && row.snippet) {
      return row.snippet.title || row.snippet.content;
    }
    if (row.history_item) {
      return row.history_item.title_preview || row.history_item.plain_text || '';
    }
    return '';
  });

  let segments = $derived(
    buildDisplaySegments(
      displayText,
      row.match_ranges,
      highlightStyle,
      showSpecialSymbols,
    ),
  );

  let colorSwatch = $derived.by(() => {
    if (!showHexColorSwatch || row.row_kind !== 'history' || row.history_item?.kind !== 'text') {
      return null;
    }
    const candidate = row.history_item.plain_text ?? row.history_item.title_preview;
    return detectColorSwatch(candidate);
  });

  let thumbUrl = $derived.by(() => {
    if (row.row_kind !== 'history' || row.history_item?.kind !== 'image') {
      return null;
    }
    return buildClipoImgUrl(
      row.history_item.thumb_path ?? row.history_item.blob_path,
    );
  });

  let clampedThumbHeight = $derived(Math.min(Math.max(22, imageHeight), 60));

  let isPinned = $derived(
    row.row_kind === 'snippet'
      ? Boolean(row.snippet?.is_pinned)
      : Boolean(row.history_item?.is_pinned),
  );

  let customShortcut = $derived(
    row.row_kind === 'snippet'
      ? row.snippet?.shortcut_key ?? null
      : row.history_item?.pin_shortcut ?? null,
  );

  let quickPickLabel = $derived.by(() => {
    if (row.quick_pick_index === null || row.quick_pick_index === undefined) {
      return null;
    }
    return isMac ? `⌘${row.quick_pick_index}` : `^${row.quick_pick_index}`;
  });

  let appInitials = $derived.by(() => {
    const name = row.history_item?.source_app_name?.trim();
    if (!name) {
      return null;
    }
    const parts = name.split(/\s+/).filter(Boolean);
    if (parts.length >= 2) {
      return (parts[0][0] + parts[1][0]).toUpperCase();
    }
    return name.slice(0, 2).toUpperCase();
  });
</script>

<button
  type="button"
  role="option"
  aria-selected={selected}
  onmouseenter={onHover}
  onclick={onActivate}
  class="group flex h-full w-full items-center gap-2 rounded-lg px-2 text-left transition-colors"
  style:background-color={selected ? 'var(--clipo-selection)' : 'transparent'}
  style:color={selected ? 'var(--clipo-selection-text)' : 'var(--clipo-text)'}
>
  <!-- Leading icon / thumbnail / swatch -->
  <div class="flex shrink-0 items-center justify-center">
    {#if thumbUrl}
      <img
        src={thumbUrl}
        alt={row.history_item?.title_preview ?? 'Clipboard image'}
        class="max-w-[56px] rounded border object-cover"
        style:height={`${clampedThumbHeight}px`}
        style:border-color={selected ? 'rgba(255,255,255,0.3)' : 'var(--clipo-border)'}
        loading="lazy"
      />
    {:else if colorSwatch}
      <span
        class="inline-block h-4 w-4 rounded-full border shadow-xs"
        style:background-color={colorSwatch.cssColor}
        style:border-color={selected ? 'rgba(255,255,255,0.6)' : 'var(--clipo-border)'}
        title={colorSwatch.hex}
      ></span>
    {:else if row.row_kind === 'snippet'}
      <span
        class="inline-flex h-5 w-5 items-center justify-center rounded font-mono text-[10px] font-bold"
        style:background-color={selected ? 'rgba(255,255,255,0.2)' : 'var(--clipo-accent-soft)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-accent)'}
      >
        {'{}'}
      </span>
    {:else if row.history_item?.kind === 'file'}
      <span
        class="inline-flex h-5 w-5 items-center justify-center rounded text-[11px]"
        style:background-color={selected ? 'rgba(255,255,255,0.2)' : 'var(--clipo-badge-bg)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-muted)'}
      >
        📄
      </span>
    {:else if showApplicationIcons && appInitials}
      <span
        class="inline-flex h-5 w-5 items-center justify-center rounded text-[9px] font-bold tracking-tight"
        style:background-color={selected ? 'rgba(255,255,255,0.2)' : 'var(--clipo-badge-bg)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-muted)'}
        title={row.history_item?.source_app_name ?? undefined}
      >
        {appInitials}
      </span>
    {:else}
      <span
        class="inline-flex h-5 w-5 items-center justify-center rounded text-[10px]"
        style:background-color={selected ? 'rgba(255,255,255,0.16)' : 'var(--clipo-surface-muted)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-faint)'}
      >
        T
      </span>
    {/if}
  </div>

  <!-- Title / text with UTF-16 match highlighting -->
  <div class="min-w-0 flex-1">
    <div class="truncate text-[12.5px] leading-snug">
      {#each segments as seg, idx (idx)}
        {#if seg.matched}
          {#if seg.style === 'bold'}
            <strong class="font-bold">{seg.text}</strong>
          {:else if seg.style === 'underline'}
            <span class="underline decoration-2 underline-offset-2">{seg.text}</span>
          {:else}
            <mark
              class="rounded-[2px] px-0.5"
              style:background-color={selected ? 'rgba(255, 255, 255, 0.28)' : 'var(--clipo-highlight-bg)'}
              style:color="inherit"
            >
              {seg.text}
            </mark>
          {/if}
        {:else}
          <span>{seg.text}</span>
        {/if}
      {/each}
    </div>
  </div>

  <!-- Trailing badges (pin, language, custom shortcut, quick-pick) -->
  <div class="flex shrink-0 items-center gap-1">
    {#if row.row_kind === 'snippet' && row.snippet?.language}
      <span
        class="rounded px-1 py-0.5 font-mono text-[9px] uppercase"
        style:background-color={selected ? 'rgba(255,255,255,0.18)' : 'var(--clipo-badge-bg)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-muted)'}
      >
        {row.snippet.language}
      </span>
    {/if}

    {#if isPinned}
      <span
        class="text-[10px]"
        title="Pinned"
        style:color={selected ? '#ffffff' : 'var(--clipo-accent)'}
      >
        ★
      </span>
    {/if}

    {#if customShortcut}
      <span
        class="rounded px-1 py-0.5 font-mono text-[10px]"
        style:background-color={selected ? 'rgba(255,255,255,0.2)' : 'var(--clipo-badge-bg)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-muted)'}
      >
        {customShortcut}
      </span>
    {/if}

    {#if quickPickLabel}
      <span
        class="rounded px-1 py-0.5 font-mono text-[10px] tabular-nums"
        style:background-color={selected ? 'rgba(255,255,255,0.18)' : 'var(--clipo-badge-bg)'}
        style:color={selected ? '#ffffff' : 'var(--clipo-text-faint)'}
      >
        {quickPickLabel}
      </span>
    {/if}
  </div>
</button>
