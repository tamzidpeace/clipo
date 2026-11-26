<script lang="ts">
  import type {
    AppSettingsDto,
    GeneralSettingsDto,
    PlatformCapabilities,
    SearchMode,
    UpdateStatusDto,
  } from '../lib/types';
  import HotkeyRecorder from './HotkeyRecorder.svelte';

  interface Props {
    settings: AppSettingsDto;
    capabilities: PlatformCapabilities | null;
    updateStatus: UpdateStatusDto | null;
    checkingUpdates: boolean;
    isMac: boolean;
    onUpdateGeneral: (patch: Partial<GeneralSettingsDto>) => void;
    onCheckUpdates: () => void;
    onOpenAccessibility: () => void;
    onRecordingChange: (recording: boolean) => void;
  }

  let {
    settings,
    capabilities,
    updateStatus,
    checkingUpdates,
    isMac,
    onUpdateGeneral,
    onCheckUpdates,
    onOpenAccessibility,
    onRecordingChange,
  }: Props = $props();

  let pasteLegend = $derived(
    isMac
      ? [
          { keys: '↵', desc: 'Paste (or copy if auto-paste is off)' },
          { keys: '⌥↵', desc: 'Copy to clipboard only' },
          { keys: '⌘↵', desc: 'Copy & paste with formatting' },
          { keys: '⌘⇧↵', desc: 'Copy & paste without formatting' },
        ]
      : [
          { keys: '↵', desc: 'Paste (or copy if auto-paste is off)' },
          { keys: 'Alt+↵', desc: 'Copy to clipboard only' },
          { keys: 'Ctrl+↵', desc: 'Copy & paste with formatting' },
          { keys: 'Ctrl+⇧+↵', desc: 'Copy & paste without formatting' },
        ],
  );

  let isMacOs = $derived(
    capabilities ? capabilities.os === 'macos' : isMac,
  );
  let isWayland = $derived(
    capabilities?.session_type?.toLowerCase() === 'wayland',
  );

  function statusLabel(status: UpdateStatusDto['status']): string {
    switch (status) {
      case 'up_to_date':
        return 'Up to date';
      case 'update_available':
        return 'Update available';
      case 'fallback':
        return 'Manual check';
      case 'error':
        return 'Check failed';
    }
  }
</script>

