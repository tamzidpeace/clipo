export type SupportedLanguage =
  | 'json'
  | 'bash'
  | 'sql'
  | 'typescript'
  | 'plaintext';

export type TokenKind =
  | 'plain'
  | 'keyword'
  | 'string'
  | 'number'
  | 'comment'
  | 'operator'
  | 'key'
  | 'boolean'
  | 'variable';

export interface SyntaxToken {
  text: string;
  kind: TokenKind;
}

const TS_KEYWORDS = new Set([
  'import',
  'from',
  'export',
  'default',
  'const',
  'let',
  'var',
  'function',
  'return',
  'if',
  'else',
  'for',
  'while',
  'switch',
  'case',
  'break',
  'continue',
  'async',
  'await',
  'try',
  'catch',
  'finally',
  'throw',
  'new',
  'class',
  'extends',
  'implements',
  'interface',
  'type',
  'enum',
  'typeof',
  'keyof',
  'as',
  'in',
  'of',
  'void',
]);

const BASH_KEYWORDS = new Set([
  'if',
  'then',
  'else',
  'elif',
  'fi',
  'for',
  'in',
  'do',
  'done',
  'while',
  'case',
  'esac',
  'function',
  'export',
  'local',
  'readonly',
  'echo',
  'printf',
  'cd',
  'ls',
  'grep',
  'sed',
  'awk',
  'curl',
  'git',
  'npm',
  'npx',
  'cargo',
  'sudo',
  'chmod',
  'mkdir',
  'rm',
  'cat',
  'exit',
  'return',
]);

const SQL_KEYWORDS = new Set([
  'select',
  'from',
  'where',
  'insert',
  'into',
  'values',
  'update',
  'set',
  'delete',
  'create',
  'alter',
  'drop',
  'table',
  'index',
  'join',
  'inner',
  'left',
  'right',
  'on',
  'group',
  'by',
  'order',
  'having',
  'limit',
  'offset',
  'and',
  'or',
  'not',
  'as',
  'distinct',
  'with',
  'union',
  'is',
  'like',
  'in',
  'exists',
  'case',
  'when',
  'then',
  'else',
  'end',
]);

const BOOLEANS = new Set(['true', 'false', 'null', 'undefined']);

function normalizeLanguageHint(hint?: string | null): SupportedLanguage | null {
  if (!hint) {
    return null;
  }
  const lower = hint.trim().toLowerCase();
  if (!lower) {
    return null;
  }
  if (lower === 'json' || lower === 'jsonc') {
    return 'json';
  }
  if (
    lower === 'bash' ||
    lower === 'sh' ||
    lower === 'shell' ||
    lower === 'zsh'
  ) {
    return 'bash';
  }
  if (lower === 'sql' || lower === 'sqlite' || lower === 'postgres') {
    return 'sql';
  }
  if (
    lower === 'ts' ||
    lower === 'tsx' ||
    lower === 'typescript' ||
    lower === 'js' ||
    lower === 'jsx' ||
    lower === 'javascript'
  ) {
    return 'typescript';
  }
  if (lower === 'text' || lower === 'plain' || lower === 'plaintext') {
    return 'plaintext';
  }
  return null;
}

