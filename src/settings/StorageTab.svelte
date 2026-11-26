<script lang="ts">
  import type {
    AppSettingsDto,
    SortBy,
    StorageSettingsDto,
    StorageStats,
  } from '../lib/types';

  interface Props {
    settings: AppSettingsDto;
    stats: StorageStats | null;
    loadingStats: boolean;
    onUpdateStorage: (patch: Partial<StorageSettingsDto>) => void;
    onRefreshStats: () => void;
  }

  let {
    settings,
    stats,
    loadingStats,
    onUpdateStorage,
    onRefreshStats,
  }: Props = $props();

  function clampMaxItems(raw: number): number {
    if (!Number.isFinite(raw)) {
      return settings.storage.max_items;
    }
    return Math.min(10000, Math.max(1, Math.round(raw)));
  }

  function stepMaxItems(delta: number): void {
    const next = clampMaxItems(settings.storage.max_items + delta);
    if (next !== settings.storage.max_items) {
      onUpdateStorage({ max_items: next });
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) {
      return `${bytes} B`;
    }
    const kb = bytes / 1024;
    if (kb < 1024) {
      return `${kb.toFixed(1)} KB`;
    }
    const mb = kb / 1024;
    if (mb < 1024) {
      return `${mb.toFixed(1)} MB`;
    }
    return `${(mb / 1024).toFixed(2)} GB`;
  }
</script>

<div class="flex flex-col gap-5">
  <!-- Save Types -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Save
    </h2>

    <div class="flex flex-col divide-y" style:border-color="var(--clipo-border-subtle)">
      <label class="flex cursor-pointer items-center justify-between gap-3 py-2 first:pt-0">
        <div>
          <div class="text-[13px] font-medium">Save files</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Record copied file and folder paths from Finder or file managers.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.storage.save_files}
          onchange={(e) =>
            onUpdateStorage({
              save_files: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Save images</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Store copied screenshots and image data as PNG thumbnails and blobs.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.storage.save_images}
          onchange={(e) =>
            onUpdateStorage({
              save_images: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2 last:pb-0"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Save text</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Capture plain text, rich text (RTF), and HTML clipboard payloads.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.storage.save_text}
          onchange={(e) =>
            onUpdateStorage({
              save_text: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>
    </div>
  </section>

  <!-- Capacity & Sort -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Retention &amp; Ordering
    </h2>

    <div class="flex flex-col gap-3">
      <div class="flex items-center justify-between gap-3">
        <div>
          <div class="text-[13px] font-medium">Max history size</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Unpinned items kept in history (1 to 10,000). Pinned items are never pruned.
          </div>
        </div>

        <div class="flex items-center gap-1">
          <button
            type="button"
            onclick={() => stepMaxItems(-25)}
            aria-label="Decrease max history size"
            class="inline-flex h-7 w-7 items-center justify-center rounded-md border text-[13px] font-semibold hover:opacity-80"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            −
          </button>
          <input
            type="number"
            min="1"
            max="10000"
            value={settings.storage.max_items}
            onchange={(e) => {
              const next = clampMaxItems(
                Number((e.currentTarget as HTMLInputElement).value),
              );
              (e.currentTarget as HTMLInputElement).value = String(next);
              onUpdateStorage({ max_items: next });
            }}
            class="h-7 w-20 rounded-md border px-2 text-center font-mono text-[12px] tabular-nums outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          />
          <button
            type="button"
            onclick={() => stepMaxItems(25)}
            aria-label="Increase max history size"
            class="inline-flex h-7 w-7 items-center justify-center rounded-md border text-[13px] font-semibold hover:opacity-80"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            +
          </button>
        </div>
      </div>

      <div
        class="flex items-center justify-between gap-3 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Sort by</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Order in which unpinned history entries appear.
          </div>
        </div>
        <select
          value={settings.storage.sort_by}
          onchange={(e) =>
            onUpdateStorage({
              sort_by: (e.currentTarget as HTMLSelectElement).value as SortBy,
            })}
          class="rounded-md border px-2.5 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="last_copied">Time of last copy</option>
          <option value="first_copied">Time of first copy</option>
          <option value="copy_count">Number of copies</option>
        </select>
      </div>
    </div>
  </section>

  <!-- Live Disk Usage -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="mb-3 flex items-center justify-between gap-2">
      <div>
        <h2 class="text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
          Storage Usage
        </h2>
        <p class="text-[11px]" style:color="var(--clipo-text-muted)">
          Live SQLite database and offloaded blob footprint on disk.
        </p>
      </div>
      <button
        type="button"
        onclick={onRefreshStats}
        disabled={loadingStats}
        class="rounded-md border px-2.5 py-1 text-[11.5px] font-medium hover:opacity-85 disabled:opacity-50"
        style:background-color="var(--clipo-surface)"
        style:border-color="var(--clipo-border)"
      >
        {loadingStats ? 'Refreshing…' : 'Refresh'}
      </button>
    </div>

    {#if stats}
      <div class="grid grid-cols-3 gap-2.5">
        <div
          class="rounded-lg border p-2.5"
          style:background-color="var(--clipo-surface-muted)"
          style:border-color="var(--clipo-border-subtle)"
        >
          <div class="text-[10.5px]" style:color="var(--clipo-text-muted)">
            History items
          </div>
          <div class="mt-0.5 text-[16px] font-semibold tabular-nums">
            {stats.total_items}
          </div>
          <div class="text-[10px]" style:color="var(--clipo-text-faint)">
            {stats.pinned_items} pinned · {stats.unpinned_items} unpinned
          </div>
        </div>

        <div
          class="rounded-lg border p-2.5"
          style:background-color="var(--clipo-surface-muted)"
          style:border-color="var(--clipo-border-subtle)"
        >
          <div class="text-[10.5px]" style:color="var(--clipo-text-muted)">
            Snippets
          </div>
          <div class="mt-0.5 text-[16px] font-semibold tabular-nums">
            {stats.snippet_count}
          </div>
          <div class="text-[10px]" style:color="var(--clipo-text-faint)">
            Reusable templates
          </div>
        </div>

        <div
          class="rounded-lg border p-2.5"
          style:background-color="var(--clipo-surface-muted)"
          style:border-color="var(--clipo-border-subtle)"
        >
          <div class="text-[10.5px]" style:color="var(--clipo-text-muted)">
            Total disk usage
          </div>
          <div class="mt-0.5 text-[16px] font-semibold tabular-nums">
            {formatBytes(stats.total_bytes)}
          </div>
          <div class="text-[10px]" style:color="var(--clipo-text-faint)">
            DB {formatBytes(stats.db_bytes)} · Blobs {formatBytes(stats.blob_bytes)}
          </div>
        </div>
      </div>
    {:else}
      <div
        class="rounded-lg border p-3 text-[12px]"
        style:background-color="var(--clipo-surface-muted)"
        style:border-color="var(--clipo-border-subtle)"
        style:color="var(--clipo-text-muted)"
      >
        Storage stats unavailable outside the desktop runtime.
      </div>
    {/if}
  </section>
</div>
