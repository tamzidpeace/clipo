export type HotkeyPlatform = 'mac' | 'other';

export interface ParsedHotkey {
  commandOrControl: boolean;
  control: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
  canonical: string;
  hasModifier: boolean;
}

export interface HotkeyEventLike {
  key: string;
  code?: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}

export interface HotkeyCaptureOptions {
  requireModifier?: boolean;
  platform?: HotkeyPlatform;
}

export type HotkeyFailureReason =
  | 'modifier_only'
  | 'missing_modifier'
  | 'unsupported_key';

export type HotkeyCaptureResult =
  | { ok: true; accelerator: string }
  | { ok: false; reason: HotkeyFailureReason; message: string };

export interface HotkeyValidationResult {
  valid: boolean;
  normalized?: string;
  reason?: HotkeyFailureReason | 'empty' | 'invalid';
  message?: string;
}

const MODIFIER_KEYS = new Set([
  'shift',
  'shiftleft',
  'shiftright',
  'meta',
  'metaleft',
  'metaright',
  'control',
  'controlleft',
  'controlright',
  'ctrl',
  'alt',
  'altleft',
  'altright',
  'option',
  'os',
  'super',
  'altgraph',
  'capslock',
  'fn',
  'dead',
  'unidentified',
]);

const KEY_ALIASES: Record<string, string> = {
  space: 'Space',
  spacebar: 'Space',
  ' ': 'Space',
  backspace: 'Backspace',
  delete: 'Delete',
  del: 'Delete',
  enter: 'Enter',
  return: 'Enter',
  tab: 'Tab',
  escape: 'Escape',
  esc: 'Escape',
  arrowup: 'Up',
  up: 'Up',
  arrowdown: 'Down',
  down: 'Down',
  arrowleft: 'Left',
  left: 'Left',
  arrowright: 'Right',
  right: 'Right',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  comma: ',',
  period: '.',
  slash: '/',
  backslash: '\\',
  semicolon: ';',
  quote: "'",
  backquote: '`',
  minus: '-',
  equal: '=',
  plus: '+',
  bracketleft: '[',
  bracketright: ']',
};

const CODE_TO_KEY: Record<string, string> = {
  Space: 'Space',
  Backspace: 'Backspace',
  Delete: 'Delete',
  Enter: 'Enter',
  NumpadEnter: 'Enter',
  Tab: 'Tab',
  Escape: 'Escape',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
};

const ALLOWED_PUNCTUATION = new Set([
  ',',
  '.',
  '/',
  '\\',
  ';',
  "'",
  '`',
  '-',
  '=',
  '+',
  '[',
  ']',
]);

export function detectHotkeyPlatform(): HotkeyPlatform {
  if (typeof navigator !== 'undefined') {
    const label = navigator.platform || navigator.userAgent || '';
    if (/Mac|iPhone|iPad|iPod/i.test(label)) {
      return 'mac';
    }
  }
  return 'other';
}

function normalizeKeyToken(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed && raw !== ' ') {
    return null;
  }
  const lower = trimmed.toLowerCase();
  if (MODIFIER_KEYS.has(lower)) {
    return null;
  }

  if (raw === ' ' || lower in KEY_ALIASES) {
    return raw === ' ' ? 'Space' : KEY_ALIASES[lower];
  }

  if (/^[a-zA-Z]$/.test(trimmed)) {
    return trimmed.toUpperCase();
  }

  if (/^[0-9]$/.test(trimmed)) {
    return trimmed;
  }

  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(trimmed)) {
    return trimmed.toUpperCase();
  }

  if (ALLOWED_PUNCTUATION.has(trimmed)) {
    return trimmed;
  }

  return null;
}

function buildCanonical(parts: {
  commandOrControl: boolean;
  control: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
}): string {
  const tokens: string[] = [];
  if (parts.commandOrControl) {
    tokens.push('CommandOrControl');
  }
  if (parts.control) {
    tokens.push('Control');
  }
  if (parts.meta) {
    tokens.push('Super');
  }
  if (parts.alt) {
    tokens.push('Alt');
  }
  if (parts.shift) {
    tokens.push('Shift');
  }
  tokens.push(parts.key);
  return tokens.join('+');
}

function splitAcceleratorParts(accelerator: string): string[] {
  const trimmed = accelerator.trim();
  if (!trimmed) {
    return [];
  }
  // Preserve trailing `+` when encoded as `CommandOrControl++`
  if (trimmed === '+') {
    return ['+'];
  }
  if (trimmed.endsWith('++')) {
    const prefix = trimmed.slice(0, -2);
    const head = prefix
      .split('+')
      .map((p) => p.trim())
      .filter(Boolean);
    return [...head, '+'];
  }
  return trimmed
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean);
}

export function parseHotkey(accelerator: string): ParsedHotkey | null {
  const parts = splitAcceleratorParts(accelerator);
  if (parts.length === 0) {
    return null;
  }

  let commandOrControl = false;
  let control = false;
  let meta = false;
  let alt = false;
  let shift = false;
  let key: string | null = null;

  for (const part of parts) {
    const lower = part.toLowerCase();
    if (lower === 'commandorcontrol' || lower === 'cmdorctrl') {
      commandOrControl = true;
    } else if (lower === 'control' || lower === 'ctrl') {
      control = true;
    } else if (
      lower === 'command' ||
      lower === 'cmd' ||
      lower === 'meta' ||
      lower === 'super' ||
      lower === 'win'
    ) {
      meta = true;
    } else if (lower === 'alt' || lower === 'option') {
      alt = true;
    } else if (lower === 'shift') {
      shift = true;
    } else {
      const normalized = normalizeKeyToken(part);
      if (!normalized || key !== null) {
        return null;
      }
      key = normalized;
    }
  }

  if (!key) {
    return null;
  }

  const hasModifier = commandOrControl || control || meta || alt || shift;
  return {
    commandOrControl,
    control,
    meta,
    alt,
    shift,
    key,
    canonical: buildCanonical({
      commandOrControl,
      control,
      meta,
      alt,
      shift,
      key,
    }),
    hasModifier,
  };
}

