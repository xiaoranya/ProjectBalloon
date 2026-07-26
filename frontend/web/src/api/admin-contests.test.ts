import { beforeEach, describe, expect, it, vi } from 'vitest';
import { adminContestApi } from './admin-contests';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('Rust contest administration API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('uses the administrator submission URL and camelCase filters', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ content: [] }));

    await adminContestApi.listSubmissions(42, {
      page: 2,
      size: 30,
      teamId: 8,
      problemId: 7,
      status: 'ACCEPTED',
      language: 'cpp',
    });

    expect(fetch).toHaveBeenCalledWith(
      '/api/admin/contests/42/submissions?page=2&size=30&sort=submittedAt%2Cdesc&teamId=8&problemId=7&status=ACCEPTED&language=cpp',
      expect.any(Object),
    );
  });

  it('preserves the Java-compatible contest judge queue status route', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ contestId: 42, drained: true }));

    await adminContestApi.getJudgeQueueStatus(42);

    expect(vi.mocked(fetch).mock.calls[0][0]).toBe(
      '/api/admin/contests/42/judge-queue/status',
    );
  });

  it('passes expectedJudgementId to the contest-scoped single rejudge route', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ submissionId: 9, status: 'PENDING' }, 202));

    await adminContestApi.rejudgeSubmission(42, 9, '11111111-1111-1111-1111-111111111111');

    expect(fetchMock.mock.calls[1][0]).toBe('/api/admin/contests/42/submissions/9/rejudge');
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ expectedJudgementId: '11111111-1111-1111-1111-111111111111' }),
    }));
  });

  it('uses the exact Rust export URLs', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(new Response(new Blob(['scoreboard']), { status: 200 }))
      .mockResolvedValueOnce(new Response(new Blob(['submissions']), { status: 200 }))
      .mockResolvedValueOnce(new Response(new Blob(['sources']), { status: 200 }));

    await adminContestApi.exportScoreboard(42);
    await adminContestApi.exportSubmissions(42);
    await adminContestApi.exportSubmissionSources(42);

    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/admin/contests/42/scoreboard.csv',
      '/api/admin/contests/42/exports/submissions.csv',
      '/api/admin/contests/42/exports/submission-sources.zip',
    ]);
  });

  it('sends lifecycle and extension concurrency contracts in camelCase', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ contestId: 42, to: 'RUNNING' }))
      .mockResolvedValueOnce(jsonResponse({ contestId: 42, endAt: '2026-07-20T14:00:00Z' }));

    await adminContestApi.transitionContest(42, 'RUNNING');
    await adminContestApi.extendContest(
      42,
      '2026-07-20T12:00:00Z',
      '2026-07-20T14:00:00Z',
    );

    expect(fetchMock.mock.calls[1][0]).toBe('/api/contests/42/transitions');
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({ body: JSON.stringify({ to: 'RUNNING' }) }));
    expect(fetchMock.mock.calls[2][0]).toBe('/api/contests/42/extensions');
    expect(fetchMock.mock.calls[2][1]).toEqual(expect.objectContaining({
      body: JSON.stringify({
        expectedEndAt: '2026-07-20T12:00:00Z',
        newEndAt: '2026-07-20T14:00:00Z',
      }),
    }));
  });

  it('uses the Java-compatible contest clone route', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' })).mockResolvedValueOnce(jsonResponse({ sourceContestId: 42, problemsCopied: 2, teamsCopied: 3 }, 201));
    await adminContestApi.cloneContest(42, { name: 'Copy', visibility: 'PRIVATE', startAt: null, freezeAt: null, endAt: null, copyTeams: true });
    expect(fetchMock.mock.calls[1][0]).toBe('/api/contests/42/clones');
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({ method: 'POST', body: JSON.stringify({ name: 'Copy', visibility: 'PRIVATE', startAt: null, freezeAt: null, endAt: null, copyTeams: true }) }));
  });

  it('caps each problem request at the Rust size limit and omits unsupported sorting', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ content: [] }));

    await adminContestApi.listProblems(0, 500, 42);

    expect(fetch).toHaveBeenCalledWith(
      '/api/problems?page=0&size=100&contestId=42',
      expect.any(Object),
    );
  });

  it('loads every problem page without increasing the per-request size', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({
        content: [{ id: 1, title: 'First' }],
        page: 0,
        size: 100,
        totalElements: 201,
        totalPages: 3,
      }))
      .mockResolvedValueOnce(jsonResponse({
        content: [{ id: 101, title: 'Second page' }],
        page: 1,
        size: 100,
        totalElements: 201,
        totalPages: 3,
      }))
      .mockResolvedValueOnce(jsonResponse({
        content: [{ id: 201, title: 'Third page' }],
        page: 2,
        size: 100,
        totalElements: 201,
        totalPages: 3,
      }));

    const problems = await adminContestApi.listAllProblems(42);

    expect(problems.map((problem) => problem.id)).toEqual([1, 101, 201]);
    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/problems?page=0&size=100&contestId=42',
      '/api/problems?page=1&size=100&contestId=42',
      '/api/problems?page=2&size=100&contestId=42',
    ]);
  });

  it('updates an assigned problem through the contest-scoped PATCH route', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse({ contestId: 42, problemId: 7, alias: 'B' }));

    await adminContestApi.updateProblemAssignment(42, 7, {
      alias: 'B',
      displayOrder: 2,
      color: '#ff0000',
    });

    expect(fetchMock.mock.calls[1][0]).toBe('/api/contests/42/problems/7');
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({
      method: 'PATCH',
      body: JSON.stringify({ alias: 'B', displayOrder: 2, color: '#ff0000' }),
    }));
  });

  it('sends the complete reorder array to the Rust PUT route', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }))
      .mockResolvedValueOnce(jsonResponse([]));
    const entries = [{ problemId: 3, displayOrder: 1 }, { problemId: 5, displayOrder: 2 }];

    await adminContestApi.reorderProblems(42, entries);

    expect(fetchMock.mock.calls[1][0]).toBe('/api/contests/42/problems/reorder');
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({ method: 'PUT', body: JSON.stringify(entries) }));
  });
});
