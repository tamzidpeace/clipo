import { describe, expect, it } from 'vitest';
import {
  DEFAULT_APP_SETTINGS,
  DEFAULT_FILTER_TABS,
  type AppSettingsDto,
  type FilterTab,
} from '../../src/lib/types';

describe('shared domain types', () => {
  it('defines DEFAULT_FILTER_TABS in expected order', () => {
    const expected: FilterTab[] = ['all', 'snippets', 'images', 'files'];
    expect(DEFAULT_FILTER_TABS).toEqual(expected);
  });

  it('provides DEFAULT_APP_SETTINGS matching Maccy defaults', () => {
    const settings: AppSettingsDto = DEFAULT_APP_SETTINGS;
    expect(settings.storage.max_items).toBe(200);
    expect(settings.storage.save_text).toBe(true);
    expect(settings.storage.save_images).toBe(true);
    expect(settings.storage.save_files).toBe(true);
    expect(settings.storage.sort_by).toBe('last_copied');
    expect(settings.appearance.image_height).toBe(40);
    expect(settings.appearance.open_preview_automatically).toBe(true);
    expect(settings.appearance.preview_delay_ms).toBe(1500);
    expect(settings.appearance.pin_to).toBe('top');
    expect(settings.appearance.popup_at).toBe('cursor');
    expect(settings.appearance.highlight_matches).toBe('bold');
    expect(settings.appearance.search_field).toBe('always');
    expect(settings.appearance.theme).toBe('system');
    expect(settings.general.search_mode).toBe('fuzzy');
    expect(settings.general.paste_automatically).toBe(true);
    expect(settings.general.paste_without_formatting).toBe(false);
  });
});
