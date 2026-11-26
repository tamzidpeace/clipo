<script lang="ts">
  import type {
    AppearanceSettingsDto,
    AppSettingsDto,
    MatchHighlightStyle,
    PinPosition,
    PopupPosition,
    SearchFieldMode,
    ThemeMode,
  } from '../lib/types';

  interface Props {
    settings: AppSettingsDto;
    onUpdateAppearance: (patch: Partial<AppearanceSettingsDto>) => void;
  }

  let { settings, onUpdateAppearance }: Props = $props();

  let showSearchField = $derived(settings.appearance.search_field !== 'never');
  let localImageHeight = $state(40);

  $effect(() => {
    localImageHeight = settings.appearance.image_height;
  });

  function clampNumber(raw: number, min: number, max: number, fallback: number): number {
    if (!Number.isFinite(raw)) {
      return fallback;
    }
    return Math.min(max, Math.max(min, Math.round(raw)));
  }
</script>

<div class="flex flex-col gap-5">
  <!-- Window & Preview -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Popup &amp; Preview
    </h2>

    <div class="flex flex-col gap-3">
      <div class="grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-[12px] font-medium">Popup at</span>
          <select
            value={settings.appearance.popup_at}
            onchange={(e) =>
              onUpdateAppearance({
                popup_at: (e.currentTarget as HTMLSelectElement)
                  .value as PopupPosition,
              })}
            class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            <option value="cursor">Cursor</option>
            <option value="active_window">Window center</option>
            <option value="screen_center">Screen center</option>
            <option value="menu_icon">Menu icon</option>
          </select>
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[12px] font-medium">Pin to</span>
          <select
            value={settings.appearance.pin_to}
            onchange={(e) =>
              onUpdateAppearance({
                pin_to: (e.currentTarget as HTMLSelectElement)
                  .value as PinPosition,
              })}
            class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          >
            <option value="top">Top</option>
            <option value="bottom">Bottom</option>
          </select>
        </label>
      </div>

      <div
        class="flex items-center justify-between gap-4 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Image height</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Thumbnail height inside the popup list (20–120 px).
          </div>
        </div>
        <div class="flex items-center gap-2.5">
          <input
            type="range"
            min="20"
            max="120"
            step="2"
            value={localImageHeight}
            oninput={(e) => {
              const raw = (e.currentTarget as HTMLInputElement).value.trim();
              const parsed = Number(raw);
              if (raw && Number.isFinite(parsed)) {
                localImageHeight = clampNumber(
                  parsed,
                  20,
                  120,
                  settings.appearance.image_height,
                );
              }
            }}
            onchange={(e) => {
              const input = e.currentTarget as HTMLInputElement;
              const raw = input.value.trim();
              if (!raw) {
                localImageHeight = settings.appearance.image_height;
                return;
              }
              const parsed = Number(raw);
              if (!Number.isFinite(parsed)) {
                localImageHeight = settings.appearance.image_height;
                return;
              }
              const next = clampNumber(
                parsed,
                20,
                120,
                settings.appearance.image_height,
              );
              localImageHeight = next;
              if (next !== settings.appearance.image_height) {
                onUpdateAppearance({ image_height: next });
              }
            }}
            class="w-28 accent-[var(--clipo-accent)]"
          />
          <input
            type="number"
            min="20"
            max="120"
            value={localImageHeight}
            onchange={(e) => {
              const input = e.currentTarget as HTMLInputElement;
              const raw = input.value.trim();
              if (!raw) {
                localImageHeight = settings.appearance.image_height;
                input.value = String(settings.appearance.image_height);
                return;
              }
              const parsed = Number(raw);
              if (!Number.isFinite(parsed)) {
                localImageHeight = settings.appearance.image_height;
                input.value = String(settings.appearance.image_height);
                return;
              }
              const next = clampNumber(
                parsed,
                20,
                120,
                settings.appearance.image_height,
              );
              localImageHeight = next;
              input.value = String(next);
              if (next !== settings.appearance.image_height) {
                onUpdateAppearance({ image_height: next });
              }
            }}
            class="w-16 rounded-md border px-2 py-1 text-center font-mono text-[12px] tabular-nums outline-none"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          />
        </div>
      </div>

      <div
        class="flex flex-col gap-2.5 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <label class="flex cursor-pointer items-center justify-between gap-3">
          <div>
            <div class="text-[13px] font-medium">Open preview automatically</div>
            <div class="text-[11px]" style:color="var(--clipo-text-muted)">
              Show the sidecar preview pane automatically after hovering/selecting.
            </div>
          </div>
          <input
            type="checkbox"
            checked={settings.appearance.open_preview_automatically}
            onchange={(e) =>
              onUpdateAppearance({
                open_preview_automatically: (
                  e.currentTarget as HTMLInputElement
                ).checked,
              })}
            class="h-4 w-4 accent-[var(--clipo-accent)]"
          />
        </label>

        <div class="flex items-center justify-between gap-3">
          <span class="text-[12.5px]" style:color="var(--clipo-text-muted)">
            Preview delay (ms)
          </span>
          <input
            type="number"
            min="0"
            max="10000"
            step="100"
            disabled={!settings.appearance.open_preview_automatically}
            value={settings.appearance.preview_delay_ms}
            onchange={(e) => {
              const input = e.currentTarget as HTMLInputElement;
              const raw = input.value.trim();
              if (!raw) {
                input.value = String(settings.appearance.preview_delay_ms);
                return;
              }
              const parsed = Number(raw);
              if (!Number.isFinite(parsed)) {
                input.value = String(settings.appearance.preview_delay_ms);
                return;
              }
              const next = clampNumber(
                parsed,
                0,
                10000,
                settings.appearance.preview_delay_ms,
              );
              input.value = String(next);
              if (next !== settings.appearance.preview_delay_ms) {
                onUpdateAppearance({ preview_delay_ms: next });
              }
            }}
            class="w-24 rounded-md border px-2 py-1 text-center font-mono text-[12px] tabular-nums outline-none disabled:opacity-45"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border)"
          />
        </div>
      </div>

      <div
        class="flex items-center justify-between gap-3 border-t pt-3"
        style:border-color="var(--clipo-border-subtle)"
      >
        <div>
          <div class="text-[13px] font-medium">Highlight matches</div>
          <div class="text-[11px]" style:color="var(--clipo-text-muted)">
            Visual style for matched search characters in rows.
          </div>
        </div>
        <select
          value={settings.appearance.highlight_matches}
          onchange={(e) =>
            onUpdateAppearance({
              highlight_matches: (e.currentTarget as HTMLSelectElement)
                .value as MatchHighlightStyle,
            })}
          class="rounded-md border px-2.5 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="bold">Bold</option>
          <option value="underline">Underline</option>
          <option value="highlight">Highlight</option>
        </select>
      </div>
    </div>
  </section>

  <!-- Elements & Visibility -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Interface Elements
    </h2>

    <div class="flex flex-col divide-y" style:border-color="var(--clipo-border-subtle)">
      <div class="flex items-center justify-between gap-3 py-2 first:pt-0">
        <label class="flex flex-1 cursor-pointer items-center gap-2.5">
          <input
            type="checkbox"
            checked={showSearchField}
            onchange={(e) =>
              onUpdateAppearance({
                search_field: (e.currentTarget as HTMLInputElement).checked
                  ? 'always'
                  : 'never',
              })}
            class="h-4 w-4 accent-[var(--clipo-accent)]"
          />
          <div>
            <div class="text-[13px] font-medium">Show search field</div>
            <div class="text-[11px]" style:color="var(--clipo-text-muted)">
              Display the search bar in the popup header.
            </div>
          </div>
        </label>

        <select
          value={settings.appearance.search_field}
          onchange={(e) =>
            onUpdateAppearance({
              search_field: (e.currentTarget as HTMLSelectElement)
                .value as SearchFieldMode,
            })}
          class="rounded-md border px-2.5 py-1 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="always">Always</option>
          <option value="when_typing">When typing</option>
          <option value="never">Hidden</option>
        </select>
      </div>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show title before search field</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_title_before_search_field}
          onchange={(e) =>
            onUpdateAppearance({
              show_title_before_search_field: (
                e.currentTarget as HTMLInputElement
              ).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show special symbols</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_special_symbols}
          onchange={(e) =>
            onUpdateAppearance({
              show_special_symbols: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show menu icon</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_menu_icon}
          onchange={(e) =>
            onUpdateAppearance({
              show_menu_icon: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show recent copy next to menu icon</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_recent_copy_next_to_menu_icon}
          onchange={(e) =>
            onUpdateAppearance({
              show_recent_copy_next_to_menu_icon: (
                e.currentTarget as HTMLInputElement
              ).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show application icons</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_application_icons}
          onchange={(e) =>
            onUpdateAppearance({
              show_application_icons: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show footer</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_footer}
          onchange={(e) =>
            onUpdateAppearance({
              show_footer: (e.currentTarget as HTMLInputElement).checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>

      <label
        class="flex cursor-pointer items-center justify-between gap-3 py-2 last:pb-0"
        style:border-color="var(--clipo-border-subtle)"
      >
        <span class="text-[13px] font-medium">Show hex color swatch</span>
        <input
          type="checkbox"
          checked={settings.appearance.show_hex_color_swatch}
          onchange={(e) =>
            onUpdateAppearance({
              show_hex_color_swatch: (e.currentTarget as HTMLInputElement)
                .checked,
            })}
          class="h-4 w-4 accent-[var(--clipo-accent)]"
        />
      </label>
    </div>
  </section>

  <!-- Icons & Theme -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <h2 class="mb-3 text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
      Icon &amp; Theme
    </h2>

    <div class="grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="text-[12px] font-medium">Menu icon</span>
        <select
          value={settings.appearance.menu_icon}
          onchange={(e) =>
            onUpdateAppearance({
              menu_icon: (e.currentTarget as HTMLSelectElement).value,
            })}
          class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="clipo">Clipo</option>
          <option value="clipboard">Clipboard</option>
          <option value="scissors">Scissors</option>
          <option value="paperclip">Paperclip</option>
        </select>
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-[12px] font-medium">Theme</span>
        <select
          value={settings.appearance.theme}
          onchange={(e) =>
            onUpdateAppearance({
              theme: (e.currentTarget as HTMLSelectElement).value as ThemeMode,
            })}
          class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
    </div>
  </section>
</div>
