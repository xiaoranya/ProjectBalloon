import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, apiRequest, clearCsrfToken, getErrorMessage, setUnauthorizedHandler } from './client';

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('apiRequest', () => {
  beforeEach(() => {
    clearCsrfToken();
    setUnauthorizedHandler(() => undefined);
    vi.stubGlobal('fetch', vi.fn());
  });

  it('fetches a CSRF token and attaches it to mutation requests', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'csrf-token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiRequest('/api/example', { method: 'POST', body: { value: 1 } });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const options = fetchMock.mock.calls[1][1] as RequestInit;
    expect(new Headers(options.headers).get('X-XSRF-TOKEN')).toBe('csrf-token');
    expect(new Headers(options.headers).get('Content-Type')).toBe('application/json');
    expect(options.body).toBe(JSON.stringify({ value: 1 }));
  });

  it('preserves FormData without forcing a content type', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(
        jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'csrf-token' }),
      )
      .mockResolvedValueOnce(jsonResponse({ submissionId: 42, judgementId: 'judge-id' }, 202));
    const form = new FormData();
    form.append('source', new File(['int main(){}'], 'main.cpp'));

    await apiRequest('/api/contests/1/submissions', { method: 'POST', body: form });

    const options = fetchMock.mock.calls[1][1] as RequestInit;
    expect(options.body).toBe(form);
    expect(new Headers(options.headers).has('Content-Type')).toBe(false);
  });

  it('only accepts an explicitly allowed non-success status', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(jsonResponse({ status: 'down' }, 503));

    await expect(apiRequest('/api/health', { acceptedStatuses: [503] })).resolves.toEqual({ status: 'down' });
    expect(fetchMock.mock.calls[0][1]).not.toHaveProperty('acceptedStatuses');

    fetchMock.mockResolvedValueOnce(jsonResponse({ status: 'down' }, 503));
    await expect(apiRequest('/api/health')).rejects.toMatchObject({ status: 503 });
  });

  it('normalizes structured API errors and runs the unauthorized handler', async () => {
    const unauthorized = vi.fn();
    setUnauthorizedHandler(unauthorized);
    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ code: 'SESSION_EXPIRED', message: '登录已失效' }, 401),
    );

    const promise = apiRequest('/api/auth/me');

    await expect(promise).rejects.toMatchObject({
      status: 401,
      code: 'SESSION_EXPIRED',
      message: '登录已失效',
    } satisfies Partial<ApiError>);
    expect(unauthorized).toHaveBeenCalledOnce();
  });

  it('runs the unauthorized handler when CSRF acquisition reports an expired session', async () => {
    const unauthorized = vi.fn();
    setUnauthorizedHandler(unauthorized);
    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ code: 'SESSION_EXPIRED', message: '登录已失效' }, 401),
    );

    await expect(apiRequest('/api/example', { method: 'POST' })).rejects.toMatchObject({ status: 401 });
    expect(unauthorized).toHaveBeenCalledOnce();
  });

  it('translates submission business errors for contestants', () => {
    expect(getErrorMessage(new ApiError(409, 'CONTEST_NOT_RUNNING', 'CONTEST_NOT_RUNNING'))).toBe(
      '比赛当前不接受提交',
    );
    expect(getErrorMessage(new ApiError(400, 'BAD_REQUEST', 'SOURCE_EXTENSION_MISMATCH'))).toBe(
      '源码扩展名与所选语言不匹配',
    );
    expect(getErrorMessage(new ApiError(400, 'BAD_REQUEST', 'SOURCE_TOO_LARGE'))).toBe(
      '源码文件不能超过 64 KiB',
    );
  });
});
