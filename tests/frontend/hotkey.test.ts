import { describe, expect, it } from 'vitest';
import {
  eventToAccelerator,
  formatHotkey,
  matchesKeyboardEvent,
  parseHotkey,
  validateHotkey,
} from '../../src/lib/hotkey';

describe('hotkey utilities', () => {
  describe('parseHotkey', () => {
    it('parses multi-modifier accelerators into structured descriptors', () => {
      const parsed = parseHotkey('CommandOrControl+Shift+C');
      expect(parsed).toEqual({
        commandOrControl: true,
        control: false,
        meta: false,
        alt: false,
        shift: true,
        key: 'C',
        canonical: 'CommandOrControl+Shift+C',
        hasModifier: true,
      });
    });

    it('parses Alt+P, Alt+Backspace, and Control+Space', () => {
      expect(parseHotkey('Alt+P')).toEqual({
        commandOrControl: false,
        control: false,
        meta: false,
        alt: true,
        shift: false,
        key: 'P',
        canonical: 'Alt+P',
        hasModifier: true,
      });

      expect(parseHotkey('Alt+Backspace')).toEqual({
        commandOrControl: false,
        control: false,
        meta: false,
        alt: true,
        shift: false,
        key: 'Backspace',
        canonical: 'Alt+Backspace',
        hasModifier: true,
      });

      expect(parseHotkey('Control+Space')).toEqual({
        commandOrControl: false,
        control: true,
        meta: false,
        alt: false,
        shift: false,
        key: 'Space',
        canonical: 'Control+Space',
        hasModifier: true,
      });
    });

    it('parses single-key accelerators without modifiers', () => {
      expect(parseHotkey('Space')).toEqual({
        commandOrControl: false,
        control: false,
        meta: false,
        alt: false,
        shift: false,
        key: 'Space',
        canonical: 'Space',
        hasModifier: false,
      });

      expect(parseHotkey('f2')).toEqual({
        commandOrControl: false,
        control: false,
        meta: false,
        alt: false,
        shift: false,
        key: 'F2',
        canonical: 'F2',
        hasModifier: false,
      });
    });

    it('returns null for empty, malformed +, or modifier-only strings', () => {
      expect(parseHotkey('')).toBeNull();
      expect(parseHotkey('   ')).toBeNull();
      expect(parseHotkey('Shift')).toBeNull();
      expect(parseHotkey('CommandOrControl+Alt')).toBeNull();
      expect(parseHotkey('CommandOrControl++')).toBeNull();
      expect(parseHotkey('CommandOrControl+')).toBeNull();
      expect(parseHotkey('+')).toBeNull();
    });

    it('normalizes plus alias without emitting trailing ++', () => {
      const parsed = parseHotkey('CommandOrControl+plus');
      expect(parsed).not.toBeNull();
      expect(parsed?.canonical).toBe('CommandOrControl+=');
      expect(parsed?.canonical.endsWith('++')).toBe(false);
    });
  });

  describe('eventToAccelerator', () => {
    it('converts keyboard events to canonical Tauri accelerators on macOS', () => {
      expect(
        eventToAccelerator(
          { key: 'C', code: 'KeyC', metaKey: true, shiftKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'CommandOrControl+Shift+C',
      });

      // Option+P produces 'π' as e.key on macOS, so e.code must take precedence
      expect(
        eventToAccelerator(
          { key: 'π', code: 'KeyP', altKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'Alt+P',
      });

      expect(
        eventToAccelerator(
          { key: ' ', code: 'Space', metaKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'CommandOrControl+Space',
      });
    });

    it('converts keyboard events to canonical Tauri accelerators on Windows/Linux', () => {
      expect(
        eventToAccelerator(
          { key: 'c', code: 'KeyC', ctrlKey: true, shiftKey: true },
          { platform: 'other', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'CommandOrControl+Shift+C',
      });

      expect(
        eventToAccelerator(
          { key: ' ', code: 'Space', ctrlKey: true },
          { platform: 'other', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'CommandOrControl+Space',
      });
    });

    it('rejects modifier-only key presses', () => {
      for (const modKey of ['Shift', 'Meta', 'Control', 'Alt']) {
        const result = eventToAccelerator(
          {
            key: modKey,
            shiftKey: modKey === 'Shift',
            metaKey: modKey === 'Meta',
            ctrlKey: modKey === 'Control',
            altKey: modKey === 'Alt',
          },
          { platform: 'mac', requireModifier: false },
        );
        expect(result.ok).toBe(false);
        if (!result.ok) {
          expect(result.reason).toBe('modifier_only');
        }
      }
    });

    it('rejects bare single-key shortcuts when requireModifier is true', () => {
      for (const event of [
        { key: 'c', code: 'KeyC' },
        { key: ' ', code: 'Space' },
        { key: 'Enter', code: 'Enter' },
      ]) {
        const result = eventToAccelerator(event, {
          platform: 'mac',
          requireModifier: true,
        });
        expect(result.ok).toBe(false);
        if (!result.ok) {
          expect(result.reason).toBe('missing_modifier');
        }
      }
    });

    it('permits single-key shortcuts when requireModifier is false', () => {
      expect(
        eventToAccelerator(
          { key: 'F2', code: 'F2' },
          { platform: 'mac', requireModifier: false },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'F2',
      });

      expect(
        eventToAccelerator(
          { key: ' ', code: 'Space' },
          { platform: 'other', requireModifier: false },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'Space',
      });
    });

    it('resolves macOS Option dead-key shortcuts via event.code', () => {
      expect(
        eventToAccelerator(
          { key: 'Dead', code: 'KeyE', altKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'Alt+E',
      });

      expect(
        eventToAccelerator(
          { key: 'Dead', code: 'KeyN', altKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'Alt+N',
      });

      expect(
        eventToAccelerator(
          { key: 'Dead', code: 'Backquote', altKey: true },
          { platform: 'mac', requireModifier: true },
        ),
      ).toEqual({
        ok: true,
        accelerator: 'Alt+`',
      });
    });

    it('rejects CapsLock, Unidentified, Fn, and codeless Dead as unsupported_key', () => {
      for (const event of [
        { key: 'CapsLock', code: 'CapsLock' },
        { key: 'Unidentified' },
        { key: 'Fn', code: 'Fn' },
        { key: 'Dead' },
      ]) {
        const result = eventToAccelerator(event, {
          platform: 'mac',
          requireModifier: false,
        });
        expect(result.ok).toBe(false);
        if (!result.ok) {
          expect(result.reason).toBe('unsupported_key');
        }
      }
    });

    it('encodes Shift+Equal (+ key) as CommandOrControl+Shift+= without trailing ++', () => {
      const result = eventToAccelerator(
        { key: '+', code: 'Equal', shiftKey: true, metaKey: true },
        { platform: 'mac', requireModifier: true },
      );
      expect(result).toEqual({
        ok: true,
        accelerator: 'CommandOrControl+Shift+=',
      });
    });
  });

  describe('validateHotkey', () => {
    it('enforces requireModifier for global shortcuts while allowing optional single keys', () => {
      expect(validateHotkey('c', { requireModifier: true }).valid).toBe(false);
      expect(validateHotkey('Space', { requireModifier: true }).valid).toBe(false);
      expect(validateHotkey('Enter', { requireModifier: true }).valid).toBe(false);
      expect(validateHotkey('CommandOrControl+Shift+C', { requireModifier: true })).toEqual({
        valid: true,
        normalized: 'CommandOrControl+Shift+C',
      });
      expect(validateHotkey('F2', { requireModifier: false })).toEqual({
        valid: true,
        normalized: 'F2',
      });
      expect(validateHotkey('CommandOrControl++', { requireModifier: true }).valid).toBe(false);
    });
  });

  describe('matchesKeyboardEvent', () => {
    it('matches arrow keys across platforms', () => {
      expect(
        matchesKeyboardEvent(
          { key: 'ArrowUp', code: 'ArrowUp', altKey: true },
          'Alt+Up',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'ArrowDown', code: 'ArrowDown' },
          'Down',
          false,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'ArrowLeft', code: 'ArrowLeft', ctrlKey: true },
          'CommandOrControl+Left',
          false,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'ArrowRight', code: 'ArrowRight', metaKey: true },
          'CommandOrControl+Right',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true },
          'Alt+Up',
          true,
        ),
      ).toBe(false);
    });

    it('matches macOS Option+punctuation and Option dead-key shortcuts', () => {
      expect(
        matchesKeyboardEvent(
          { key: '“', code: 'BracketLeft', altKey: true },
          'Alt+[',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: '…', code: 'Semicolon', altKey: true },
          'Alt+;',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'Dead', code: 'KeyE', altKey: true },
          'Alt+E',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'Dead', code: 'KeyN', altKey: true },
          'Alt+N',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'Dead', code: 'Backquote', altKey: true },
          'Alt+`',
          true,
        ),
      ).toBe(true);
    });

    it('matches CommandOrControl accelerators correctly on macOS vs Windows/Linux', () => {
      expect(
        matchesKeyboardEvent(
          { key: 'c', code: 'KeyC', metaKey: true, shiftKey: true },
          'CommandOrControl+Shift+C',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'c', code: 'KeyC', ctrlKey: true, shiftKey: true },
          'CommandOrControl+Shift+C',
          true,
        ),
      ).toBe(false);
      expect(
        matchesKeyboardEvent(
          { key: 'c', code: 'KeyC', ctrlKey: true, shiftKey: true },
          'CommandOrControl+Shift+C',
          false,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: 'c', code: 'KeyC', metaKey: true, shiftKey: true },
          'CommandOrControl+Shift+C',
          false,
        ),
      ).toBe(false);
      expect(
        matchesKeyboardEvent(
          { key: ',', code: 'Comma', metaKey: true },
          'CommandOrControl+,',
          true,
        ),
      ).toBe(true);
      expect(
        matchesKeyboardEvent(
          { key: ',', code: 'Comma', ctrlKey: true },
          'CommandOrControl+,',
          false,
        ),
      ).toBe(true);
    });
  });

  describe('formatHotkey', () => {
    it('formats accelerators with macOS symbols', () => {
      expect(formatHotkey('CommandOrControl+Shift+C', 'mac')).toBe('⇧⌘C');
      expect(formatHotkey('Alt+P', 'mac')).toBe('⌥P');
      expect(formatHotkey('Alt+Backspace', 'mac')).toBe('⌥⌫');
      expect(formatHotkey('Alt+Shift+Backspace', 'mac')).toBe('⌥⇧⌫');
      expect(formatHotkey('CommandOrControl+,', 'mac')).toBe('⌘,');
    });

    it('formats accelerators with Windows/Linux labels', () => {
      expect(formatHotkey('CommandOrControl+Shift+C', 'other')).toBe('Ctrl+Shift+C');
      expect(formatHotkey('Alt+P', 'other')).toBe('Alt+P');
      expect(formatHotkey('Alt+Backspace', 'other')).toBe('Alt+Backspace');
      expect(formatHotkey('Alt+Shift+Backspace', 'other')).toBe('Alt+Shift+Backspace');
      expect(formatHotkey('CommandOrControl+,', 'other')).toBe('Ctrl+,');
    });

    it('returns empty string for empty input', () => {
      expect(formatHotkey('', 'mac')).toBe('');
      expect(formatHotkey('   ', 'other')).toBe('');
    });
  });
});
