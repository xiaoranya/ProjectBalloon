import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { baseParse } from '@vue/compiler-dom';
import { parse as parseSfc } from 'vue/compiler-sfc';
import { describe, expect, it } from 'vitest';

// i18n guard: the app's translation keys are the Chinese strings themselves
// (t('暂无题面')), so raw Chinese text in a template means a literal that will
// never be translated. Visible strings must go through an interpolation
// ({{ t('…') }}); attribute values and script/style content are out of scope.
// A new hardcoded CJK text node turns this test red until it is routed
// through i18n or explicitly registered in the allowlist.

const CJK = /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]/;

function findVueFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) files.push(...findVueFiles(full));
    else if (entry.endsWith('.vue')) files.push(full);
  }
  return files;
}

function findSrcDir(): string {
  let dir = process.cwd();
  for (let depth = 0; depth < 6; depth += 1) {
    if (
      statSync(join(dir, 'src')).isDirectory() &&
      statSync(join(dir, 'src', 'views')).isDirectory()
    ) {
      return join(dir, 'src');
    }
    dir = dirname(dir);
  }
  throw new Error('frontend src directory not found above the working directory');
}

interface Violation {
  file: string;
  line: number;
  excerpt: string;
}

interface TemplateNode {
  type: number;
  content?: unknown;
  children?: TemplateNode[];
  loc?: { start?: { line?: number } };
}

function scanFile(webRoot: string, absolutePath: string): Violation[] {
  const source = readFileSync(absolutePath, 'utf8');
  const { descriptor, errors } = parseSfc(source, { filename: absolutePath });
  if (errors.length > 0 || descriptor.template === null) return [];
  // baseParse yields the raw pre-transform AST: TEXT nodes (type 2) are the
  // original literal text children, before Vue compiles them away.
  const ast = baseParse(descriptor.template.content);
  // The template block's first line inside the .vue file.
  const lineOffset = descriptor.template.loc.start.line - 1;
  const file = absolutePath.slice(webRoot.length + 1);
  const violations: Violation[] = [];
  const walk = (node: TemplateNode) => {
    if (node.type === 2 && typeof node.content === 'string') {
      const text = node.content.trim();
      if (text && CJK.test(text)) {
        violations.push({
          file,
          line: lineOffset + (node.loc?.start?.line ?? 1),
          excerpt: text.slice(0, 40),
        });
      }
    }
    for (const child of node.children ?? []) walk(child);
  };
  walk(ast as unknown as TemplateNode);
  return violations;
}

describe('i18n template literal guard', () => {
  it('has no raw CJK text nodes outside the allowlist', () => {
    const srcDir = findSrcDir();
    const webRoot = dirname(srcDir);
    const files = [
      ...findVueFiles(join(srcDir, 'views')),
      ...findVueFiles(join(srcDir, 'components')),
    ];
    expect(files.length).toBeGreaterThan(30);

    const violations: Violation[] = [];
    for (const file of files) violations.push(...scanFile(webRoot, file));

    const allowlistPath = join(srcDir, 'i18n', 'template-literal-allowlist.json');
    const allowlist = JSON.parse(readFileSync(allowlistPath, 'utf8')) as Violation[];

    const key = (v: Violation) => `${v.file}:${v.line}:${v.excerpt}`;
    const allowlisted = new Set(allowlist.map(key));
    const unexpected = violations.filter((v) => !allowlisted.has(key(v)));
    const stale = allowlist.filter((entry) => !violations.some((v) => key(v) === key(entry)));

    expect(
      unexpected.map(key),
      'new hardcoded CJK template text must go through t(...) or be allowlisted',
    ).toEqual([]);
    expect(
      stale.map((entry) => key(entry)),
      'allowlist entries whose violations are gone must be removed',
    ).toEqual([]);
  });
});
