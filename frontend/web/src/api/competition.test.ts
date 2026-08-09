import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { competitionApi } from './competition';

vi.mock('./client', () => ({ apiRequest: vi.fn() }));

describe('competition API', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());

  it('uses contest-scoped workstation binding endpoints', async () => {
    await competitionApi.workstations();
    await competitionApi.bind(23, 5, 11);
    await competitionApi.rotate(23, 7);
    await competitionApi.revoke(23, 7);

    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/competition/workstations');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/contests/23/workstation-bindings', {
      method: 'POST',
      body: { workstationId: 5, teamId: 11 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(
      3,
      '/api/admin/contests/23/workstation-bindings/7/rotate',
      { method: 'POST' },
    );
    expect(apiRequest).toHaveBeenNthCalledWith(4, '/api/admin/contests/23/workstation-bindings/7', {
      method: 'DELETE',
    });
  });
});
