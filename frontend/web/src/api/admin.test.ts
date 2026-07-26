import { beforeEach, describe, expect, it, vi } from 'vitest';
import { adminApi } from './admin';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('Rust admin API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('reads degraded readiness from the public Rust health route', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ status: 'down', service: 'xcpc-platform' }, 503));

    await expect(adminApi.getHealth()).resolves.toMatchObject({ status: 'down' });
    expect(fetch).toHaveBeenCalledWith(
      '/api/health',
      expect.not.objectContaining({ acceptedStatuses: expect.anything() }),
    );
  });

  it('encodes Rust audit pagination and camelCase filters', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ content: [] }));

    await adminApi.listAuditLogs({
      page: 2,
      actorUserId: 88,
      action: 'staff account',
      result: 'success',
      from: '2026-07-20T00:00:00.000Z',
      to: '2026-07-20T08:00:00.000Z',
    });

    const url = new URL(String(vi.mocked(fetch).mock.calls[0][0]), 'http://localhost');
    expect(url.pathname).toBe('/api/admin/audit-logs');
    expect(Object.fromEntries(url.searchParams)).toEqual({
      page: '2',
      size: '25',
      sort: 'createdAt,desc',
      actorUserId: '88',
      action: 'staff account',
      result: 'success',
      from: '2026-07-20T00:00:00.000Z',
      to: '2026-07-20T08:00:00.000Z',
    });
  });

  it('loads only manageable contest pages for management selectors', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ content: [{ id: 1 }], totalPages: 2 }))
      .mockResolvedValueOnce(jsonResponse({ content: [{ id: 2 }], totalPages: 2 }));

    const contests = await adminApi.listAllManageableContests();

    expect(contests.map((contest) => contest.id)).toEqual([1, 2]);
    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/contests?page=0&size=500&sort=updatedAt%2Cdesc&manageableOnly=true',
      '/api/contests?page=1&size=500&sort=updatedAt%2Cdesc&manageableOnly=true',
    ]);
  });

  it('uses exact staff and contest-scope bodies including reset password', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ id: 9 }))
      .mockResolvedValueOnce(jsonResponse({ id: 9 }))
      .mockResolvedValueOnce(jsonResponse({ id: 9 }))
      .mockResolvedValueOnce(jsonResponse({ userId: 9, contestIds: [2, 4] }));

    await adminApi.createStaffAccount({
      username: 'judge-01',
      displayName: 'Judge 01',
      userType: 'JUDGE',
      initialPassword: 'temporary123',
    });
    await adminApi.updateStaffAccount(9, { displayName: 'Chief Judge', enabled: false });
    await adminApi.resetStaffPassword(9, 'reset-password');
    await adminApi.updateContestAdminScope(9, [2, 4]);

    expect(fetchMock.mock.calls.slice(1).map(([url]) => url)).toEqual([
      '/api/admin/staff-accounts',
      '/api/admin/staff-accounts/9',
      '/api/admin/staff-accounts/9/reset-password',
      '/api/admin/contest-admins/9/contests',
    ]);
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({
        username: 'judge-01',
        displayName: 'Judge 01',
        userType: 'JUDGE',
        initialPassword: 'temporary123',
      }),
    }));
    expect(fetchMock.mock.calls[2][1]).toEqual(expect.objectContaining({
      method: 'PATCH',
      body: JSON.stringify({ displayName: 'Chief Judge', enabled: false }),
    }));
    expect(fetchMock.mock.calls[3][1]).toEqual(expect.objectContaining({
      body: JSON.stringify({ newPassword: 'reset-password' }),
    }));
    expect(fetchMock.mock.calls[4][1]).toEqual(expect.objectContaining({
      method: 'PUT',
      body: JSON.stringify({ contestIds: [2, 4] }),
    }));
  });
});
