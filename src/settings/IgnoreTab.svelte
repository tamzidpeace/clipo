<script lang="ts">
  import type {
    AppSettingsDto,
    IgnoreRuleDto,
    IgnoreRuleType,
  } from '../lib/types';

  interface Props {
    settings: AppSettingsDto;
    rules: IgnoreRuleDto[];
    loading: boolean;
    onToggleAllowlist: (enabled: boolean) => void;
    onUpsertRule: (
      id: number | null,
      ruleType: IgnoreRuleType,
      pattern: string,
      enabled: boolean,
    ) => Promise<void>;
    onDeleteRule: (id: number) => Promise<void>;
  }

  let {
    settings,
    rules,
    loading,
    onToggleAllowlist,
    onUpsertRule,
    onDeleteRule,
  }: Props = $props();

  type FilterKind = 'all' | IgnoreRuleType;

  let activeFilter = $state<FilterKind>('all');
  let newRuleType = $state<IgnoreRuleType>('app');
  let newPattern = $state('');
  let submitting = $state(false);
  let formError = $state<string | null>(null);

  let counts = $derived.by(() => {
    let app = 0;
    let pasteboard_type = 0;
    let regex = 0;
    for (const rule of rules) {
      if (rule.rule_type === 'app') app += 1;
      else if (rule.rule_type === 'pasteboard_type') pasteboard_type += 1;
      else if (rule.rule_type === 'regex') regex += 1;
    }
    return { all: rules.length, app, pasteboard_type, regex };
  });

  let filteredRules = $derived(
    activeFilter === 'all'
      ? rules
      : rules.filter((r) => r.rule_type === activeFilter),
  );

  let clientRegexError = $derived.by(() => {
    if (newRuleType !== 'regex' || newPattern.trim().length === 0) {
      return null;
    }
    try {
      // eslint-disable-next-line no-new
      new RegExp(newPattern);
      return null;
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  });

  function ruleTypeLabel(kind: IgnoreRuleType): string {
    switch (kind) {
      case 'app':
        return 'Application';
      case 'pasteboard_type':
        return 'Pasteboard type';
      case 'regex':
        return 'Regex';
    }
  }

  function placeholderForType(kind: IgnoreRuleType): string {
    switch (kind) {
      case 'app':
        return 'e.g. com.1password.1password or 1Password';
      case 'pasteboard_type':
        return 'e.g. org.nspasteboard.ConcealedType';
      case 'regex':
        return 'e.g. ^Bearer\\s+[A-Za-z0-9._-]+$';
    }
  }

  function selectFilter(next: FilterKind): void {
    activeFilter = next;
    if (next !== 'all') {
      newRuleType = next;
    }
  }

  async function handleAddRule(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    const trimmed = newPattern.trim();
    if (!trimmed) {
      formError = 'Please enter a pattern before adding a rule.';
      return;
    }
    if (clientRegexError) {
      formError = `Invalid regex: ${clientRegexError}`;
      return;
    }

    submitting = true;
    formError = null;
    try {
      await onUpsertRule(null, newRuleType, trimmed, true);
      newPattern = '';
    } catch (err) {
      formError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  async function handleToggleEnabled(
    rule: IgnoreRuleDto,
    input: HTMLInputElement,
  ): Promise<void> {
    const previous = rule.enabled;
    const next = input.checked;
    formError = null;
    try {
      await onUpsertRule(
        rule.id,
        rule.rule_type,
        rule.pattern,
        next,
      );
    } catch (err) {
      input.checked = previous;
      formError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDelete(id: number): Promise<void> {
    formError = null;
    try {
      await onDeleteRule(id);
    } catch (err) {
      formError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="flex flex-col gap-5">
  <!-- Allowlist Mode -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <label class="flex cursor-pointer items-center justify-between gap-4">
      <div>
        <div class="text-[13px] font-medium">
          Ignore all applications except listed
        </div>
        <div class="text-[11px]" style:color="var(--clipo-text-muted)">
          When enabled, Clipo only records copies originating from enabled Application rules below (allowlist mode).
        </div>
      </div>
      <input
        type="checkbox"
        checked={settings.advanced.ignore_all_apps_except_listed}
        onchange={(e) =>
          onToggleAllowlist((e.currentTarget as HTMLInputElement).checked)}
        class="h-4 w-4 shrink-0 accent-[var(--clipo-accent)]"
      />
    </label>
  </section>

  <!-- Add Rule & Filter Pills -->
  <section
    class="rounded-xl border p-4"
    style:background-color="var(--clipo-surface-elevated)"
    style:border-color="var(--clipo-border)"
  >
    <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
      <h2 class="text-[12px] font-semibold uppercase tracking-wide" style:color="var(--clipo-text-muted)">
        Ignore Rules
      </h2>

      <div class="flex flex-wrap items-center gap-1">
        {#each [
          { id: 'all', label: `All (${counts.all})` },
          { id: 'app', label: `Applications (${counts.app})` },
          { id: 'pasteboard_type', label: `Pasteboard Types (${counts.pasteboard_type})` },
          { id: 'regex', label: `Regex (${counts.regex})` },
        ] as pill (pill.id)}
          <button
            type="button"
            onclick={() => selectFilter(pill.id as FilterKind)}
            class="rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors"
            style:background-color={activeFilter === pill.id
              ? 'var(--clipo-accent-soft)'
              : 'var(--clipo-surface)'}
            style:border-color={activeFilter === pill.id
              ? 'var(--clipo-accent)'
              : 'var(--clipo-border)'}
            style:color={activeFilter === pill.id
              ? 'var(--clipo-accent)'
              : 'var(--clipo-text-muted)'}
          >
            {pill.label}
          </button>
        {/each}
      </div>
    </div>

    <form onsubmit={handleAddRule} class="flex flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <select
          bind:value={newRuleType}
          aria-label="Ignore rule type"
          class="rounded-md border px-2.5 py-1.5 text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color="var(--clipo-border)"
        >
          <option value="app">Application</option>
          <option value="pasteboard_type">Pasteboard type</option>
          <option value="regex">Regex</option>
        </select>

        <input
          type="text"
          bind:value={newPattern}
          placeholder={placeholderForType(newRuleType)}
          spellcheck="false"
          autocomplete="off"
          aria-label="Ignore rule pattern"
          class="min-w-[200px] flex-1 rounded-md border px-2.5 py-1.5 font-mono text-[12px] outline-none"
          style:background-color="var(--clipo-surface)"
          style:border-color={clientRegexError
            ? 'var(--clipo-danger)'
            : 'var(--clipo-border)'}
        />

        <button
          type="submit"
          disabled={submitting || !newPattern.trim() || Boolean(clientRegexError)}
          class="rounded-md px-3 py-1.5 text-[12px] font-semibold text-white transition-opacity hover:opacity-90 disabled:opacity-45"
          style:background-color="var(--clipo-accent)"
        >
          {submitting ? 'Adding…' : 'Add Rule'}
        </button>
      </div>

      {#if clientRegexError}
        <div
          role="alert"
          class="rounded-md px-2.5 py-1.5 text-[11px]"
          style:background-color="rgba(220, 38, 38, 0.12)"
          style:color="var(--clipo-danger)"
        >
          Invalid regex syntax: {clientRegexError}
        </div>
      {/if}

      {#if formError}
        <div
          role="alert"
          class="rounded-md px-2.5 py-1.5 text-[11px]"
          style:background-color="rgba(220, 38, 38, 0.12)"
          style:color="var(--clipo-danger)"
        >
          {formError}
        </div>
      {/if}
    </form>

    <div class="mt-4 flex flex-col gap-1.5">
      {#if loading}
        <div class="py-6 text-center text-[12px]" style:color="var(--clipo-text-muted)">
          Loading ignore rules…
        </div>
      {:else if filteredRules.length === 0}
        <div
          class="rounded-lg border border-dashed py-6 text-center text-[12px]"
          style:border-color="var(--clipo-border)"
          style:color="var(--clipo-text-muted)"
        >
          No {activeFilter === 'all' ? '' : `${ruleTypeLabel(activeFilter).toLowerCase()} `}rules configured yet.
        </div>
      {:else}
        {#each filteredRules as rule (rule.id)}
          <div
            class="flex items-center justify-between gap-3 rounded-lg border px-3 py-2"
            style:background-color="var(--clipo-surface)"
            style:border-color="var(--clipo-border-subtle)"
          >
            <label class="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5">
              <input
                type="checkbox"
                checked={rule.enabled}
                onchange={(e) =>
                  void handleToggleEnabled(
                    rule,
                    e.currentTarget as HTMLInputElement,
                  )}
                class="h-4 w-4 shrink-0 accent-[var(--clipo-accent)]"
              />
              <span
                class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase"
                style:background-color="var(--clipo-badge-bg)"
                style:color="var(--clipo-badge-text)"
              >
                {ruleTypeLabel(rule.rule_type)}
              </span>
              <span
                class="truncate font-mono text-[12px]"
                style:color={rule.enabled
                  ? 'var(--clipo-text)'
                  : 'var(--clipo-text-faint)'}
              >
                {rule.pattern}
              </span>
            </label>

            <button
              type="button"
              onclick={() => void handleDelete(rule.id)}
              class="shrink-0 rounded border px-2 py-0.5 text-[11px] font-medium transition-opacity hover:opacity-80"
              style:border-color="var(--clipo-border)"
              style:color="var(--clipo-danger)"
            >
              Delete
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </section>
</div>