export function detectLanguage(
  text: string,
  hint?: string | null,
): SupportedLanguage {
  const hinted = normalizeLanguageHint(hint);
  if (hinted) {
    return hinted;
  }

  const trimmed = text.trim();
  if (!trimmed) {
    return 'plaintext';
  }

  if (
    (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
    (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    try {
      JSON.parse(trimmed);
      return 'json';
    } catch {
      // Fall through to heuristic detection
    }
  }

  if (
    /^#!\s*\/(?:usr\/bin\/env\s+)?(?:ba|z|da)?sh\b/m.test(trimmed) ||
    /(?:^|\n)\s*(?:export\s+[A-Z_][A-Z0-9_]*=|echo\s+|curl\s+|sudo\s+|chmod\s+|git\s+|cargo\s+|npm\s+)/m.test(
      trimmed,
    ) ||
    /\$\([a-zA-Z0-9_.-]+|\|\s*grep\b/.test(trimmed)
  ) {
    return 'bash';
  }

  if (
    /\b(?:SELECT\b[\s\S]+\bFROM|INSERT\s+INTO|UPDATE\b[\s\S]+\bSET|DELETE\s+FROM|CREATE\s+TABLE|ALTER\s+TABLE)\b/i.test(
      trimmed,
    )
  ) {
    return 'sql';
  }

  if (
    /\b(?:import\s+[\s\S]+\s+from\s+['"]|export\s+(?:default|const|function|interface|type|class)\b|interface\s+[A-Za-z_]|const\s+[A-Za-z_]|let\s+[A-Za-z_]|function\s+[A-Za-z_])/.test(
      trimmed,
    ) ||
    /=>|:\s*(?:string|number|boolean|unknown|void)\b/.test(trimmed)
  ) {
    return 'typescript';
  }

  return 'plaintext';
}

export function tryPrettyPrintJson(text: string): {
  formatted: string;
  isValidJson: boolean;
} {
  const trimmed = text.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) {
    return { formatted: text, isValidJson: false };
  }
  try {
    const parsed = JSON.parse(trimmed);
    return {
      formatted: JSON.stringify(parsed, null, 2),
      isValidJson: true,
    };
  } catch {
    return { formatted: text, isValidJson: false };
  }
}

function readQuoted(text: string, start: number, quote: string): number {
  let i = start + 1;
  while (i < text.length) {
    const ch = text[i];
    if (ch === '\\') {
      i += 2;
      continue;
    }
    if (ch === quote) {
      return i + 1;
    }
    if (quote !== '`' && ch === '\n') {
      return i;
    }
    i += 1;
  }
  return text.length;
}

function isWordStart(ch: string): boolean {
  return /[A-Za-z_$]/.test(ch);
}

function isWordPart(ch: string): boolean {
  return /[A-Za-z0-9_$]/.test(ch);
}

function pushToken(tokens: SyntaxToken[], kind: TokenKind, text: string): void {
  if (!text) {
    return;
  }
  const prev = tokens[tokens.length - 1];
  if (prev && prev.kind === kind) {
    prev.text += text;
  } else {
    tokens.push({ kind, text });
  }
}

export function tokenizeCode(
  text: string,
  languageHint?: SupportedLanguage | string | null,
): { language: SupportedLanguage; tokens: SyntaxToken[] } {
  const language = detectLanguage(text, languageHint);
  if (!text) {
    return { language, tokens: [] };
  }
  if (language === 'plaintext') {
    return { language, tokens: [{ kind: 'plain', text }] };
  }

  const tokens: SyntaxToken[] = [];
  let i = 0;

  while (i < text.length) {
    const ch = text[i];
    const next = text[i + 1] ?? '';

    // Comments
    if (language === 'typescript' && ch === '/' && next === '/') {
      const nl = text.indexOf('\n', i);
      const end = nl === -1 ? text.length : nl;
      pushToken(tokens, 'comment', text.slice(i, end));
      i = end;
      continue;
    }

    if (language === 'typescript' && ch === '/' && next === '*') {
      const close = text.indexOf('*/', i + 2);
      const end = close === -1 ? text.length : close + 2;
      pushToken(tokens, 'comment', text.slice(i, end));
      i = end;
      continue;
    }

    if (language === 'bash' && ch === '#') {
      const nl = text.indexOf('\n', i);
      const end = nl === -1 ? text.length : nl;
      pushToken(tokens, 'comment', text.slice(i, end));
      i = end;
      continue;
    }

    if (language === 'sql' && ch === '-' && next === '-') {
      const nl = text.indexOf('\n', i);
      const end = nl === -1 ? text.length : nl;
      pushToken(tokens, 'comment', text.slice(i, end));
      i = end;
      continue;
    }

    // Strings & JSON keys
    if (
      ch === '"' ||
      ch === "'" ||
      (language === 'typescript' && ch === '`')
    ) {
      const end = readQuoted(text, i, ch);
      const slice = text.slice(i, end);
      if (language === 'json' && ch === '"') {
        let j = end;
        while (j < text.length && (text[j] === ' ' || text[j] === '\t')) {
          j += 1;
        }
        pushToken(tokens, text[j] === ':' ? 'key' : 'string', slice);
      } else {
        pushToken(tokens, 'string', slice);
      }
      i = end;
      continue;
    }

    // Bash variables ($VAR or ${VAR})
    if (language === 'bash' && ch === '$') {
      if (next === '{') {
        const close = text.indexOf('}', i + 2);
        const end = close === -1 ? text.length : close + 1;
        pushToken(tokens, 'variable', text.slice(i, end));
        i = end;
        continue;
      }
      if (/[A-Za-z_0-9@*?#]/.test(next)) {
        let end = i + 2;
        while (end < text.length && /[A-Za-z0-9_]/.test(text[end])) {
          end += 1;
        }
        pushToken(tokens, 'variable', text.slice(i, end));
        i = end;
        continue;
      }
    }

    // Numbers
    if (/[0-9]/.test(ch)) {
      let end = i + 1;
      while (end < text.length && /[0-9._xXa-fA-FeE+-]/.test(text[end])) {
        if ((text[end] === '+' || text[end] === '-') && !/[eE]/.test(text[end - 1])) {
          break;
        }
        end += 1;
      }
      pushToken(tokens, 'number', text.slice(i, end));
      i = end;
      continue;
    }

    // Identifiers & keywords
    if (isWordStart(ch)) {
      let end = i + 1;
      while (end < text.length && isWordPart(text[end])) {
        end += 1;
      }
      const word = text.slice(i, end);
      const lower = word.toLowerCase();

      if (BOOLEANS.has(lower) && (language !== 'sql' || lower === 'null' || lower === 'true' || lower === 'false')) {
        pushToken(tokens, 'boolean', word);
      } else if (language === 'typescript' && TS_KEYWORDS.has(word)) {
        pushToken(tokens, 'keyword', word);
      } else if (language === 'bash' && BASH_KEYWORDS.has(word)) {
        pushToken(tokens, 'keyword', word);
      } else if (language === 'sql' && SQL_KEYWORDS.has(lower)) {
        pushToken(tokens, 'keyword', word);
      } else {
        pushToken(tokens, 'plain', word);
      }
      i = end;
      continue;
    }

    // Operators & punctuation
    if (/[{}[\]():.,;=<>+\-*/%!&|^~?]/.test(ch)) {
      pushToken(tokens, 'operator', ch);
      i += 1;
      continue;
    }

    // Whitespace & other characters
    let end = i + 1;
    while (
      end < text.length &&
      !/[A-Za-z0-9_$"'`#/\\\-{}[\]():.,;=<>+\-*/%!&|^~?]/.test(text[end])
    ) {
      end += 1;
    }
    pushToken(tokens, 'plain', text.slice(i, end));
    i = end;
  }

  return { language, tokens };
}
