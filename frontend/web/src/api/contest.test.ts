import { beforeEach, describe, expect, it, vi } from 'vitest';
import { contestApi } from './contest';
import { clearCsrfToken } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('contest API contract', () => {
  beforeEach(() => {
    clearCsrfToken();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('uses contest-scoped submission detail URLs', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ id: 9 }));

    await contestApi.getSubmission(7, 9);

    expect(fetch).toHaveBeenCalledWith(
      '/api/contests/7/submissions/9',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('uses the Rust contest problem endpoint instead of the legacy overview endpoint', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse([]));

    await contestApi.listProblems(7, 'zh-CN');

    expect(fetch).toHaveBeenCalledWith(
      '/api/contests/7/problems?lang=zh-CN',
      expect.any(Object),
    );
  });
});
