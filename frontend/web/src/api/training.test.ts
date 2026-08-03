import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearCsrfToken } from './client';
import { trainingApi } from './training';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const csrf = { headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' };

describe('training API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('builds public problem-bank filters and preserves nullable publication data', async () => {
    const page = {
      content: [
        {
          id: 4,
          slug: 'private-draft',
          title: 'Draft',
          statement: null,
          difficulty: null,
          tags: [],
          publishedAt: null,
        },
      ],
      page: 2,
      size: 25,
      totalElements: 1,
      totalPages: 1,
    };
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(page));

    const result = await trainingApi.problemBank(2, 25, 'graph basics', 3);

    expect(fetch).toHaveBeenCalledWith(
      '/api/public/problem-bank?page=2&size=25&tag=graph+basics&difficulty=3',
      expect.objectContaining({ method: 'GET' }),
    );
    expect(result.content[0].publishedAt).toBeNull();
  });

  it('submits source as multipart data with the correct metadata and extension', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(
        jsonResponse(
          { submissionId: 8, judgementId: 'j-8', status: 'PENDING', submittedAt: '' },
          201,
        ),
      );

    await trainingApi.submit(4, 'cpp', 'int main() {}', 12, 31);

    const request = vi.mocked(fetch).mock.calls[1][1] as RequestInit;
    expect(vi.mocked(fetch).mock.calls[1][0]).toBe('/api/practice/submissions');
    expect(request.method).toBe('POST');
    expect((request.headers as Headers).get('X-XSRF-TOKEN')).toBe('token');
    expect((request.headers as Headers).get('Content-Type')).toBeNull();
    expect(request.body).toBeInstanceOf(FormData);
    const form = request.body as FormData;
    expect(JSON.parse(String(form.get('metadata')))).toEqual({
      problemId: 4,
      language: 'cpp',
      trainingEnrollmentId: 12,
      virtualSessionId: 31,
    });
    expect((form.get('source') as File).name).toBe('Main.cpp');
  });

  it('uses the output ZIP extension and omits optional submission identifiers', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockResolvedValueOnce(
        jsonResponse(
          { submissionId: 9, judgementId: 'j-9', status: 'PENDING', submittedAt: '' },
          201,
        ),
      );

    await trainingApi.submit(5, 'output', 'zip-bytes');

    const form = vi.mocked(fetch).mock.calls[1][1]!.body as FormData;
    expect(JSON.parse(String(form.get('metadata')))).toEqual({ problemId: 5, language: 'output' });
    expect((form.get('source') as File).name).toBe('Main.zip');
  });

  it('keeps mutation routes, JSON bodies, and CSRF behavior aligned with the Rust API', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(csrf))
      .mockImplementation(async () => jsonResponse({}));

    await trainingApi.enroll(7);
    await trainingApi.favorite(4, true);
    await trainingApi.archiveVirtualSession(11);
    await trainingApi.createVirtualSession({
      title: 'Practice',
      durationMinutes: 90,
      problemIds: [4, 5],
    });
    await trainingApi.updatePracticeSettings({
      dailySubmissionLimit: 20,
      concurrentJudgingLimit: 2,
      sourceRetentionDays: 30,
    });
    await trainingApi.saveEditorial(4, 'zh-CN', {
      title: '题解',
      body: '# 解法',
      unlockPolicy: 'AFTER_ATTEMPT',
      published: true,
    });

    const calls = vi.mocked(fetch).mock.calls.slice(1);
    expect(calls.map(([url]) => url)).toEqual([
      '/api/training/sets/7/enroll',
      '/api/practice/problems/4/favorite',
      '/api/practice/virtual-sessions/11/archive',
      '/api/practice/virtual-sessions',
      '/api/admin/practice/settings',
      '/api/admin/problems/4/editorials/zh-CN',
    ]);
    expect(
      calls.every(([, options]) => (options?.headers as Headers).get('X-XSRF-TOKEN') === 'token'),
    ).toBe(true);
    expect(calls[1][1]).toEqual(
      expect.objectContaining({ body: JSON.stringify({ favorite: true }) }),
    );
    expect(calls[3][1]).toEqual(
      expect.objectContaining({
        body: JSON.stringify({ title: 'Practice', durationMinutes: 90, problemIds: [4, 5] }),
      }),
    );
  });
});
