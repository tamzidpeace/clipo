<script lang="ts">
  import type { FilterTab } from '../lib/types';

  interface Props {
    activeTab: FilterTab;
    tabs: FilterTab[];
    totalHistoryCount: number;
    totalSnippetCount: number;
    onSelectTab: (tab: FilterTab) => void;
  }

  let {
    activeTab,
    tabs,
    totalHistoryCount,
    totalSnippetCount,
    onSelectTab,
  }: Props = $props();

  const TAB_LABELS: Record<FilterTab, string> = {
    all: 'All',
    snippets: 'Snippets',
    images: 'Images',
    files: 'Files',
  };

  function tabBadge(tab: FilterTab): number | null {
    if (tab === 'all') {
      return totalHistoryCount + totalSnippetCount;
    }
    if (tab === 'snippets') {
      return totalSnippetCount;
    }
    return null;
  }
</script>

<nav
  aria-label="Filter items"
  class="flex items-center justify-between gap-2 border-b px-3 py-1.5"
  style:border-color="var(--clipo-border-subtle)"
>
  <div
    class="flex flex-1 items-center gap-1 rounded-lg p-0.5"
    style:background-color="var(--clipo-surface-muted)"
  >
    {#each tabs as tab (tab)}
      {@const isActive = activeTab === tab}
      {@const badge = tabBadge(tab)}
      <button
        type="button"
        role="tab"
        aria-selected={isActive}
        onclick={() => onSelectTab(tab)}
        class="flex flex-1 items-center justify-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors"
        style:background-color={isActive ? 'var(--clipo-surface-elevated)' : 'transparent'}
        style:color={isActive ? 'var(--clipo-text)' : 'var(--clipo-text-muted)'}
        style:box-shadow={isActive ? '0 1px 2px rgba(0, 0, 0, 0.08)' : 'none'}
      >
        <span>{TAB_LABELS[tab]}</span>
        {#if badge !== null}
          <span
            class="rounded-full px-1 text-[9px] font-semibold tabular-nums"
            style:background-color={isActive ? 'var(--clipo-accent-soft)' : 'var(--clipo-badge-bg)'}
            style:color={isActive ? 'var(--clipo-accent)' : 'var(--clipo-text-faint)'}
          >
            {badge}
          </span>
        {/if}
      </button>
    {/each}
  </div>

  <span
    class="shrink-0 font-mono text-[10px]"
    style:color="var(--clipo-text-faint)"
    title="Cycle tabs with Tab / Shift+Tab"
  >
    ⇥
  </span>
</nav>
