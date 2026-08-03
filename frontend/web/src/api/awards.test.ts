import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { awardsApi, type AwardCategoryPayload } from './awards';

vi.mock('./client', () => ({ apiRequest: vi.fn() }));
const payload: AwardCategoryPayload = {
  code: 'GOLD',
  name: '金奖',
  displayOrder: 1,
  includeStar: false,
  groupName: null,
  participationType: 'OFFICIAL',
  firstBlood: false,
  rule: { ruleType: 'FIXED_COUNT', fixedCount: 3, ratio: null, rankFrom: null, rankTo: null },
};

describe('awardsApi', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());

  it('uses optimistic versions for category mutations', async () => {
    await awardsApi.updateCategory(4, 2, payload);
    await awardsApi.deleteCategory(4, 3);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/award-categories/4', {
      method: 'PUT',
      body: { expectedVersion: 2, ...payload },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/award-categories/4', {
      method: 'DELETE',
      body: { expectedVersion: 3 },
    });
  });

  it('selects only completed official Resolver runs and explicit snapshot candidates', async () => {
    await awardsApi.completedRuns(7);
    await awardsApi.candidates(7);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/contests/7/awards/resolver-runs');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/contests/7/awards/candidates');
  });

  it('versions manual changes and freeze transitions', async () => {
    await awardsApi.addRecipient(7, 2, 8, 5);
    await awardsApi.removeRecipient(11, 6);
    await awardsApi.freeze(7, 7);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/admin/contests/7/awards/manual', {
      method: 'POST',
      body: { categoryId: 2, teamId: 8, expectedSetVersion: 5 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/admin/award-recipients/11', {
      method: 'DELETE',
      body: { expectedVersion: 6 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(3, '/api/admin/contests/7/awards/freeze', {
      method: 'POST',
      body: { expectedVersion: 7 },
    });
  });

  it('uses compatible presentation and host-script endpoints', async () => {
    await awardsApi.presentation(7);
    await awardsApi.updatePresentation(7, {
      currentCategoryId: 2,
      status: 'PRESENTING',
      autoRotate: true,
      intervalSeconds: 15,
    });
    await awardsApi.hostScript(7);
    await awardsApi.saveHostScript(7, {
      openingText: 'open',
      closingText: 'close',
      sections: [],
      expectedVersion: 3,
    });
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/public/contests/7/awards/presentation');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/contests/7/awards/presentation', {
      method: 'PUT',
      body: { currentCategoryId: 2, status: 'PRESENTING', autoRotate: true, intervalSeconds: 15 },
    });
    expect(apiRequest).toHaveBeenNthCalledWith(3, '/api/contests/7/awards/host-script');
    expect(apiRequest).toHaveBeenNthCalledWith(4, '/api/contests/7/awards/host-script', {
      method: 'PUT',
      body: { openingText: 'open', closingText: 'close', sections: [], expectedVersion: 3 },
    });
  });

  it('exports immutable certificate data through the Java-compatible endpoint', async () => {
    await awardsApi.certificates(7);
    expect(apiRequest).toHaveBeenCalledWith('/api/contests/7/awards/certificates/export', {
      responseType: 'blob',
      headers: { Accept: 'text/csv' },
    });
  });
});