<div class="flex flex-col gap-5">
  <!-- Startup & Updates -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Startup &amp; Updates
    </h2>

    <div class="flex flex-col gap-3">
      <label class="flex cursor-pointer items-center justify-between gap-3">
        <div>
          <div class="text-[13px] font-medium">Launch at login</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Start Clipo automatically in the background when you sign in.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.general.launch_at_login}
          onchange={(e) =>
            onUpdateGeneral({
              launch_at_login: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <div class="border-t pt-3" style:border-color="var(--clipo-border-subtle)">
        <div class="flex items-center justify-between gap-3">
          <label class="flex flex-1 cursor-pointer items-center gap-2.5">
            <input
              type="checkbox"
              checked={settings.general.check_updates_automatically}
              onchange={(e) =>
                onUpdateGeneral({
                  check_updates_automatically: (e.currentTarget as HTMLInputElement)
                    .checked,
                })}
              class="h-4 w-4 accent-[var(--clipo-accent)]"
            />
            <div>
              <div class="text-[13px] font-medium">
                Check for updates automatically
              </div>
              <div class="text-[11px]" style:color="var(--clipo-text-muted)">
                Checks GitHub releases for newer versions of Clipo.
              </div>
            </div>
          </label>

          <button
            type="button"
            onclick={onCheckUpdates}
            disabled={checkingUpdates}
            class="shrink-0 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-85 disabled:opacity-50"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            {checkingUpdates ? 'Checking…' : 'Check now'}
          </button>
        </div>

        {#if updateStatus}
          <div
            class="mt-3 rounded-lg border p-3 text-[12px]"
            style:background-color="var(--clipo-surface-muted)"
            style:border-color="var(--clipo-border-subtle)"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <span
                  class="rounded px-1.5 py-0.5 text-[10.5px] font-semibold"
                  style:background-color={updateStatus.status === 'update_available'
                    ? 'var(--clipo-accent-soft)'
                    : 'var(--clipo-badge-bg)'}
                  style:color={updateStatus.status === 'update_available'
                    ? 'var(--clipo-accent)'
                    : updateStatus.status === 'error'
                      ? 'var(--clipo-danger)'
                      : 'var(--clipo-badge-text)'}
                >
                  {statusLabel(updateStatus.status)}
                </span>
                <span class="font-mono text-[11px]" style:color="var(--clipo-text-muted)">
                  Current: v{updateStatus.current_version}
                  {#if updateStatus.available_version}
                    · Latest: v{updateStatus.available_version}
                  {/if}
                </span>
              </div>

              {#if updateStatus.release_url}
                <a
                  href={updateStatus.release_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-[11.5px] font-medium hover:underline"
                  style:color="var(--clipo-accent)"
                >
                  View releases ↗
                </a>
              {/if}
            </div>

            {#if updateStatus.message}
              <p class="mt-1.5 text-[11.5px]" style:color="var(--clipo-text-muted)">
                {updateStatus.message}
              </p>
            {/if}

            {#if updateStatus.release_notes}
              <pre
                class="clipo-scroll mt-2 max-h-24 overflow-y-auto whitespace-pre-wrap rounded border p-2 font-mono text-[11px]"
                style:background-color="var(--clipo-surface)"
                style:border-color="var(--clipo-border-subtle)"
              >{updateStatus.release_notes}</pre>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </section>

  <!-- Shortcuts -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Shortcuts
    </h2>

    <div class="flex flex-col divide-y" style:border-color="var(--clipo-border-subtle)">
      <div class="flex items-center justify-between gap-4 py-2 first:pt-0">
        <div>
          <div class="text-[13px] font-medium">Open popup</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Global hotkey to toggle the Clipo clipboard window.
          </div>
        </div>
        <HotkeyRecorder
          value={settings.general.open_shortcut}
          requireModifier={true}
          allowEmpty={false}
          ariaLabel="Open popup hotkey"
          onChange={(open_shortcut) => onUpdateGeneral({ open_shortcut })}
          {onRecordingChange}
        />
      </div>

      <div
        class="flex items-center justify-between gap-4 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Pin / Unpin</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Toggle pin state for the selected item inside the popup.
          </div>
        </div>
        <HotkeyRecorder
          value={settings.general.pin_shortcut}
          requireModifier={false}
          allowEmpty={true}
          ariaLabel="Pin or unpin hotkey"
          onChange={(pin_shortcut) => onUpdateGeneral({ pin_shortcut })}
          {onRecordingChange}
        />
      </div>

      <div
        class="flex items-center justify-between gap-4 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Preview pane</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Expand or collapse the sidecar preview pane.
          </div>
        </div>
        <HotkeyRecorder
          value={settings.general.preview_shortcut}
          requireModifier={false}
          allowEmpty={true}
          ariaLabel="Preview pane hotkey"
          onChange={(preview_shortcut) => onUpdateGeneral({ preview_shortcut })}
          {onRecordingChange}
        />
      </div>

      <div
        class="flex items-center justify-between gap-4 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Save as snippet</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Promote the selected history item to a reusable snippet.
          </div>
        </div>
        <HotkeyRecorder
          value={settings.general.save_snippet_shortcut}
          requireModifier={false}
          allowEmpty={true}
          ariaLabel="Save as snippet hotkey"
          onChange={(save_snippet_shortcut) =>
            onUpdateGeneral({ save_snippet_shortcut })}
          {onRecordingChange}
        />
      </div>

      <div
        class="flex items-center justify-between gap-4 py-2 last:pb-0"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Delete item</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Remove the selected history item or snippet.
          </div>
        </div>
        <HotkeyRecorder
          value={settings.general.delete_shortcut}
          requireModifier={false}
          allowEmpty={true}
          ariaLabel="Delete item hotkey"
          onChange={(delete_shortcut) => onUpdateGeneral({ delete_shortcut })}
          {onRecordingChange}
        />
      </div>
    </div>
  </section>

  <!-- Search & Paste -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Search &amp; Paste
    </h2>

    <div class="flex flex-col gap-3">
      <div class="flex items-center justify-between gap-3">
        <div>
          <div class="text-[13px] font-medium">Search mode</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Matching algorithm used while filtering history and snippets.
          </div>
        </div>
        <select
          value={settings.general.search_mode}
          onchange={(e) =>
            onUpdateGeneral({
              search_mode: (e.currentTarget as HTMLSelectElement)
                .value as SearchMode,
            })}
          class="rounded-md border px-2.5 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="fuzzy">Fuzzy</option>
          <option value="exact">Exact</option>
          <option value="regex">Regular expressions</option>
          <option value="mixed">Mixed</option>
        </select>
      </div>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Paste automatically</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Automatically paste into the previous window after selecting an item.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.general.paste_automatically}
          onchange={(e) =>
            onUpdateGeneral({
              paste_automatically: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Paste without formatting</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Strip RTF and HTML styles when pasting rich clipboard entries.
          </div>
        </div>
        <input
          type="checkbox"
          checked={settings.general.paste_without_formatting}
          onchange={(e) =>
            onUpdateGeneral({
              paste_without_formatting: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <div
        class="grid grid-cols-2 gap-2 rounded-lg border p-2.5 text-[11px]"
        style:background-color="var(--clipo-surface-muted)"
        style:border-color="var(--clipo-border-subtle)"
      >
        {#each pasteLegend as entry (entry.keys)}
          <div class="flex items-center gap-2">
            <kbd
              class="shrink-0 rounded border px-1.5 py-0.5 font-mono text-[10.5px] font-semibold"
              style:background-color="var(--clipo-surface-elevated)"
              style:border-color="var(--clipo-border)"
            >
              {entry.keys}
            </kbd>
            <span class="truncate" style:color="var(--clipo-text-muted)">
              {entry.desc}
            </span>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Platform Capabilities -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="mb-2 flex items-center justify-between">
      <h2 class="text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
        System &amp; Permissions
      </h2>
      {#if capabilities}
        <span
          class="rounded px-1.5 py-0.5 font-mono text-[10.5px]"
          style:background-color="var(--clipo-badge-bg)"
          style:color="var(--clipo-badge-text)"
        >
          {capabilities.os} · {capabilities.session_type}
        </span>
      {/if}
    </div>

    {#if isMacOs}
      <div class="flex items-center justify-between gap-3">
        <div class="flex flex-col gap-1">
          <div class="flex items-center gap-2 text-[13px] font-medium">
            <span>Accessibility Permission</span>
            <span
              class="rounded px-1.5 py-0.5 text-[10.5px] font-semibold"
              style:background-color={capabilities?.accessibility_granted === true
                ? 'rgba(34, 197, 94, 0.16)'
                : capabilities?.accessibility_granted === false
                  ? 'rgba(217, 119, 6, 0.16)'
                  : 'var(--clipo-badge-bg)'}
              style:color={capabilities?.accessibility_granted === true
                ? '#16a34a'
                : capabilities?.accessibility_granted === false
                  ? 'var(--clipo-warning)'
                  : 'var(--clipo-text-muted)'}
            >
              {capabilities?.accessibility_granted === true
                ? 'Granted'
                : capabilities?.accessibility_granted === false
                  ? 'Not granted'
                  : 'Unknown'}
            </span>
          </div>
          <p class="text-[11px]" style:color="var(--clipo-text-muted)">
            Required on macOS for automatic pasting into the active application.
          </p>
        </div>

        <button
          type="button"
          onclick={onOpenAccessibility}
          class="shrink-0 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-85"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          Open Accessibility Settings
        </button>
      </div>
    {:else if isWayland}
      <div
        class="rounded-lg border p-3 text-[12px]"
        style:background-color="rgba(217, 119, 6, 0.1)"
        style:border-color="rgba(217, 119, 6, 0.35)"
      >
        <div class="font-semibold" style:color="var(--clipo-warning)">
          Wayland Session Detected
        </div>
        <p class="mt-1 text-[11.5px]" style:color="var(--clipo-text-muted)">
          {capabilities?.notes ??
            'Wayland isolates synthetic keyboard input and global shortcuts. Clipo copies selected items to your clipboard so you can paste manually.'}
        </p>
      </div>
    {:else}
      <div class="flex items-center justify-between gap-3 text-[12px]">
        <div>
          <div class="text-[13px] font-medium">Automatic Paste Support</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            {capabilities?.notes ??
              (capabilities?.can_auto_paste
                ? 'Synthetic paste is supported for your active desktop session.'
                : 'Auto-paste is unavailable; selections will copy to the clipboard.')}
          </div>
        </div>
        <span
          class="shrink-0 rounded px-2 py-0.5 text-[11px] font-semibold"
          style:background-color={capabilities?.can_auto_paste
            ? 'rgba(34, 197, 94, 0.16)'
            : 'var(--clipo-badge-bg)'}
          style:color={capabilities?.can_auto_paste
            ? '#16a34a'
            : 'var(--clipo-text-muted)'}
        >
          {capabilities?.can_auto_paste ? 'Supported' : 'Copy only'}
        </span>
      </div>
    {/if}
  </section>
</div>
