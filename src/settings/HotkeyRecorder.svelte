<script lang="ts">
  import {
    detectHotkeyPlatform,
    eventToAccelerator,
    formatHotkey,
  } from '../lib/hotkey';

  interface Props {
    value: string;
    requireModifier?: boolean;
    allowEmpty?: boolean;
    placeholder?: string;
    ariaLabel?: string;
    onChange: (nextValue: string) => void;
    onRecordingChange?: (recording: boolean) => void;
  }

  let {
    value,
    requireModifier = false,
    allowEmpty = true,
    placeholder = 'Record shortcut',
    ariaLabel = 'Keyboard shortcut recorder',
    onChange,
    onRecordingChange,
  }: Props = $props();

  const platform = detectHotkeyPlatform();

  let recording = $state(false);
  let feedback = $state<string | null>(null);
  let buttonEl: HTMLButtonElement | null = $state(null);

  let displayText = $derived.by(() => {
    if (recording) {
      return 'Press shortcut…';
    }
    const formatted = formatHotkey(value, platform);
    return formatted || placeholder;
  });

  function setRecording(next: boolean): void {
    if (recording === next) {
      return;
    }
    recording = next;
    onRecordingChange?.(next);
  }

  function startRecording(): void {
    feedback = null;
    setRecording(true);
    buttonEl?.focus();
  }

  function cancelRecording(): void {
    feedback = null;
    setRecording(false);
  }

  function clearShortcut(e: MouseEvent): void {
    e.stopPropagation();
    feedback = null;
    setRecording(false);
    onChange('');
  }

  function handleKeyDown(e: KeyboardEvent): void {
    if (!recording) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        startRecording();
      }
      return;
    }

    e.preventDefault();
    e.stopPropagation();

    if (
      e.key === 'Escape' &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.shiftKey
    ) {
      cancelRecording();
      return;
    }

    const result = eventToAccelerator(e, { requireModifier, platform });
    if (!result.ok) {
      feedback =
        result.reason === 'modifier_only'
          ? 'Press a key together with modifiers…'
          : result.message;
      return;
    }

    feedback = null;
    setRecording(false);
    onChange(result.accelerator);
  }
</script>

<div class="inline-flex flex-col items-end gap-1">
  <div class="inline-flex items-center gap-1">
    <button
      bind:this={buttonEl}
      type="button"
      aria-label={ariaLabel}
      aria-pressed={recording}
      onclick={() => (recording ? cancelRecording() : startRecording())}
      onkeydown={handleKeyDown}
      onblur={() => {
        if (recording) {
          cancelRecording();
        }
      }}
      class="inline-flex min-w-[126px] items-center justify-center rounded-md border px-2.5 py-1 text-[11.5px] font-medium transition-colors outline-none"
      style:background-color={recording
        ? 'var(--clipo-accent-soft)'
        : 'var(--clipo-surface-elevated)'}
      style:border-color={recording
        ? 'var(--clipo-accent)'
        : feedback
          ? 'var(--clipo-danger)'
          : 'var(--clipo-border)'}
      style:color={recording
        ? 'var(--clipo-accent)'
        : value.trim()
          ? 'var(--clipo-text)'
          : 'var(--clipo-text-faint)'}
    >
      <span class="font-mono tracking-tight">{displayText}</span>
    </button>

    {#if allowEmpty && value.trim().length > 0 && !recording}
      <button
        type="button"
        onclick={clearShortcut}
        title="Clear shortcut"
        aria-label="Clear shortcut"
        class="inline-flex h-6 w-6 items-center justify-center rounded-md border text-[12px] transition-opacity hover:opacity-80"
        style:background-color="var(--clipo-surface-elevated)"
        style:border-color="var(--clipo-border)"
        style:color="var(--clipo-text-muted)"
      >
        ×
      </button>
    {/if}
  </div>

  {#if feedback}
    <p class="text-right text-[10.5px]" style:color="var(--clipo-danger)">
      {feedback}
    </p>
  {/if}
</div>
