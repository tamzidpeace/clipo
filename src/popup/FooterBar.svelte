<script lang="ts">
  interface Props {
    paused: boolean;
    itemCount: number;
    previewOpen: boolean;
    onTogglePause: () => void;
    onNewSnippet: () => void;
    onClearUnpinned: () => void;
    onOpenPreferences: () => void;
    onTogglePreview: () => void;
  }

  let {
    paused,
    itemCount,
    previewOpen,
    onTogglePause,
    onNewSnippet,
    onClearUnpinned,
    onOpenPreferences,
    onTogglePreview,
  }: Props = $props();
</script>

<footer
  class="flex items-center justify-between gap-2 border-t px-2.5 py-1.5 text-[11px]"
  style:border-color="var(--clipo-border-subtle)"
  style:background-color="var(--clipo-surface-muted)"
  style:color="var(--clipo-text-muted)"
>
  <div class="flex items-center gap-1.5">
    <button
      type="button"
      onclick={onTogglePause}
      class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10.5px] font-medium transition-colors hover:opacity-85"
      style:background-color={paused ? 'rgba(217, 119, 6, 0.16)' : 'var(--clipo-badge-bg)'}
      style:color={paused ? 'var(--clipo-warning)' : 'var(--clipo-text-muted)'}
      title={paused ? 'Resume clipboard capture' : 'Pause clipboard capture'}
    >
      <span
        class="inline-block h-1.5 w-1.5 rounded-full"
        style:background-color={paused ? 'var(--clipo-warning)' : '#22c55e'}
      ></span>
      <span>{paused ? 'Paused' : `${itemCount} items`}</span>
    </button>

    <button
      type="button"
      onclick={onTogglePreview}
      class="rounded px-1.5 py-0.5 text-[10.5px] font-medium transition-colors hover:opacity-85"
      style:background-color={previewOpen ? 'var(--clipo-accent-soft)' : 'transparent'}
      style:color={previewOpen ? 'var(--clipo-accent)' : 'var(--clipo-text-muted)'}
      title="Toggle preview pane (Ctrl+Space)"
    >
      Preview
    </button>
  </div>

  <div class="flex items-center gap-1">
    <button
      type="button"
      onclick={onNewSnippet}
      class="rounded px-1.5 py-0.5 text-[10.5px] font-medium hover:underline"
      style:color="var(--clipo-text)"
    >
      New Snippet…
    </button>

    <button
      type="button"
      onclick={onClearUnpinned}
      class="rounded px-1.5 py-0.5 text-[10.5px] hover:underline"
      style:color="var(--clipo-text-muted)"
      title="Clear all unpinned history items"
    >
      Clear
    </button>

    <button
      type="button"
      onclick={onOpenPreferences}
      class="rounded px-1.5 py-0.5 text-[10.5px] hover:underline"
      style:color="var(--clipo-text-muted)"
    >
      Preferences…
    </button>
  </div>
</footer>
