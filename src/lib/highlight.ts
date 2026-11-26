import type { MatchHighlightStyle, MatchRangeDto } from './types';

export interface MatchSegment {
  text: string;
  matched: boolean;
  style: MatchHighlightStyle;
}

export interface ColorSwatch {
  raw: string;
  hex: string;
  rgb: string;
  hsl: string;
  cssColor: string;
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

function snapStartIndex(text: string, index: number): number {
  const clamped = Math.max(0, Math.min(text.length, Math.trunc(index)));
  if (
    clamped > 0 &&
    clamped < text.length &&
    isLowSurrogate(text.charCodeAt(clamped)) &&
    isHighSurrogate(text.charCodeAt(clamped - 1))
  ) {
    return clamped - 1;
  }
  return clamped;
}

function snapEndIndex(text: string, index: number): number {
  const clamped = Math.max(0, Math.min(text.length, Math.trunc(index)));
  if (
    clamped > 0 &&
    clamped < text.length &&
    isLowSurrogate(text.charCodeAt(clamped)) &&
    isHighSurrogate(text.charCodeAt(clamped - 1))
  ) {
    return clamped + 1;
  }
  return clamped;
}

function normalizeRanges(text: string, ranges: MatchRangeDto[]): MatchRangeDto[] {
  if (!text.length || !ranges.length) {
    return [];
  }

  const valid: MatchRangeDto[] = [];
  for (const range of ranges) {
    if (!Number.isFinite(range.start) || !Number.isFinite(range.end)) {
      continue;
    }
    const start = snapStartIndex(text, range.start);
    const end = snapEndIndex(text, range.end);
    if (end > start) {
      valid.push({ start, end });
    }
  }

  if (!valid.length) {
    return [];
  }

  valid.sort((a, b) => (a.start !== b.start ? a.start - b.start : a.end - b.end));

  const merged: MatchRangeDto[] = [valid[0]];
  for (let i = 1; i < valid.length; i += 1) {
    const prev = merged[merged.length - 1];
    const curr = valid[i];
    if (curr.start <= prev.end) {
      prev.end = Math.max(prev.end, curr.end);
    } else {
      merged.push({ ...curr });
    }
  }

  return merged;
}

/**
 * Splits `text` into matched and unmatched segments using UTF-16 code unit indices
 * while clamping out-of-bounds/overlapping ranges and preserving surrogate pairs.
 */
export function splitHighlightSegments(
  text: string,
  ranges: MatchRangeDto[],
  style: MatchHighlightStyle = 'bold',
): MatchSegment[] {
  if (!text) {
    return [];
  }

  const merged = normalizeRanges(text, ranges);
  if (!merged.length) {
    return [{ text, matched: false, style }];
  }

  const segments: MatchSegment[] = [];
  let cursor = 0;

  for (const range of merged) {
    if (range.start > cursor) {
      segments.push({
        text: text.slice(cursor, range.start),
        matched: false,
        style,
      });
    }
    segments.push({
      text: text.slice(range.start, range.end),
      matched: true,
      style,
    });
    cursor = range.end;
  }

  if (cursor < text.length) {
    segments.push({
      text: text.slice(cursor),
      matched: false,
      style,
    });
  }

  return segments;
}

/**
 * Replaces whitespace control characters with visible symbols (`↵`, `⇥`, `·`).
 */
export function formatSpecialSymbols(text: string): string {
  return text
    .replace(/\r\n/g, '↵')
    .replace(/[\r\n]/g, '↵')
    .replace(/\t/g, '⇥')
    .replace(/ /g, '·');
}

/**
 * Splits `text` by UTF-16 `ranges` first and optionally visualizes special symbols
 * per segment so match boundaries remain intact.
 */
export function buildDisplaySegments(
  text: string,
  ranges: MatchRangeDto[],
  style: MatchHighlightStyle = 'bold',
  showSpecialSymbols = false,
): MatchSegment[] {
  const segments = splitHighlightSegments(text, ranges, style);
  if (!showSpecialSymbols) {
    return segments;
  }
  return segments.map((segment) => ({
    ...segment,
    text: formatSpecialSymbols(segment.text),
  }));
}

interface ParsedRgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

function parseHexColor(trimmed: string): ParsedRgba | null {
  const match = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.exec(
    trimmed,
  );
  if (!match) {
    return null;
  }
  const hex = match[1];
  if (hex.length === 3 || hex.length === 4) {
    const r = parseInt(hex[0] + hex[0], 16);
    const g = parseInt(hex[1] + hex[1], 16);
    const b = parseInt(hex[2] + hex[2], 16);
    const a = hex.length === 4 ? parseInt(hex[3] + hex[3], 16) / 255 : 1;
    return { r, g, b, a };
  }
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  const a = hex.length === 8 ? parseInt(hex.slice(6, 8), 16) / 255 : 1;
  return { r, g, b, a };
}

function parseRgbFunction(trimmed: string): ParsedRgba | null {
  const match =
    /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})(?:\s*,\s*(0|1|0?\.\d+|1\.0+|\d{1,3}%))?\s*\)$/i.exec(
      trimmed,
    );
  if (!match) {
    return null;
  }
  const r = Number(match[1]);
  const g = Number(match[2]);
  const b = Number(match[3]);
  if (r > 255 || g > 255 || b > 255) {
    return null;
  }

  let a = 1;
  if (match[4] !== undefined) {
    if (match[4].endsWith('%')) {
      a = Number(match[4].slice(0, -1)) / 100;
    } else {
      a = Number(match[4]);
    }
    if (!Number.isFinite(a) || a < 0 || a > 1) {
      return null;
    }
  }

  return { r, g, b, a };
}

