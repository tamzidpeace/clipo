<script lang="ts">
  import type {
    AdvancedSettingsDto,
    AppSettingsDto,
    RuntimeStatusDto,
  } from '../lib/types';

  interface Props {
    settings: AppSettingsDto;
    runtimeStatus: RuntimeStatusDto | null;
    onUpdateAdvanced: (patch: Partial<AdvancedSettingsDto>) => void;
    onTogglePaused: (paused: boolean) => void;
    onIgnoreNext: () => void;
    onClearUnpinned: () => Promise<void>;
  }

  let {
    settings,
    runtimeStatus,
    onUpdateAdvanced,
    onTogglePaused,
    onIgnoreNext,
    onClearUnpinned,
  }: Props = $props();

  const CLI_COMMANDS = [
    {
      cmd: 'clipo --pause',
      desc: 'Pause background clipboard capture',
    },
    {
      cmd: 'clipo --resume',
      desc: 'Resume background clipboard capture',
    },
    {
      cmd: 'clipo --ignore-next',
      desc: 'Skip recording the very next clipboard change',
    },
    {
      cmd: 'clipo --toggle',
      desc: 'Toggle the Clipo popup window from a script or shortcut',
    },
  ];

  let confirmingClear = $state(false);
  let clearing = $state(false);
  let clearFeedback = $state<string | null>(null);
  let copiedCmd = $state<string | null>(null);

  let isPaused = $derived(
    runtimeStatus?.paused ?? settings.advanced.turn_off,
  );
  let isIgnoreNextArmed = $derived(runtimeStatus?.ignore_next ?? false);

  function clampPollInterval(raw: number): number {
    if (!Number.isFinite(raw)) {
      return settings.advanced.poll_interval_ms;
    }
    return Math.min(10000, Math.max(50, Math.round(raw)));
  }

  async function handleClearUnpinned(): Promise<void> {
    if (!confirmingClear) {
      confirmingClear = true;
      clearFeedback = null;
      return;
    }
    clearing = true;
    try {
      await onClearUnpinned();
      confirmingClear = false;
      clearFeedback = 'Unpinned clipboard history cleared.';
    } finally {
      clearing = false;
    }
  }

  async function copyCliCommand(cmd: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(cmd);
      copiedCmd = cmd;
      setTimeout(() => {
        if (copiedCmd === cmd) {
          copiedCmd = null;
        }
      }, 1600);
    } catch {
      // Clipboard write unavailable in headless mode
    }
  }
</script>

