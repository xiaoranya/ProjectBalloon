/**
 * Browser helpers that survive a non-secure context.
 *
 * The project's headline deployment is an offline LAN reached over plain HTTP
 * (`http://192.168.x.x`), which is *not* a secure context. There
 * `crypto.randomUUID` and `navigator.clipboard` are `undefined`: the first
 * threw a `TypeError` that blanked whole views, the second silently did
 * nothing. Both helpers keep the modern API when it exists and fall back
 * otherwise.
 */

/** RFC 4122 version 4 identifier built from `crypto.getRandomValues`. */
export function randomId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  const bytes = new Uint8Array(16);
  if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
    crypto.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Math.floor(Math.random() * 256);
    }
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20),
  ].join('-');
}

/**
 * Copies `text` to the clipboard. Returns `false` when neither the async
 * clipboard API nor the legacy `execCommand` path worked, so the caller can
 * tell the user to copy manually instead of pretending it succeeded.
 */
export async function copyText(text: string): Promise<boolean> {
  if (typeof navigator !== 'undefined' && typeof navigator.clipboard?.writeText === 'function') {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Permission denied or a detached document: fall through to the fallback.
    }
  }
  if (typeof document === 'undefined') return false;
  const holder = document.createElement('textarea');
  holder.value = text;
  holder.setAttribute('readonly', '');
  holder.style.position = 'fixed';
  holder.style.top = '-1000px';
  holder.style.opacity = '0';
  document.body.appendChild(holder);
  holder.select();
  holder.setSelectionRange(0, text.length);
  try {
    return document.execCommand('copy');
  } catch {
    return false;
  } finally {
    document.body.removeChild(holder);
  }
}
