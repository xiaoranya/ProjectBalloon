import { beforeEach, describe, expect, it, vi } from 'vitest';
import { announcementApi } from './announcements';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const csrf = { headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' };
const announcement = {
  id: 9,
  contestId: 7,
  title: 'Notice',
  body: 'Body',
  pinned: false,
  status: 'SCHEDULED',
  createdByUserId: 1,
  publishedAt: null,
  scheduledAt: '2026-07-22T10:00:00Z',
  withdrawnAt: null,
  withdrawnByUserId: null,
  sourceClarificationId: null,
  cancelledAt: null,
  cancelledByUserId: null,
  createdAt: '2026-07-22T08:00:00Z',
  updatedAt: '2026-07-22T08:00:00Z',
  version: 0,
};

describe('Rust announcement API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('loads manager history using the contest-scoped array endpoint', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse([announcement]));

    await announcementApi.list(7, true);

    expect(vi.mocked(fetch).mock.calls[0][0]).toBe(
      '/api/contests/7/announcements?includeWithdrawn=true',
    );
  });

  it('creates, reschedules, cancels, pins and withdraws through exact endpoints', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse(announcement, 201))
      .mockResolvedValueOnce(jsonResponse({ ...announcement, version: 1 }))
      .mockResolvedValueOnce(jsonResponse({ ...announcement, status: 'CANCELLED' }))
      .mockResolvedValueOnce(jsonResponse({ ...announcement, pinned: true }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    const payload = {
      title: 'Notice',
      body: 'Body',
      pinned: false,
      scheduledAt: '2026-07-22T10:00:00Z',
    };

    await announcementApi.create(7, payload);
    await announcementApi.schedule(9, payload);
    await announcementApi.cancel(9);
    await announcementApi.pin(9, true);
    await announcementApi.withdraw(9);

    const calls = vi.mocked(fetch).mock.calls.slice(1);
    expect(calls.map(([url]) => url)).toEqual([
      '/api/contests/7/announcements',
      '/api/announcements/9/schedule',
      '/api/announcements/9/cancel',
      '/api/announcements/9/pin',
      '/api/announcements/9/withdraw',
    ]);
    expect(calls[0][1]).toEqual(
      expect.objectContaining({ method: 'POST', body: JSON.stringify(payload) }),
    );
    expect(calls[3][1]).toEqual(
      expect.objectContaining({ body: JSON.stringify({ pinned: true }) }),
    );
  });
});