<div class="flex flex-col gap-5">
  <!-- Capture Control -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Clipboard Capture
    </h2>

    <div class="flex flex-col divide-y" style:border-color="var(--clipo-border-subtle)">
      <label class="flex cursor-pointer items-center justify-between gap-3 py-2 first:pt-0">
        <div>
          <div class="text-[13px] font-medium">Turn off Clipo</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Pause clipboard monitoring without quitting Clipo.
          </div>
        </div>
        <input
          type="checkbox"
          checked={isPaused}
          onchange={(e) =>
            onTogglePaused((e.currentTarget as HTMLInputElement).checked)}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <div
        class="flex items-center justify-between gap-3 py-2.5"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="flex items-center gap-2 text-[13px] font-medium">
            <span>Ignore next copy</span>
            {#if isIgnoreNextArmed}
              <span
                class="rounded px-1.5 py-0.5 text-[10px] font-semibold"
                style:background-color="rgba(217, 119, 6, 0.16)"
                style:color="var(--clipo-warning)"
              >
                Armed
              </span>
            {/if}
          </div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Skip recording the next external clipboard change once.
          </div>
        </div>

        <button
          type="button"
          onclick={onIgnoreNext}
          class="shrink-0 rounded-md border px-3 py-1.5 text-[12px] font-medium hover:opacity-85"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          {isIgnoreNextArmed ? 'Armed for next copy' : 'Ignore next copy'}
        </button>
      </div>

      <div
        class="flex items-center justify-between gap-3 py-2.5"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Poll interval (ms)</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Clipboard polling cadence in milliseconds (minimum 50 ms).
          </div>
        </div>
        <input
          type="number"
          min="50"
          max="10000"
          step="50"
          value={settings.advanced.poll_interval_ms}
          onchange={(e) => {
            const next = clampPollInterval(
              Number((e.currentTarget as HTMLInputElement).value),
            );
            (e.currentTarget as HTMLInputElement).value = String(next);
            onUpdateAdvanced({ poll_interval_ms: next });
          }}
          class="w-24 rounded-md border px-2 py-1 text-center font-mono text-[12px] tabular-nums outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        />
      </div>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2 last:pb-0"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">
            Move cursor to <code class="font-mono text-[12px]">{'{{cursor}}'}</code> placeholder
          </div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Send left-arrow keystrokes after pasting a snippet containing {'{{cursor}}'}.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.advanced.move_cursor_on_snippet_placeholder}
          onchange={(e) =>
            onUpdateAdvanced({
              move_cursor_on_snippet_placeholder: (
                e.currentTarget as HTMLInputElement
              ).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>
    </div>
  </section>

  <!-- Cleanup -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Cleanup
    </h2>

    <div class="flex flex-col divide-y" style:border-color="var(--clipo-border-subtle)">
      <label class="flex cursor-pointer items-center justify-between gap-3 py-2 first:pt-0">
        <div>
          <div class="text-[13px] font-medium">Clear history on quit</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Remove unpinned clipboard history items automatically when exiting Clipo.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.advanced.clear_history_on_quit}
          onchange={(e) =>
            onUpdateAdvanced({
              clear_history_on_quit: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Clear the system clipboard too</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Empty the OS clipboard alongside history cleanup on quit.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.advanced.clear_system_clipboard_on_quit}
          onchange={(e) =>
            onUpdateAdvanced({
              clear_system_clipboard_on_quit: (
                e.currentTarget as HTMLInputElement
              ).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <div
        class="flex flex-wrap items-center justify-between gap-3 pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Clear unpinned history</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Immediately delete all unpinned items while preserving pinned entries and snippets.
          </div>
        </div>

        <div class="flex items-center gap-2">
          {#if confirmingClear}
            <button
              type="button"
              onclick={() => {
                confirmingClear = false;
              }}
              class="rounded-md border px-2.5 py-1 text-[11.5px] font-medium hover:opacity-85"
              style:border-color="var(--clipo-border)"
            >
              Cancel
            </button>
          {/if}
          <button
            type="button"
            onclick={() => void handleClearUnpinned()}
            disabled={clearing}
            class="rounded-md border px-3 py-1.5 text-[12px] font-semibold transition-opacity hover:opacity-85 disabled:opacity-50"
            style:background-color={confirmingClear
              ? 'var(--clipo-danger)'
              : 'var(--clipo-surface)'}
            style:border-color={confirmingClear
              ? 'var(--clipo-danger)'
              : 'var(--clipo-border)'}
            style:color={confirmingClear ? '#ffffff' : 'var(--clipo-danger)'}
          >
            {clearing
              ? 'Clearing…'
              : confirmingClear
                ? 'Confirm clear unpinned'
                : 'Clear unpinned history'}
          </button>
        </div>
      </div>
    </div>

    {#if clearFeedback}
      <p class="mt-2 text-[11.5px]" style:color="var(--clipo-accent)">
        {clearFeedback}
      </p>
    {/if}
  </section>

  <!-- CLI Reference -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-1 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      CLI Reference
    </h2>
    <p class="mb-3 text-[11px]" style:color="var(--clipo-text-muted)">
      Control the running Clipo instance from scripts, Alfred/Raycast workflows, or terminal aliases.
    </p>

    <div class="flex flex-col gap-2">
      {#each CLI_COMMANDS as item (item.cmd)}
        <div
          class="flex items-center justify-between gap-3 rounded-lg border px-3 py-2"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border-subtle)"
        >
          <div class="min-w-0">
            <code class="font-mono text-[12px] font-semibold">{item.cmd}</code>
            <div class="text-[11px]" style:color="var(--clipo-text-muted)">
              {item.desc}
            </div>
          </div>
          <button
            type="button"
            onclick={() => void copyCliCommand(item.cmd)}
            class="shrink-0 rounded-md border px-2.5 py-1 text-[11px] font-medium hover:opacity-85"
            style:background-color="var(--clipo-surface-elevated)"
            style:border-color="var(--clipo-border)"
          >
            {copiedCmd === item.cmd ? 'Copied' : 'Copy'}
          </button>
        </div>
      {/each}
    </div>
  </section>
</div>
