import { describe, expect, it, vi } from 'vitest';

import type { BalloonTaskStatus } from '../../api/balloons';
import { statusLabel, statusType } from './status';

const identity = (key: string) => key;

describe('statusLabel', () => {
  it('maps every balloon status through the translation function', () => {
    expect(statusLabel(identity, 'PENDING')).toBe('待领取');
    expect(statusLabel(identity, 'CLAIMED')).toBe('配送中');
    expect(statusLabel(identity, 'DELIVERED')).toBe('已送达');
    expect(statusLabel(identity, 'CANCELLED')).toBe('已取消');
  });

  it('delegates the lookup to the caller-provided translator', () => {
    const t = vi.fn((key: string) => `T:${key}`);
    expect(statusLabel(t, 'DELIVERED')).toBe('T:已送达');
    expect(t).toHaveBeenCalledWith('已送达');
  });
});

describe('statusType', () => {
  it.each<[BalloonTaskStatus, 'success' | 'warning' | 'danger']>([
    ['PENDING', 'warning'],
    ['CLAIMED', 'warning'],
    ['DELIVERED', 'success'],
    ['CANCELLED', 'danger'],
  ])('renders %s as %s', (status, expected) => {
    expect(statusType(status)).toBe(expected);
  });
});
