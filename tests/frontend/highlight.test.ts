import { describe, expect, it } from 'vitest';
import {
  buildClipoImgUrl,
  buildDisplaySegments,
  detectColorSwatch,
  formatSpecialSymbols,
  hexToHslString,
  hexToRgbString,
  splitHighlightSegments,
} from '../../src/lib/highlight';

describe('highlight utilities', () => {
  it('splits plain text by match ranges using bold, underline, or highlight style', () => {
    const segments = splitHighlightSegments(
      'hello clipo world',
      [{ start: 6, end: 11 }],
      'highlight',
    );
    expect(segments).toEqual([
      { text: 'hello ', matched: false, style: 'highlight' },
      { text: 'clipo', matched: true, style: 'highlight' },
      { text: ' world', matched: false, style: 'highlight' },
    ]);
  });

  it('handles UTF-16 match ranges with multibyte and emoji surrogate pairs safely', () => {
    // '🚀' occupies 2 UTF-16 code units (indices 0..2), ' hello' is 2..8
    const text = '🚀 hello 世界';
    const segments = splitHighlightSegments(
      text,
      [
        { start: 0, end: 2 },
        { start: 3, end: 8 },
      ],
      'bold',
    );
    expect(segments.map((s) => ({ text: s.text, matched: s.matched }))).toEqual([
      { text: '🚀', matched: true },
      { text: ' ', matched: false },
      { text: 'hello', matched: true },
      { text: ' 世界', matched: false },
    ]);

    // Mid-surrogate range should snap cleanly without splitting surrogate halves
    const snapped = splitHighlightSegments(text, [{ start: 1, end: 2 }], 'underline');
    expect(snapped[0].text).toBe('🚀');
    expect(snapped[0].matched).toBe(true);
    expect(snapped.map((s) => s.text).join('')).toBe(text);
  });

  it('clamps out-of-bounds, unsorted, and overlapping ranges without throwing', () => {
    const text = 'abcdef';
    const segments = splitHighlightSegments(
      text,
      [
        { start: 3, end: 100 },
        { start: -5, end: 2 },
        { start: 1, end: 4 },
        { start: 5, end: 2 },
      ],
      'underline',
    );
    expect(segments).toEqual([
      { text: 'abcdef', matched: true, style: 'underline' },
    ]);
  });

  it('visualizes special symbols without corrupting match segment boundaries', () => {
    expect(formatSpecialSymbols('a b\tc\nd\r\ne')).toBe('a·b⇥c↵d↵e');

    const text = 'foo bar\nbaz';
    const display = buildDisplaySegments(
      text,
      [{ start: 4, end: 7 }],
      'bold',
      true,
    );
    expect(display).toEqual([
      { text: 'foo·', matched: false, style: 'bold' },
      { text: 'bar', matched: true, style: 'bold' },
      { text: '↵baz', matched: false, style: 'bold' },
    ]);
  });

  it('detects #RGB, #RRGGBB, #RRGGBBAA, and rgb()/rgba() color swatches', () => {
    const shortHex = detectColorSwatch(' #f53 ');
    expect(shortHex).not.toBeNull();
    expect(shortHex?.hex).toBe('#ff5533');
    expect(shortHex?.rgb).toBe('rgb(255, 85, 51)');

    const fullHex = detectColorSwatch('#ff5733');
    expect(fullHex?.hex).toBe('#ff5733');
    expect(fullHex?.rgb).toBe('rgb(255, 87, 51)');
    expect(fullHex?.hsl).toBe('hsl(11, 100%, 60%)');

    const alphaHex = detectColorSwatch('#ff000080');
    expect(alphaHex?.hex).toBe('#ff000080');
    expect(alphaHex?.rgb).toBe('rgba(255, 0, 0, 0.5)');
    expect(alphaHex?.hsl).toBe('hsla(0, 100%, 50%, 0.5)');

    const rgb = detectColorSwatch('rgb(0, 128, 255)');
    expect(rgb?.hex).toBe('#0080ff');
    expect(rgb?.rgb).toBe('rgb(0, 128, 255)');

    const rgba = detectColorSwatch('rgba(16, 32, 48, 0.25)');
    expect(rgba?.rgb).toBe('rgba(16, 32, 48, 0.25)');

    expect(detectColorSwatch('not a color #12345')).toBeNull();
  });

  it('converts hex colors to RGB and HSL preview strings', () => {
    expect(hexToRgbString('#000000')).toBe('rgb(0, 0, 0)');
    expect(hexToHslString('#ffffff')).toBe('hsl(0, 0%, 100%)');
    expect(hexToRgbString('invalid')).toBeNull();
    expect(hexToHslString('invalid')).toBeNull();
  });

  it('builds safe clipoimg://localhost URLs for blob and thumb paths', () => {
    expect(buildClipoImgUrl('thumbs/my file.png')).toBe(
      'clipoimg://localhost/thumbs/my%20file.png',
    );
    expect(buildClipoImgUrl('/images/a.png')).toBe('clipoimg://localhost/images/a.png');
    expect(buildClipoImgUrl(null)).toBeNull();
  });
});
