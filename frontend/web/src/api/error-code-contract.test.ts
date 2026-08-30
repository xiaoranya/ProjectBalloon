import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

// CI guard: every business code the API can emit must be covered in client.ts
// or explicitly registered in the allowlist as a code that intentionally falls
// back to the raw server message. A code is covered when client.ts maps it by
// code in `businessMessages`, or when every static server message the backend
// emits for the code has a Chinese translation in `serverMessageTranslations`
// (for codes whose message varies by context). A new backend code turns this
// test red until it lands in one of the two lists.
function findApiSrcDir(): string {
  let dir = process.cwd();
  for (let depth = 0; depth < 6; depth += 1) {
    const candidate = join(dir, 'apps', 'api', 'src');
    if (existsSync(candidate)) return candidate;
    dir = dirname(dir);
  }
  throw new Error('apps/api/src not found above the working directory');
}

const apiSrcDir = findApiSrcDir();
const clientTsPath = join(apiSrcDir, '../../../frontend/web/src/api/client.ts');
const allowlistPath = join(apiSrcDir, '../../../frontend/web/src/api/unknown-codes.allowlist.json');

const codeConstructorPattern =
  /(?:AppError::)?(?:bad_request|unauthorized|forbidden|not_found|conflict|too_many_requests|service_unavailable)\(\s*"([A-Z][A-Z0-9_]+)"/g;
// Fixed codes baked into error.rs response bodies.
const fixedCodePattern = /code:\s*Cow::Borrowed\("([A-Z][A-Z0-9_]+)"\)/g;
// The same call sites with the static server message captured as well.
const codeMessageConstructorPattern =
  /(?:AppError::)?(?:bad_request|unauthorized|forbidden|not_found|conflict|too_many_requests|service_unavailable)\(\s*"([A-Z][A-Z0-9_]+)"\s*,\s*"([^"]+)"/g;
const fixedCodeMessagePattern =
  /code:\s*Cow::Borrowed\("([A-Z][A-Z0-9_]+)"\)\s*,\s*message:\s*Cow::Borrowed\("([^"]+)"\)/g;

function collectRustFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      files.push(...collectRustFiles(full));
    } else if (entry.endsWith('.rs')) {
      files.push(full);
    }
  }
  return files;
}

function extractBackendCodes(): Set<string> {
  const codes = new Set<string>();
  for (const file of collectRustFiles(apiSrcDir)) {
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(codeConstructorPattern)) {
      codes.add(match[1]);
    }
    for (const match of text.matchAll(fixedCodePattern)) {
      codes.add(match[1]);
    }
  }
  return codes;
}

function extractBackendCodeMessages(): Map<string, Set<string>> {
  const messages = new Map<string, Set<string>>();
  const add = (code: string, message: string) => {
    if (!messages.has(code)) messages.set(code, new Set());
    messages.get(code)!.add(message);
  };
  for (const file of collectRustFiles(apiSrcDir)) {
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(codeMessageConstructorPattern)) {
      add(match[1], match[2]);
    }
    for (const match of text.matchAll(fixedCodeMessagePattern)) {
      add(match[1], match[2]);
    }
  }
  return messages;
}

function extractClientTranslations(): { codeKeys: Set<string>; serverMessageKeys: Set<string> } {
  const text = readFileSync(clientTsPath, 'utf8');
  const block = (name: string): string => {
    const start = text.indexOf(`const ${name}`);
    if (start === -1) return '';
    return text.slice(start, text.indexOf('};', start));
  };
  const codeKeys = new Set(
    [...block('businessMessages').matchAll(/^\s{2}([A-Z][A-Z0-9_]+):/gm)].map((m) => m[1]),
  );
  const serverMessageKeys = new Set(
    [...block('serverMessageTranslations').matchAll(/^\s{2}'([^']+)':/gm)].map((m) => m[1]),
  );
  return { codeKeys, serverMessageKeys };
}

describe('API business code contract', () => {
  it('maps or allowlists every backend business code', () => {
    const backendCodes = extractBackendCodes();
    expect(backendCodes.size).toBeGreaterThan(100);

    const { codeKeys, serverMessageKeys } = extractClientTranslations();
    const codeMessages = extractBackendCodeMessages();
    // The allowlist file is either a bare array of codes or an object carrying
    // the triage note plus an `entries` array (same shape as the type-diff
    // allowlist in docs/api).
    const parsed = JSON.parse(readFileSync(allowlistPath, 'utf8')) as
      string[] | { entries: string[] };
    const allowlist = new Set(Array.isArray(parsed) ? parsed : parsed.entries);

    const covered = (code: string): boolean =>
      codeKeys.has(code) ||
      (codeMessages.get(code) !== undefined &&
        codeMessages.get(code)!.size > 0 &&
        [...codeMessages.get(code)!].every((message) => serverMessageKeys.has(message)));

    const unknown = [...backendCodes].filter((code) => !covered(code) && !allowlist.has(code));
    expect(
      unknown,
      'new backend codes must be mapped in client.ts or added to the allowlist',
    ).toEqual([]);

    const staleAllowlist = [...allowlist].filter(
      (code) => covered(code) || !backendCodes.has(code),
    );
    expect(
      staleAllowlist,
      'allowlist entries that are now mapped or no longer emitted must be removed',
    ).toEqual([]);
  });
});
