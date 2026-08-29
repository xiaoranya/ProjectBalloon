import { beforeEach, describe, expect, it, vi } from 'vitest';
import { adminProblemApi } from './admin-problems';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const problem = {
  id: 7,
  slug: 'two-sum',
  title: 'Two Sum',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['cpp'],
  testdataVersion: 0,
  testdataSha256: null,
  defaultLangCode: 'en',
  createdBy: 1,
  version: 3,
  createdAt: '2026-07-20T00:00:00Z',
  updatedAt: '2026-07-20T00:00:00Z',
};

describe('Rust problem administration API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('caps the Rust page size at 100 and sends no unsupported sort query', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ content: [] }));
    await adminProblemApi.listProblems(2, 500);
    expect(fetch).toHaveBeenCalledWith('/api/problems?page=2&size=100', expect.any(Object));
  });

  it('sends required expectedVersion in the PATCH body', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 4 }));

    await adminProblemApi.updateProblem(7, { expectedVersion: 3, title: 'Three Sum' });

    expect(vi.mocked(fetch).mock.calls[1][0]).toBe('/api/problems/7');
    expect(vi.mocked(fetch).mock.calls[1][1]).toEqual(
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ expectedVersion: 3, title: 'Three Sum' }),
      }),
    );
  });

  it('reads publication status and PUTs visibility to the admin publication endpoint', async () => {
    const publication = { visibility: 'PRIVATE', difficulty: null, tags: [], publishedAt: null };
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(publication));
    await adminProblemApi.getPublication(7);
    expect(vi.mocked(fetch).mock.calls[0][0]).toBe('/api/admin/problems/7/publication');

    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          visibility: 'PUBLIC',
          difficulty: 3,
          tags: ['dp'],
          publishedAt: '2026-08-29T00:00:00Z',
        }),
      );

    const published = await adminProblemApi.updatePublication(7, {
      visibility: 'PUBLIC',
      difficulty: 3,
      tags: ['dp'],
    });

    expect(vi.mocked(fetch).mock.calls[2][0]).toBe('/api/admin/problems/7/publication');
    expect(vi.mocked(fetch).mock.calls[2][1]).toEqual(
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ visibility: 'PUBLIC', difficulty: 3, tags: ['dp'] }),
      }),
    );
    expect(published.visibility).toBe('PUBLIC');
  });

  it('uses exact attachment multipart fields and refreshes the Problem afterward', async () => {
    const attachment = { id: 11, problemId: 7, kind: 'SAMPLE', originalFilename: 'sample.txt' };
    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(jsonResponse(attachment, 201))
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 4 }));
    const file = new File(['sample'], 'sample.txt', { type: 'text/plain' });

    const result = await adminProblemApi.uploadAttachment(7, 'SAMPLE', file);

    const options = vi.mocked(fetch).mock.calls[1][1] as RequestInit;
    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/auth/csrf',
      '/api/problems/7/attachments',
      '/api/problems/7',
    ]);
    expect(options.body).toBeInstanceOf(FormData);
    const body = options.body as FormData;
    expect(body.get('kind')).toBe('SAMPLE');
    expect(body.get('file')).toBe(file);
    expect([...body.keys()]).toEqual(['kind', 'file']);
    expect(result.problem?.version).toBe(4);
  });

  it('uses only the file multipart field for test data and refreshes after the incomplete response', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(
        jsonResponse(
          { problemId: 7, version: 1, caseCount: 2, bytes: 20, sha256: 'abc', createdAt: 'now' },
          201,
        ),
      )
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 4, testdataVersion: 1 }));
    const file = new File(['zip'], 'cases.zip', { type: 'application/zip' });

    const result = await adminProblemApi.uploadTestdata(7, file);

    const body = vi.mocked(fetch).mock.calls[1][1]?.body as FormData;
    expect([...body.keys()]).toEqual(['file']);
    expect(body.get('file')).toBe(file);
    expect(vi.mocked(fetch).mock.calls[2][0]).toBe('/api/problems/7');
    expect(result.problem?.testdataVersion).toBe(1);
  });

  it('refreshes the optimistic concurrency version after statement upsert and attachment delete', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ problemId: 7, langCode: 'zh-CN', body: '# 题目' }))
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 4 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 5 }));

    await adminProblemApi.upsertStatement(7, 'zh-CN', '# 题目');
    const deleted = await adminProblemApi.deleteAttachment(7, 11);

    expect(vi.mocked(fetch).mock.calls[1][0]).toBe('/api/problems/7/statements/zh-CN');
    expect(vi.mocked(fetch).mock.calls[1][1]?.body).toBe(JSON.stringify({ body: '# 题目' }));
    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/auth/csrf',
      '/api/problems/7/statements/zh-CN',
      '/api/problems/7',
      '/api/problems/7/attachments/11',
      '/api/problems/7',
    ]);
    expect(deleted.problem?.version).toBe(5);
  });

  it('preserves a committed mutation result when the required version refresh fails', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ problemId: 7, version: 2, caseCount: 1 }, 201))
      .mockRejectedValueOnce(new TypeError('network unavailable'));

    const result = await adminProblemApi.uploadTestdata(7, new File(['zip'], 'cases.zip'));

    expect(result.result.version).toBe(2);
    expect(result.problem).toBeNull();
    expect(result.refreshFailed).toBe(true);
  });

  it('lists, downloads and activates immutable test-data versions', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse([{ problemId: 7, version: 2, active: true }]))
      .mockResolvedValueOnce(new Response(new Blob(['testdata'])))
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ problemId: 7, version: 1, active: true }))
      .mockResolvedValueOnce(jsonResponse({ ...problem, version: 4, testdataVersion: 1 }));

    await adminProblemApi.listTestdataVersions(7);
    await adminProblemApi.downloadTestdataVersion(7, 1);
    const activated = await adminProblemApi.activateTestdataVersion(7, 1, 2);

    expect(vi.mocked(fetch).mock.calls.map(([url]) => url)).toEqual([
      '/api/problems/7/testdata/versions',
      '/api/problems/7/testdata/versions/1',
      '/api/auth/csrf',
      '/api/problems/7/testdata/versions/1/activate',
      '/api/problems/7',
    ]);
    expect(vi.mocked(fetch).mock.calls[3][1]).toEqual(
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ expectedCurrentVersion: 2 }),
      }),
    );
    expect(activated.problem?.testdataVersion).toBe(1);
  });
});
