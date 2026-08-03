import { afterEach } from 'vitest';

// Node's fetch implementation expects its own Blob implementation when a
// Response is constructed with a binary body. jsdom installs a separate Blob
// global, which makes the same tests fail on Node 22 with `object.stream is not
// a function`. Keep the fetch and Blob implementations from the same realm.
const nodeProcess = (
  globalThis as typeof globalThis & {
    process?: { getBuiltinModule?: (name: string) => { Blob?: typeof Blob } };
  }
).process;
const nodeBlob = nodeProcess?.getBuiltinModule?.('buffer')?.Blob;
if (nodeBlob) globalThis.Blob = nodeBlob;

afterEach(() => {
  document.body.innerHTML = '';
});
