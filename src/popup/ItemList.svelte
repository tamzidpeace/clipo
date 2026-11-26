<script lang="ts">
  import type { MatchHighlightStyle, SearchRowDto } from '../lib/types';
  import ItemRow from './ItemRow.svelte';

  interface Props {
    rows: SearchRowDto[];
    selectedIndex: number;
    highlightStyle: MatchHighlightStyle;
    showSpecialSymbols: boolean;
    showApplicationIcons: boolean;
    showHexColorSwatch: boolean;
    imageHeight: number;
    isMac: boolean;
    query: string;
    onSelectIndex: (index: number) => void;
    onActivateRow: (row: SearchRowDto, e: MouseEvent) => void;
  }

  let {
    rows,
    selectedIndex,
    highlightStyle,
    showSpecialSymbols,
    showApplicationIcons,
    showHexColorSwatch,
    imageHeight,
    isMac,
    query,
    onSelectIndex,
    onActivateRow,
  }: Props = $props();

  let containerEl: HTMLDivElement | null = $state(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(360);

  const OVERSCAN = 6;

  function getRowHeight(row: SearchRowDto): number {
    if (row.row_kind === 'history' && row.history_item?.kind === 'image') {
      return Math.max(40, Math.min(Math.max(22, imageHeight), 60) + 12);
    }
    return 34;
  }

  function rowKey(row: SearchRowDto): string {
    if (row.row_kind === 'snippet' && row.snippet) {
      return `snippet:${row.snippet.id}`;
    }
    if (row.history_item) {
      return `history:${row.history_item.id}`;
    }
    return `unknown:${row.score}`;
  }

  interface LayoutItem {
    row: SearchRowDto;
    index: number;
    top: number;
    height: number;
    key: string;
  }

  let layout = $derived.by(() => {
    const items: LayoutItem[] = [];
    let top = 0;
    for (let i = 0; i < rows.length; i += 1) {
      const row = rows[i];
      const height = getRowHeight(row);
      items.push({
        row,
        index: i,
        top,
        height,
        key: rowKey(row),
      });
      top += height;
    }
    return { items, totalHeight: top };
  });

  let visibleRange = $derived.by(() => {
    const items = layout.items;
    if (!items.length) {
      return { start: 0, end: 0 };
    }

    const viewStart = scrollTop;
    const viewEnd = scrollTop + viewportHeight;

    let low = 0;
    let high = items.length - 1;
    let firstVisible = 0;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (items[mid].top + items[mid].height >= viewStart) {
        firstVisible = mid;
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    let lastVisible = firstVisible;
    while (
      lastVisible < items.length - 1 &&
      items[lastVisible].top < viewEnd
    ) {
      lastVisible += 1;
    }

    return {
      start: Math.max(0, firstVisible - OVERSCAN),
      end: Math.min(items.length, lastVisible + OVERSCAN + 1),
    };
  });

  let visibleItems = $derived(
    layout.items.slice(visibleRange.start, visibleRange.end),
  );

  $effect(() => {
    if (!containerEl) {
      return;
    }
    viewportHeight = containerEl.clientHeight || 360;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        viewportHeight = entry.contentRect.height || 360;
      }
    });
    observer.observe(containerEl);
    return () => observer.disconnect();
  });

  // Ensure keyboard-selected row remains cleanly scrolled into view.
  $effect(() => {
    const el = containerEl;
    const item = layout.items[selectedIndex];
    if (!el || !item) {
      return;
    }
    const padding = 4;
    const viewTop = el.scrollTop;
    const viewBottom = viewTop + el.clientHeight;
    if (item.top - padding < viewTop) {
      el.scrollTop = Math.max(0, item.top - padding);
    } else if (item.top + item.height + padding > viewBottom) {
      el.scrollTop = item.top + item.height + padding - el.clientHeight;
    }
  });
</script>

<div
  bind:this={containerEl}
  onscroll={(e) => {
    scrollTop = (e.currentTarget as HTMLDivElement).scrollTop;
  }}
  role="listbox"
  aria-label="Clipboard items"
  class="clipo-scroll relative min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5"
>
  {#if rows.length === 0}
    <div
      class="flex h-full flex-col items-center justify-center gap-1 px-4 text-center"
      style:color="var(--clipo-text-faint)"
    >
      <span class="text-[12.5px] font-medium">
        {query.trim() ? 'No matching items' : 'Clipboard history is empty'}
      </span>
      <span class="text-[11px]">
        {query.trim()
          ? 'Try another search query or switch tabs.'
          : 'Copy text, images, or files — or press Alt+S to add a snippet.'}
      </span>
    </div>
  {:else}
    <div
      class="relative w-full"
      style:height={`${layout.totalHeight}px`}
    >
      {#each visibleItems as item (item.key)}
        <div
          class="absolute right-0 left-0 py-0.5"
          style:height={`${item.height}px`}
          style:transform={`translateY(${item.top}px)`}
        >
          <ItemRow
            row={item.row}
            selected={item.index === selectedIndex}
            {highlightStyle}
            {showSpecialSymbols}
            {showApplicationIcons}
            {showHexColorSwatch}
            {imageHeight}
            {isMac}
            onHover={() => onSelectIndex(item.index)}
            onActivate={(e) => onActivateRow(item.row, e)}
          />
        </div>
      {/each}
    </div>
  {/if}
</div>
