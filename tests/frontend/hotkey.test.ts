import { describe, expect, it } from 'vitest';
import {
  eventToAccelerator,
  formatHotkey,
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

    it('returns null for empty or modifier-only strings', () => {
      expect(parseHotkey('')).toBeNull();
      expect(parseHotkey('   ')).toBeNull();
      expect(parseHotkey('Shift')).toBeNull();
      expect(parseHotkey('CommandOrControl+Alt')).toBeNull();
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
