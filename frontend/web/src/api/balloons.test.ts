import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { balloonApi } from './balloons';

vi.mock('./client', () => ({ apiRequest: vi.fn() }));

describe('balloonApi', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());

  it('uses contest-scoped list and statistics endpoints', async () => {
    await balloonApi.list(7, 'CLAIMED');
    await balloonApi.stats(7);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/contests/7/balloons?status=CLAIMED');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/contests/7/balloons/stats');
  });

  it('sends optimistic versions to every task mutation', async () => {
    await balloonApi.claim(9, 2);
    await balloonApi.deliver(9, 3);
    await balloonApi.cancel(9, 4, 'duplicate');
    await balloonApi.reopen(9, 5);
    await balloonApi.note(9, 6, 'north gate');
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/balloons/9/claim', {
      method: 'POST',
      body: { expectedVersion: 2 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/balloons/9/deliver', {
      method: 'POST',
      body: { expectedVersion: 3 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(3, '/api/balloons/9/cancel', {
      method: 'POST',
      body: { expectedVersion: 4, reason: 'duplicate' },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(4, '/api/balloons/9/reopen', {
      method: 'POST',
      body: { expectedVersion: 5 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(5, '/api/balloons/9/note', {
      method: 'PATCH',
      body: { expectedVersion: 6, note: 'north gate' },
    });
  });
});
