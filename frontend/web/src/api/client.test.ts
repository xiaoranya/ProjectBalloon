import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  apiRequest,
  clearCsrfToken,
  getErrorMessage,
  setUnauthorizedHandler,
} from './client';

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

    await expect(apiRequest('/api/health', { acceptedStatuses: [503] })).resolves.toEqual({
      status: 'down',
    });
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

    await expect(apiRequest('/api/example', { method: 'POST' })).rejects.toMatchObject({
      status: 401,
    });
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

  it('falls back to the server message when the business code is unknown', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ code: 'FUTURE_UNKNOWN_CODE', message: '后端的新提示' }, 409),
    );

    const error = await apiRequest('/api/example').catch((reason: unknown) => reason);

    expect(getErrorMessage(error)).toBe('后端的新提示');
  });

  it('falls back to the original text when neither code nor message is known', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(
      new Response('upstream exploded', { status: 502, headers: { 'content-type': 'text/plain' } }),
    );

    const error = await apiRequest('/api/example').catch((reason: unknown) => reason);

    expect(error).toMatchObject({ code: 'HTTP_502' });
    expect(getErrorMessage(error)).toBe('upstream exploded');
  });

  it('keeps the status default message when the body carries no message', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(jsonResponse({}, 429));

    const error = await apiRequest('/api/example').catch((reason: unknown) => reason);

    expect(error).toMatchObject({ code: 'HTTP_429' });
    expect(getErrorMessage(error)).toBe('操作过于频繁，请稍后重试');
  });

  it('suppresses the unauthorized handler when the caller opts out', async () => {
    const unauthorized = vi.fn();
    setUnauthorizedHandler(unauthorized);
    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ code: 'SESSION_EXPIRED', message: '登录已失效' }, 401),
    );

    await expect(
      apiRequest('/api/example', { suppressUnauthorizedHandler: true }),
    ).rejects.toMatchObject({ status: 401 });
    expect(unauthorized).not.toHaveBeenCalled();
  });

  it('describes non-Error rejection values', () => {
    expect(getErrorMessage('boom')).toBe('发生未知错误');
  });

  it('returns undefined for a 204 response without parsing a body', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));

    await expect(apiRequest('/api/example')).resolves.toBeUndefined();
  });

  it('reads plain text when responseType is text', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(
      new Response('plain output', {
        status: 200,
        headers: { 'content-type': 'text/plain' },
      }),
    );

    await expect(apiRequest<string>('/api/example', { responseType: 'text' })).resolves.toBe(
      'plain output',
    );
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(new Headers(options.headers).get('Accept')).toBe('text/plain');
  });

  it('reads binary data when responseType is blob', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(
      new Response(new Blob(['zip-bytes']), {
        status: 200,
        headers: { 'content-type': 'application/octet-stream' },
      }),
    );

    const blob = await apiRequest<Blob>('/api/example', { responseType: 'blob' });
    expect(await blob.text()).toBe('zip-bytes');
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(new Headers(options.headers).get('Accept')).toBe('application/octet-stream');
  });

  it('keeps the caller-supplied Accept header untouched', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiRequest('/api/example', { headers: { Accept: 'text/csv' } });

    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(new Headers(options.headers).get('Accept')).toBe('text/csv');
  });

  it('fetches the CSRF token only once for concurrent mutations', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockImplementation((input: RequestInfo | URL) => {
      if (String(input) === '/api/auth/csrf') {
        return new Promise<Response>((resolve) =>
          setTimeout(
            () =>
              resolve(
                jsonResponse({
                  headerName: 'X-XSRF-TOKEN',
                  parameterName: '_csrf',
                  token: 'csrf-token',
                }),
              ),
            5,
          ),
        );
      }
      return Promise.resolve(jsonResponse({ ok: true }));
    });

    await Promise.all([
      apiRequest('/api/example', { method: 'POST', body: { n: 1 } }),
      apiRequest('/api/example', { method: 'POST', body: { n: 2 } }),
      apiRequest('/api/example', { method: 'PUT', body: { n: 3 } }),
    ]);

    const csrfCalls = fetchMock.mock.calls.filter(([input]) => String(input) === '/api/auth/csrf');
    expect(csrfCalls).toHaveLength(1);
    for (const [input, options] of fetchMock.mock.calls) {
      if (String(input) === '/api/auth/csrf') continue;
      expect(new Headers((options as RequestInit).headers).get('X-XSRF-TOKEN')).toBe('csrf-token');
    }
  });

  it('re-fetches the CSRF token after a logout clears it', async () => {
    const fetchMock = vi.mocked(fetch);
    const csrfResponse = () =>
      jsonResponse({ headerName: 'X-XSRF-TOKEN', parameterName: '_csrf', token: 'csrf-token' });
    fetchMock
      .mockResolvedValueOnce(csrfResponse())
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiRequest('/api/example', { method: 'POST' });
    clearCsrfToken();

    fetchMock
      .mockResolvedValueOnce(csrfResponse())
      .mockResolvedValueOnce(jsonResponse({ ok: true }));
    await apiRequest('/api/example', { method: 'POST' });

    const csrfCalls = fetchMock.mock.calls.filter(([input]) => String(input) === '/api/auth/csrf');
    expect(csrfCalls).toHaveLength(2);
  });

  it('does not fetch a CSRF token for GET requests', async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock.mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiRequest('/api/example');

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/example');
  });
});
