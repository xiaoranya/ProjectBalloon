import { afterEach } from 'vitest';
import { Blob as NodeBlob } from 'node:buffer';

// Node's fetch implementation expects its own Blob implementation when a
// Response is constructed with a binary body. jsdom installs a separate Blob
// global, which makes the same tests fail on Node 22 with `object.stream is not
// a function`. Keep the fetch and Blob implementations from the same realm.
globalThis.Blob = NodeBlob;

afterEach(() => {
  document.body.innerHTML = '';
});
