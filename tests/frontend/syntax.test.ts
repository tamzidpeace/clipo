import { describe, expect, it } from 'vitest';
import {
  detectLanguage,
  tokenizeCode,
  tryPrettyPrintJson,
} from '../../src/lib/syntax';

describe('syntax utilities', () => {
  it('detects JSON, Bash, SQL, and TypeScript/JS snippets', () => {
    expect(detectLanguage('{"name":"clipo","count":3}')).toBe('json');
    expect(detectLanguage('#!/usr/bin/env bash\necho "hi" | grep hi')).toBe('bash');
    expect(detectLanguage('SELECT id, title FROM snippets WHERE is_pinned = 1;')).toBe('sql');
    expect(
      detectLanguage("import { invoke } from '@tauri-apps/api/core';\nconst x: number = 42;"),
    ).toBe('typescript');
    expect(detectLanguage('Just some plain clipboard note.')).toBe('plaintext');
  });

  it('respects explicit language hints when provided', () => {
    expect(detectLanguage('echo hello', 'bash')).toBe('bash');
    expect(detectLanguage('const a = 1', 'ts')).toBe('typescript');
    expect(detectLanguage('select 1', 'SQL')).toBe('sql');
  });

  it('tokenizes code losslessly into highlighted spans', () => {
    const sample = '{"ok": true, "n": 42}';
    const { language, tokens } = tokenizeCode(sample);
    expect(language).toBe('json');
    expect(tokens.map((t) => t.text).join('')).toBe(sample);
    expect(tokens.some((t) => t.kind === 'key' && t.text === '"ok"')).toBe(true);
    expect(tokens.some((t) => t.kind === 'boolean' && t.text === 'true')).toBe(true);
    expect(tokens.some((t) => t.kind === 'number' && t.text === '42')).toBe(true);

    const tsSample = "const msg = 'hello'; // note";
    const tsResult = tokenizeCode(tsSample, 'typescript');
    expect(tsResult.tokens.map((t) => t.text).join('')).toBe(tsSample);
    expect(tsResult.tokens.some((t) => t.kind === 'keyword' && t.text === 'const')).toBe(true);
    expect(tsResult.tokens.some((t) => t.kind === 'string')).toBe(true);
    expect(tsResult.tokens.some((t) => t.kind === 'comment')).toBe(true);
  });

  it('pretty-prints valid JSON and leaves malformed JSON untouched without throwing', () => {
    const raw = '{"b":2,"a":{"c":[1,2]}}';
    const valid = tryPrettyPrintJson(raw);
    expect(valid.isValidJson).toBe(true);
    expect(valid.formatted).toBe('{\n  "b": 2,\n  "a": {\n    "c": [\n      1,\n      2\n    ]\n  }\n}');

    const malformed = '{"b": 2, "a": [1, 2}';
    const invalid = tryPrettyPrintJson(malformed);
    expect(invalid.isValidJson).toBe(false);
    expect(invalid.formatted).toBe(malformed);
  });
});
