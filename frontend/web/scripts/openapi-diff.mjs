// Compares the shared entity fields between the OpenAPI-generated types
// (src/api/types.generated.ts, produced by `npm run openapi:gen`) and the
// hand-maintained src/api/types.ts. Differences outside
// docs/api/type-diff-allowlist.json fail the run (used with --check in CI);
// pass the diff list to a human before registering entries in the allowlist.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const webRoot = dirname(scriptDir);
const repoRoot = dirname(dirname(webRoot));
const checkMode = process.argv.includes('--check');

const generatedPath = join(webRoot, 'src', 'api', 'types.generated.ts');
const handPath = join(webRoot, 'src', 'api', 'types.ts');
const allowlistPath = join(repoRoot, 'docs', 'api', 'type-diff-allowlist.json');

// Extracts `Name: { ... }` blocks from the generated components.schemas
// section and `export interface Name { ... }` blocks from the hand-written
// file, returning Map<name, Set<fieldName>>. Field extraction only needs the
// property names, so scanning top-level `identifier?:` lines is sufficient.
function extractSchemaFields(text, startMarker, endMarker, namePattern) {
  const start = text.indexOf(startMarker);
  if (start === -1) throw new Error(`marker not found: ${startMarker}`);
  const end = text.indexOf(endMarker, start);
  const section = end === -1 ? text.slice(start) : text.slice(start, end);
  const fields = new Map();
  const lines = section.split('\n');
  let current = null;
  let depth = 0;
  for (const line of lines) {
    const opens = (line.match(/{/g) ?? []).length;
    const closes = (line.match(/}/g) ?? []).length;
    const nameMatch = depth === 0 ? namePattern.exec(line) : null;
    if (nameMatch) {
      current = nameMatch[1];
      fields.set(current, new Set());
      // The entity header itself may open braces (e.g. `... & {`).
      depth = opens - closes;
      continue;
    }
    if (current === null) continue;
    const fieldMatch = /^\s+([A-Za-z][A-Za-z0-9]*)\??:/.exec(line);
    if (fieldMatch && depth === 1) fields.get(current).add(fieldMatch[1]);
    depth += opens - closes;
    if (depth <= 0) {
      depth = 0;
      current = null;
    }
  }
  return fields;
}

const generatedText = readFileSync(generatedPath, 'utf8');
const handText = readFileSync(handPath, 'utf8');

const generated = extractSchemaFields(
  generatedText,
  'export interface components {',
  'export type $defs',
  /^\s{8}([A-Za-z][A-Za-z0-9]*):/,
);
const hand = new Map();
{
  const interfacePattern = /^export interface ([A-Za-z][A-Za-z0-9]*) \{/gm;
  const lines = handText.split('\n');
  let current = null;
  for (const line of lines) {
    interfacePattern.lastIndex = 0;
    const nameMatch = interfacePattern.exec(line);
    if (nameMatch) {
      current = nameMatch[1];
      hand.set(current, new Set());
      continue;
    }
    if (current === null) continue;
    if (line === '}') {
      current = null;
      continue;
    }
    const fieldMatch = /^\s{2}([A-Za-z][A-Za-z0-9]*)\??:/.exec(line);
    if (fieldMatch) hand.get(current).add(fieldMatch[1]);
  }
}

const shared = [...generated.keys()].filter((name) => hand.has(name));
const onlyGenerated = [...generated.keys()].filter((name) => !hand.has(name));
const onlyHand = [...hand.keys()].filter((name) => !generated.has(name));

const allowlist = new Set(JSON.parse(readFileSync(allowlistPath, 'utf8')).entries);
const differences = [];
for (const name of shared.sort()) {
  const generatedFields = generated.get(name);
  const handFields = hand.get(name);
  for (const field of [...generatedFields].filter((f) => !handFields.has(f)).sort()) {
    differences.push({ entity: name, kind: 'field-only-in-openapi', field });
  }
  for (const field of [...handFields].filter((f) => !generatedFields.has(f)).sort()) {
    differences.push({ entity: name, kind: 'field-only-in-types', field });
  }
}

const untriaged = differences.filter((d) => !allowlist.has(`${d.entity}.${d.field}`));
const staleAllowlist = [...allowlist].filter((key) => {
  const [entity, field] = key.split('.');
  return !differences.some((d) => d.entity === entity && (field === '*' || d.field === field));
});

console.log(`shared entities: ${shared.length}`);
console.log(`openapi-only entities: ${onlyGenerated.length}`);
console.log(`types.ts-only entities: ${onlyHand.length}`);
console.log(
  `field differences: ${differences.length} (allowlisted: ${differences.length - untriaged.length})`,
);
for (const d of untriaged) console.log(`  UNTRIAGED ${d.entity}.${d.field} (${d.kind})`);
for (const d of differences.filter((d) => allowlist.has(`${d.entity}.${d.field}`))) {
  console.log(`  allowlisted ${d.entity}.${d.field} (${d.kind})`);
}
if (onlyGenerated.length > 0) console.log(`openapi-only: ${onlyGenerated.join(', ')}`);
if (onlyHand.length > 0) console.log(`types.ts-only: ${onlyHand.join(', ')}`);
if (staleAllowlist.length > 0) console.log(`stale allowlist entries: ${staleAllowlist.join(', ')}`);

if (checkMode && (untriaged.length > 0 || staleAllowlist.length > 0)) {
  console.error('openapi:diff --check failed: untriaged or stale differences above');
  process.exit(1);
}
