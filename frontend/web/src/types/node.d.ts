// Minimal ambient declarations for the node builtins used by tests that run
// under vitest's jsdom environment (e.g. the API error-code contract test).
// The project does not depend on @types/node; extend this file rather than
// adding that dependency.
declare module 'node:fs' {
  export function readdirSync(path: string): string[];
  export function readFileSync(path: string, encoding: 'utf8'): string;
  export function statSync(path: string): { isDirectory(): boolean };
  export function existsSync(path: string): boolean;
}

declare module 'node:path' {
  export function dirname(path: string): string;
  export function join(...segments: string[]): string;
}

declare const process: {
  cwd(): string;
  env: Record<string, string | undefined>;
};
