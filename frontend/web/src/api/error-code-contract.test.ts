import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

// CI guard: every business code the API can emit must be either translated in
// client.ts `businessMessages` or explicitly registered in the allowlist as a
// code that intentionally falls back to the raw server message. A new backend
// code turns this test red until it lands in one of the two lists.
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

function extractMappedCodes(): Set<string> {
  const text = readFileSync(clientTsPath, 'utf8');
  const businessMessages = text.slice(
    text.indexOf('const businessMessages'),
    text.indexOf('};', text.indexOf('const businessMessages')),
  );
  return new Set([...businessMessages.matchAll(/^\s{2}([A-Z][A-Z0-9_]+):/gm)].map((m) => m[1]));
}

describe('API business code contract', () => {
  it('maps or allowlists every backend business code', () => {
    const backendCodes = extractBackendCodes();
    expect(backendCodes.size).toBeGreaterThan(100);

    const mapped = extractMappedCodes();
    const allowlist = new Set(JSON.parse(readFileSync(allowlistPath, 'utf8')) as string[]);

    const unknown = [...backendCodes].filter((code) => !mapped.has(code) && !allowlist.has(code));
    expect(
      unknown,
      'new backend codes must be mapped in client.ts or added to the allowlist',
    ).toEqual([]);

    const staleAllowlist = [...allowlist].filter(
      (code) => mapped.has(code) || !backendCodes.has(code),
    );
    expect(
      staleAllowlist,
      'allowlist entries that are now mapped or no longer emitted must be removed',
    ).toEqual([]);
  });
});