function resolveKeyFromEvent(event: HotkeyEventLike): string | null {
  const code = event.code?.trim();
  if (code) {
    if (MODIFIER_KEYS.has(code.toLowerCase())) {
      return null;
    }
    if (code in CODE_TO_KEY) {
      return CODE_TO_KEY[code];
    }
    if (/^Key[A-Z]$/.test(code)) {
      return code.slice(3);
    }
    if (/^Digit[0-9]$/.test(code)) {
      return code.slice(5);
    }
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) {
      return code;
    }
  }

  return normalizeKeyToken(event.key);
}

function isModifierOnlyEvent(event: HotkeyEventLike): boolean {
  const keyLower = event.key.trim().toLowerCase();
  const codeLower = event.code?.trim().toLowerCase() ?? '';
  return MODIFIER_KEYS.has(keyLower) || (codeLower !== '' && MODIFIER_KEYS.has(codeLower));
}

export function eventToAccelerator(
  event: HotkeyEventLike,
  options: HotkeyCaptureOptions = {},
): HotkeyCaptureResult {
  const platform = options.platform ?? detectHotkeyPlatform();
  const requireModifier = options.requireModifier ?? false;

  if (isModifierOnlyEvent(event)) {
    return {
      ok: false,
      reason: 'modifier_only',
      message: 'Press a regular key together with your modifier keys.',
    };
  }

  const key = resolveKeyFromEvent(event);
  if (!key) {
    return {
      ok: false,
      reason: 'unsupported_key',
      message: 'That key is not supported as a shortcut.',
    };
  }

  const metaPressed = Boolean(event.metaKey);
  const ctrlPressed = Boolean(event.ctrlKey);
  const altPressed = Boolean(event.altKey);
  const shiftPressed = Boolean(event.shiftKey);

  const hasModifier = metaPressed || ctrlPressed || altPressed || shiftPressed;
  if (requireModifier && !hasModifier) {
    return {
      ok: false,
      reason: 'missing_modifier',
      message: 'Include at least one modifier key (Cmd/Ctrl, Alt, or Shift).',
    };
  }

  const commandOrControl = platform === 'mac' ? metaPressed : ctrlPressed;
  const control = platform === 'mac' ? ctrlPressed : false;
  const meta = platform === 'mac' ? false : metaPressed;

  const canonical = buildCanonical({
    commandOrControl,
    control,
    meta,
    alt: altPressed,
    shift: shiftPressed,
    key,
  });

  return { ok: true, accelerator: canonical };
}

export function validateHotkey(
  accelerator: string,
  options: { requireModifier?: boolean } = {},
): HotkeyValidationResult {
  const trimmed = accelerator.trim();
  if (!trimmed) {
    return { valid: false, reason: 'empty', message: 'Shortcut cannot be empty.' };
  }
  const parsed = parseHotkey(trimmed);
  if (!parsed) {
    return {
      valid: false,
      reason: 'invalid',
      message: 'Invalid shortcut format.',
    };
  }
  if ((options.requireModifier ?? false) && !parsed.hasModifier) {
    return {
      valid: false,
      reason: 'missing_modifier',
      message: 'Include at least one modifier key (Cmd/Ctrl, Alt, or Shift).',
    };
  }
  return { valid: true, normalized: parsed.canonical };
}

function formatMacKey(key: string): string {
  switch (key) {
    case 'Backspace':
      return '⌫';
    case 'Delete':
      return '⌦';
    case 'Enter':
      return '↩';
    case 'Tab':
      return '⇥';
    case 'Escape':
      return '⎋';
    case 'Up':
      return '↑';
    case 'Down':
      return '↓';
    case 'Left':
      return '←';
    case 'Right':
      return '→';
    default:
      return key;
  }
}

export function formatHotkey(
  accelerator: string,
  platform: HotkeyPlatform = detectHotkeyPlatform(),
): string {
  const trimmed = accelerator.trim();
  if (!trimmed) {
    return '';
  }

  const parsed = parseHotkey(trimmed);
  if (!parsed) {
    return trimmed;
  }

  if (platform === 'mac') {
    const symbols: string[] = [];
    if (parsed.control) {
      symbols.push('⌃');
    }
    if (parsed.alt) {
      symbols.push('⌥');
    }
    if (parsed.shift) {
      symbols.push('⇧');
    }
    if (parsed.commandOrControl || parsed.meta) {
      symbols.push('⌘');
    }
    symbols.push(formatMacKey(parsed.key));
    return symbols.join('');
  }

  const labels: string[] = [];
  if (parsed.commandOrControl || parsed.control) {
    labels.push('Ctrl');
  }
  if (parsed.meta) {
    labels.push('Super');
  }
  if (parsed.alt) {
    labels.push('Alt');
  }
  if (parsed.shift) {
    labels.push('Shift');
  }
  labels.push(parsed.key);
  return labels.join('+');
}
