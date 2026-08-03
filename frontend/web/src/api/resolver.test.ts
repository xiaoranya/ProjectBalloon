import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { resolverApi } from './resolver';

vi.mock('./client', () => ({ apiRequest: vi.fn() }));

describe('resolverApi', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());

  it('recovers contest runs and their latest compatible source snapshots', async () => {
    await resolverApi.list(7);
    await resolverApi.sources(7);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/contests/7/resolver-runs');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/contests/7/resolver-sources');
  });

  it('creates a run from explicit immutable snapshot identifiers', async () => {
    await resolverApi.create(7, 11, 12, true);
    expect(apiRequest).toHaveBeenCalledWith('/api/admin/contests/7/resolver-runs', {
      method: 'POST',
      body: { publicSnapshotId: 11, finalSnapshotId: 12, official: true },
    });
  });

  it('sends the current optimistic version for commands and auto-play', async () => {
    await resolverApi.next(9, 3);
    await resolverApi.autoPlay(9, 4, true, 2500);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/resolver-runs/9/next', {
      method: 'POST',
      body: { expectedVersion: 3 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/resolver-runs/9/auto-play', {
      method: 'POST',
      body: { expectedVersion: 4, enabled: true, intervalMilliseconds: 2500 },
    });
  });

  it('uses the public state endpoint without an admin path', async () => {
    await resolverApi.publicState(9);
    expect(apiRequest).toHaveBeenCalledWith('/api/public/resolver-runs/9/state');
  });
});
