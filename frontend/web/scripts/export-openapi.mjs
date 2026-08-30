// Exports the runtime OpenAPI document from the Rust implementation into
// docs/api/openapi.runtime.json. The legacy docs/api/openapi.yaml is a frozen
// Java-era compatibility baseline consumed by scripts/check-api-compat.py and
// must not be used as the TypeScript type source.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const webRoot = dirname(scriptDir);

function findRepoRoot(dir) {
  let current = dir;
  for (let depth = 0; depth < 6; depth += 1) {
    if (existsSync(join(current, 'Cargo.toml'))) return current;
    current = dirname(current);
  }
  throw new Error('Cargo.toml not found above the working directory');
}

const repoRoot = findRepoRoot(webRoot);
const outputPath = join(repoRoot, 'docs', 'api', 'openapi.runtime.json');

function hasCargo() {
  try {
    execFileSync('cargo', ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

let generated;
if (hasCargo()) {
  generated = execFileSync(
    'cargo',
    ['run', '--quiet', '-p', 'project-balloon-api', '--bin', 'export-openapi'],
    { cwd: repoRoot, encoding: 'utf8' },
  );
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, generated);
  console.log(`openapi.runtime.json regenerated at ${outputPath}`);
} else {
  // Environments without the Rust toolchain (e.g. the frontend CI job) reuse
  // the committed runtime document; `openapi:diff --check` still guards the
  // generated TypeScript against drift from src/api/types.ts.
  generated = readFileSync(outputPath, 'utf8');
  console.log(`cargo not found; reusing committed ${outputPath}`);
}
JSON.parse(generated);
