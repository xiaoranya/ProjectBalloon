import { beforeEach, describe, expect, it, vi } from 'vitest';
import { bulkRejudgeApi, type BatchRejudgeFilter } from './bulk-rejudge';
import { ApiError, clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const filter: BatchRejudgeFilter = {
  problemId: 3,
  teamId: 8,
  language: 'cpp',
  verdict: 'WRONG_ANSWER',
  submittedFrom: '2026-07-20T08:00:00.000Z',
  submittedTo: '2026-07-20T12:00:00.000Z',
};

const csrf = { headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' };
const task = {
  id: 5,
  contestId: 42,
  status: 'RUNNING',
  totalItems: 2,
  processedItems: 1,
  succeededItems: 1,
  failedItems: 0,
  cancelRequested: false,
  createdByUserId: 7,
  startedAt: null,
  completedAt: null,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:00:00Z',
  items: [],
  itemsTruncated: false,
};

describe('Rust contest-scoped bulk rejudge API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('previews with the contest in the URL and only camelCase filter fields in the body', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse({ matchedSubmissions: 2 }));

    await bulkRejudgeApi.preview(42, filter);

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/admin/contests/42/rejudge-tasks/preview',
      expect.objectContaining({ method: 'POST', body: JSON.stringify(filter) }),
    ]);
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[1][1]?.body))).not.toHaveProperty(
      'contestId',
    );
  });

  it('creates with the exact count snapshot, confirmation text, and idempotency key', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse(task, 202));
    const payload = {
      filter,
      expectedCount: 2,
      confirmationText: 'REJUDGE 2',
      idempotencyKey: 'batch-rejudge-42-operation-one',
    };

    await bulkRejudgeApi.create(42, payload);

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/admin/contests/42/rejudge-tasks',
      expect.objectContaining({ method: 'POST', body: JSON.stringify(payload) }),
    ]);
  });

  it('lists and gets tasks from contest-scoped URLs without pagination', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse([task]))
      .mockResolvedValueOnce(jsonResponse(task));

    await bulkRejudgeApi.list(42);
    await bulkRejudgeApi.get(42, 5);

    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/admin/contests/42/rejudge-tasks',
      '/api/admin/contests/42/rejudge-tasks/5',
    ]);
  });

  it('posts pause and resume with no expectedVersion body', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse({ ...task, status: 'PAUSED', cancelRequested: true }))
      .mockResolvedValueOnce(jsonResponse(task));

    await bulkRejudgeApi.pause(42, 5);
    await bulkRejudgeApi.resume(42, 5);

    expect(vi.mocked(fetch).mock.calls[1]).toEqual([
      '/api/admin/contests/42/rejudge-tasks/5/pause',
      expect.objectContaining({ method: 'POST', body: undefined }),
    ]);
    expect(vi.mocked(fetch).mock.calls[2]).toEqual([
      '/api/admin/contests/42/rejudge-tasks/5/resume',
      expect.objectContaining({ method: 'POST', body: undefined }),
    ]);
  });

  it.each([
    ['BATCH_REJUDGE_COUNT_CHANGED', 'preview stale'],
    ['IDEMPOTENCY_KEY_REUSED', 'idempotency conflict'],
  ])('preserves Rust %s conflicts for UI handling (%s)', async (code) => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(jsonResponse({ code, message: code, fieldErrors: [] }, 409));

    await expect(
      bulkRejudgeApi.create(42, {
        filter,
        expectedCount: 2,
        confirmationText: 'REJUDGE 2',
        idempotencyKey: 'batch-rejudge-42-operation-one',
      }),
    ).rejects.toMatchObject({ status: 409, code } satisfies Partial<ApiError>);
  });
});