function toHexByte(value: number): string {
  return Math.max(0, Math.min(255, Math.round(value)))
    .toString(16)
    .padStart(2, '0');
}

function formatAlpha(alpha: number): string {
  return Number(alpha.toFixed(2)).toString();
}

function rgbaToHex({ r, g, b, a }: ParsedRgba): string {
  const base = `#${toHexByte(r)}${toHexByte(g)}${toHexByte(b)}`;
  if (a >= 0.999) {
    return base;
  }
  return `${base}${toHexByte(a * 255)}`;
}

function rgbaToRgbString({ r, g, b, a }: ParsedRgba): string {
  if (a >= 0.999) {
    return `rgb(${r}, ${g}, ${b})`;
  }
  return `rgba(${r}, ${g}, ${b}, ${formatAlpha(a)})`;
}

function rgbaToHslString({ r, g, b, a }: ParsedRgba): string {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const delta = max - min;
  const l = (max + min) / 2;

  let h = 0;
  let s = 0;

  if (delta !== 0) {
    s = delta / (1 - Math.abs(2 * l - 1));
    if (max === rn) {
      h = ((gn - bn) / delta) % 6;
    } else if (max === gn) {
      h = (bn - rn) / delta + 2;
    } else {
      h = (rn - gn) / delta + 4;
    }
    h *= 60;
    if (h < 0) {
      h += 360;
    }
  }

  const hDeg = Math.round(h);
  const sPct = Math.round(s * 100);
  const lPct = Math.round(l * 100);

  if (a >= 0.999) {
    return `hsl(${hDeg}, ${sPct}%, ${lPct}%)`;
  }
  return `hsla(${hDeg}, ${sPct}%, ${lPct}%, ${formatAlpha(a)})`;
}

export function hexToRgbString(hex: string): string | null {
  const parsed = parseHexColor(hex.trim());
  return parsed ? rgbaToRgbString(parsed) : null;
}

export function hexToHslString(hex: string): string | null {
  const parsed = parseHexColor(hex.trim());
  return parsed ? rgbaToHslString(parsed) : null;
}

/**
 * Detects whether `text` represents a single CSS color literal (`#RGB`, `#RGBA`,
 * `#RRGGBB`, `#RRGGBBAA`, `rgb(...)`, or `rgba(...)`) and formats Hex/RGB/HSL strings.
 */
export function detectColorSwatch(text: string): ColorSwatch | null {
  const trimmed = text.trim();
  if (!trimmed || trimmed.length > 48) {
    return null;
  }

  const parsed = parseHexColor(trimmed) ?? parseRgbFunction(trimmed);
  if (!parsed) {
    return null;
  }

  const hex = rgbaToHex(parsed);
  const rgb = rgbaToRgbString(parsed);
  const hsl = rgbaToHslString(parsed);

  return {
    raw: trimmed,
    hex,
    rgb,
    hsl,
    cssColor: rgb,
  };
}

/**
 * Builds a `clipoimg://localhost/...` URL matching `resolve_clipoimg_blob` in `windows_mgr.rs`.
 */
export function buildClipoImgUrl(relPath: string | null | undefined): string | null {
  if (!relPath) {
    return null;
  }
  const normalized = relPath
    .split('/')
    .map((segment) => segment.trim())
    .filter((segment) => segment.length > 0)
    .map((segment) => encodeURIComponent(segment))
    .join('/');

  if (!normalized) {
    return null;
  }

  return `clipoimg://localhost/${normalized}`;
}
