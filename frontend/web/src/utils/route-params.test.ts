import { describe, expect, it } from 'vitest';

import { numericQueryId } from './route-params';

describe('numericQueryId', () => {
  it('parses single string and array values', () => {
    expect(numericQueryId('7')).toBe(7);
    expect(numericQueryId(['7'])).toBe(7);
    expect(numericQueryId('042')).toBe(42);
  });

  it('normalizes duplicate query params to the first value', () => {
    expect(numericQueryId(['7', '8'])).toBe(7);
  });

  it('rejects missing, malformed, and non-positive values', () => {
    expect(numericQueryId(undefined)).toBeNull();
    expect(numericQueryId('')).toBeNull();
    expect(numericQueryId('abc')).toBeNull();
    expect(numericQueryId([])).toBeNull();
    expect(numericQueryId('0')).toBeNull();
    expect(numericQueryId('-3')).toBeNull();
    expect(numericQueryId('1.5')).toBeNull();
  });
});
