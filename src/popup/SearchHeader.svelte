<script lang="ts">
  import type { SearchFieldMode, SearchMode } from '../lib/types';

  interface Props {
    query: string;
    searchMode: SearchMode;
    searchFieldMode: SearchFieldMode;
    showTitleBeforeSearchField: boolean;
    regexError: string | null;
    paused: boolean;
    inputEl?: HTMLInputElement | null;
    onQueryChange: (value: string) => void;
    onClearQuery: () => void;
  }

  let {
    query,
    searchMode,
    searchFieldMode,
    showTitleBeforeSearchField,
    regexError,
    paused,
    inputEl = $bindable(null),
    onQueryChange,
    onClearQuery,
  }: Props = $props();

  let showInput = $derived(
    searchFieldMode === 'always' ||
      (searchFieldMode === 'when_typing' && query.length > 0),
  );

  let modeLabel = $derived(
    searchMode === 'fuzzy'
      ? 'Fuzzy'
      : searchMode === 'exact'
        ? 'Exact'
        : searchMode === 'regex'
          ? 'Regex'
          : 'Mixed',
  );
</script>

{#if showTitleBeforeSearchField || showInput || regexError || paused}
  <header
    class="flex flex-col gap-1.5 border-b px-3 py-2"
    style:border-color="var(--clipo-border-subtle)"
  >
    {#if showTitleBeforeSearchField || (!showInput && paused)}
      <div class="flex items-center justify-between text-[11px]">
        <div class="flex items-center gap-1.5 font-semibold tracking-tight">
          <span
            class="inline-flex h-4 w-4 items-center justify-center rounded text-[10px] font-bold"
            style:background-color="var(--clipo-accent-soft)"
            style:color="var(--clipo-accent)"
          >
            C
          </span>
          <span>Clipo</span>
          <span
            class="rounded px-1.5 py-0.5 text-[10px] font-medium"
            style:background-color="var(--clipo-badge-bg)"
            style:color="var(--clipo-text-muted)"
          >
            {modeLabel}
          </span>
        </div>
        {#if paused}
          <span
            class="rounded px-1.5 py-0.5 text-[10px] font-semibold"
            style:background-color="rgba(217, 119, 6, 0.16)"
            style:color="var(--clipo-warning)"
          >
            Paused
          </span>
        {/if}
      </div>
    {/if}

    {#if showInput}
      <div
        class="flex items-center gap-2 rounded-lg border px-2.5 py-1.5 transition-colors"
        style:background-color="var(--clipo-surface-elevated)"
        style:border-color={regexError ? 'var(--clipo-danger)' : 'var(--clipo-border)'}
      >
        <svg
          class="h-3.5 w-3.5 shrink-0"
          style:color="var(--clipo-text-faint)"
          viewBox="0 0 20 20"
          fill="currentColor"
          aria-hidden="true"
        >
          <path
            fill-rule="evenodd"
            d="M9 3.5a5.5 5.5 0 1 0 3.473 9.766l3.63 3.63a.75.75 0 1 0 1.06-1.06l-3.63-3.63A5.5 5.5 0 0 0 9 3.5ZM5 9a4 4 0 1 1 8 0 4 4 0 0 1-8 0Z"
            clip-rule="evenodd"
          />
        </svg>

        <input
          bind:this={inputEl}
          type="text"
          value={query}
          oninput={(e) => onQueryChange((e.currentTarget as HTMLInputElement).value)}
          placeholder={`Search clipboard (${modeLabel.toLowerCase()})…`}
          spellcheck="false"
          autocomplete="off"
          aria-label="Search clipboard history and snippets"
          class="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:opacity-60"
          style:color="var(--clipo-text)"
        />

        {#if !showTitleBeforeSearchField}
          <span
            class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium"
            style:background-color="var(--clipo-badge-bg)"
            style:color="var(--clipo-text-muted)"
          >
            {modeLabel}
          </span>
        {/if}

        {#if query.length > 0}
          <button
            type="button"
            onclick={onClearQuery}
            title="Clear search"
            class="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[11px] hover:opacity-80"
            style:background-color="var(--clipo-badge-bg)"
            style:color="var(--clipo-text-muted)"
          >
            ×
          </button>
        {/if}
      </div>
    {/if}

    {#if regexError}
      <div
        role="alert"
        class="flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px]"
        style:background-color="rgba(220, 38, 38, 0.12)"
        style:color="var(--clipo-danger)"
      >
        <span class="font-semibold">Regex:</span>
        <span class="truncate">{regexError}</span>
      </div>
    {/if}
  </header>
{/if}
